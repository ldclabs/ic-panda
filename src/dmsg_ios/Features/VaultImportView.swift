import SwiftUI
import UniformTypeIdentifiers

struct VaultImportView: View {
    var pending: PrivateFiles.PendingImport? = nil
    @Environment(SessionModel.self) private var model
    @State private var picking = false
    @State private var staged: URL?
    @State private var handle: String?
    @State private var preview: Preview?
    @State private var selected: Set<UInt32> = []
    @State private var working = false
    @State private var completed = false
    struct Preview: Decodable {
        struct Item: Decodable, Identifiable { let index: UInt32; let title: String; let username: String; let duplicate: Bool; var id: UInt32 { index } }
        let format: String; let digest: String; let skipped: Int; let incomplete: Int; let items: [Item]
    }
    var body: some View {
        List {
            NoticeBar(text: String(localized: "Import an unencrypted 1Password, Bitwarden or browser password export. The source stays on this device; only selected records are added."))
            Button("Choose password export") { picking = true }.disabled(working)
            if working { ProgressView() }
            if completed { Label("Import completed", systemImage: "checkmark.circle") }
            if let preview {
                if preview.skipped > 0 || preview.incomplete > 0 {
                    NoticeBar(text: String(localized: "Some source records or properties are unsupported. This import copies selected fields only; archive attachments, passkeys and history are not imported here."))
                }
                ForEach(preview.items) { item in
                    Toggle(isOn: Binding(get: { selected.contains(item.index) }, set: { if $0 { selected.insert(item.index) } else { selected.remove(item.index) } })) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(item.title).font(.headline)
                            Text(item.username).font(.caption).foregroundStyle(.secondary)
                            if item.duplicate { Text("Already present").font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                }
                Button("Import selected records") { Task { await commit() } }.disabled(working || selected.isEmpty)
            }
        }.privateScreen().navigationTitle("Import passwords")
            .task { if let pending, handle == nil { await resume(pending) } }
            .fileImporter(isPresented: $picking, allowedContentTypes: [.item]) { result in Task { await inspect(result) } }
    }
    private func resume(_ pending: PrivateFiles.PendingImport) async {
        guard pending.ready, pending.purpose == "vault", model.session?.account == pending.account,
              let files = model.files, let engine = model.engine else { return }
        let epoch = model.epoch; working = true; defer { working = false }
        staged = files.staging.appending(path: pending.handle); handle = pending.handle
        do {
            let result = try await coreOperation { try await engine.vaultImportPreview(name: pending.handle, operation: $0) }
            guard epoch == model.epoch, !Task.isCancelled else { return }; preview = try model.decode(result)
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func inspect(_ result: Result<URL, Error>) async {
        guard let files = model.files, let engine = model.engine, let account = model.session?.account else { return }
        let epoch = model.epoch; working = true; defer { working = false }
        preview = nil; selected = []; completed = false
        do {
            let url = try result.get()
            let copy = try await Task.detached { try files.importDocument(url, account: account, purpose: "vault") }.value
            guard epoch == model.epoch else { return }
            staged = copy
            let name = account + "/" + copy.deletingLastPathComponent().lastPathComponent + "/" + copy.lastPathComponent
            handle = name
            let result = try await coreOperation { try await engine.vaultImportPreview(name: name, operation: $0) }
            guard epoch == model.epoch else { return }; preview = try model.decode(result)
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func commit() async {
        guard let preview, let handle, let engine = model.engine, !working else { return }
        let epoch = model.epoch, picks = selected.sorted(); working = true; defer { working = false }
        do {
            _ = try await coreOperation { try await engine.vaultImportSelected(name: handle, digest: preview.digest, picks: picks, operation: $0) }
            if let staged { try model.files?.finishImport(staged) }
            guard epoch == model.epoch else { return }
            self.preview = nil; selected = []; completed = true; await model.updateSession()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
