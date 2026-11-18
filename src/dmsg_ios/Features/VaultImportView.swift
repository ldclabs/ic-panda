import SwiftUI
import UniformTypeIdentifiers
@preconcurrency import DMsgCore

@MainActor @Observable
final class VaultImportReview {
    struct Preview: Decodable {
        struct Item: Decodable, Identifiable { let index: UInt32; let title: String; let username: String; let duplicate: Bool; var id: UInt32 { index } }
        let format: String; let digest: String; let skipped: Int; let incomplete: Int; let items: [Item]
    }

    enum Phase {
        case initial, needsCopy, staged(String), reviewed(String, Preview), imported([String], titles: [String: String])
    }
    private(set) var phase = Phase.initial

    func inspect(stage: () async throws -> String, read: (String) async throws -> Preview) async throws {
        phase = .needsCopy
        let handle = try await stage()
        phase = .staged(handle)
        let preview = try await read(handle)
        try Task.checkCancellation()
        phase = .reviewed(handle, preview)
    }

    func finish(ids: [String], picks: [UInt32]) {
        guard case let .reviewed(_, preview) = phase else { return }
        let names = Dictionary(preview.items.map { ($0.index, $0.title) }, uniquingKeysWith: { first, _ in first })
        let titles = picks.map { names[$0] ?? String(localized: "Open secret") }
        phase = .imported(ids, titles: Dictionary(zip(ids, titles), uniquingKeysWith: { first, _ in first }))
    }
}

struct VaultImportView: View {
    var pending: PrivateFiles.PendingImport? = nil
    @Environment(SessionModel.self) private var model
    @State private var review = VaultImportReview()
    @State private var picking = false
    @State private var selected: Set<UInt32> = []
    @State private var working = false
    @State private var query = ""

    var body: some View {
        List {
            NoticeBar(text: String(localized: "Import an unencrypted 1Password, Bitwarden or browser password export. The source stays on this device; only selected records are added."))
            Button("Choose password export") { picking = true }.disabled(working)
            if working { ProgressView() }
            if case let .imported(imported, titles) = review.phase {
                Label("Import completed", systemImage: "checkmark.circle")
                ForEach(imported, id: \.self) { id in
                    NavigationLink(titles[id] ?? String(localized: "Open secret")) { VaultDetailView(id: id) }
                }
            }
            if case let .staged(handle) = review.phase, !working {
                Button("Retry preview") { Task { await inspect { handle } } }
            }
            if case let .reviewed(_, preview) = review.phase {
                Section {
                    LabeledContent("Total", value: preview.items.count.formatted())
                    LabeledContent("Already present", value: preview.items.filter(\.duplicate).count.formatted())
                    LabeledContent("Skipped", value: preview.skipped.formatted())
                    LabeledContent("Incomplete", value: preview.incomplete.formatted())
                    LabeledContent("Selected", value: selected.count.formatted())
                    Button("Select non-duplicates") { selected = Set(preview.items.filter { !$0.duplicate }.prefix(1000).map(\.index)) }
                    Button("Clear selection") { selected = [] }
                    Text("Select up to 1,000 records per import.").font(.footnote).foregroundStyle(.secondary)
                }
                if preview.skipped > 0 || preview.incomplete > 0 {
                    NoticeBar(text: String(localized: "Some source records or properties are unsupported. This import copies selected fields only; archive attachments, passkeys and history are not imported here."))
                }
                ForEach(preview.items.filter { query.isEmpty || $0.title.localizedCaseInsensitiveContains(query) || $0.username.localizedCaseInsensitiveContains(query) }) { item in
                    Toggle(isOn: Binding(get: { selected.contains(item.index) }, set: { if $0 { selected.insert(item.index) } else { selected.remove(item.index) } })) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(item.title).font(.headline)
                            Text(item.username).font(.caption).foregroundStyle(.secondary)
                            if item.duplicate { Text("Already present").font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                }
                Button("Import selected records") { Task { await commit() } }.disabled(working || selected.isEmpty || selected.count > 1000)
            }
        }.disabled(working).privateScreen().navigationTitle("Import passwords").searchable(text: $query)
            .task {
                if case .initial = review.phase, let pending, pending.ready, pending.purpose == "vault", pending.account == model.session?.account {
                    await inspect { pending.handle }
                }
            }
            .fileImporter(isPresented: $picking, allowedContentTypes: [.item]) { result in
                switch result {
                case .success(let url): Task { await choose(url) }
                case .failure(let error): model.reportFileError(error)
                }
            }
    }

    private func choose(_ url: URL) async {
        guard let files = model.files, let account = model.session?.account else { return }
        await inspect {
            let copy = try await Task.detached { try files.importDocument(url, account: account, purpose: "vault") }.value
            return account + "/" + copy.deletingLastPathComponent().lastPathComponent + "/" + copy.lastPathComponent
        }
    }

    private func inspect(stage: () async throws -> String) async {
        guard !working, let engine = model.engine else { return }
        let epoch = model.epoch; working = true; selected = []; defer { working = false }
        do {
            try await review.inspect(stage: stage) { handle in
                guard epoch == model.epoch, !Task.isCancelled else { throw CancellationError() }
                let result = try await coreOperation { try await engine.vaultImportPreview(name: handle, operation: $0) }
                guard epoch == model.epoch else { throw CancellationError() }
                return try model.decode(result)
            }
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func commit() async {
        guard case let .reviewed(handle, preview) = review.phase, let engine = model.engine, let files = model.files, !working else { return }
        let epoch = model.epoch, picks = selected.sorted(); working = true; defer { working = false }
        do {
            let result = try await coreOperation { try await engine.vaultImportSelected(name: handle, digest: preview.digest, picks: picks, operation: $0) }
            do { try files.finishImport(files.staging.appending(path: handle)) }
            catch { if epoch == model.epoch { model.reportFileError(error) } }
            guard epoch == model.epoch else { return }
            review.finish(ids: try model.decode(result), picks: picks)
            selected = []; await model.updateSession()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
