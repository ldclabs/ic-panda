import SwiftUI
@preconcurrency import DMsgCore

struct VaultHistoryView: View {
    let item: VaultItem
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var versions: [Version] = []
    @State private var restoring: Version?
    @State private var working = false
    @State private var loading = true
    @State private var failed = false
    struct Version: Decodable, Identifiable {
        let revision: String; let at: UInt64; let current: Bool; let deleted: Bool; let conflict: Bool
        let device: String
        let synced: Bool
        var id: String { revision }
    }
    var body: some View {
        List {
          ForEach(versions) { version in
            VStack(alignment: .leading, spacing: 8) {
                NavigationLink { VaultDetailView(id: item.id, revision: version.revision, versionDate: version.at).id(version.revision) } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(Date(timeIntervalSince1970: Double(version.at) / 1000), format: .dateTime)
                        if version.current { Text("Current version").font(.caption).foregroundStyle(.secondary) }
                        if version.deleted { Label("In Trash", systemImage: "trash").font(.caption) }
                        if version.conflict { Label("Sync conflicts", systemImage: "arrow.triangle.branch").font(.caption) }
                        if !version.synced { Text("Waiting to sync").font(.caption) }
                        Text(model.deviceLabel(version.device) ?? version.device).font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                }
                if !version.current && !version.deleted && !item.reserved {
                    NavigationLink("Compare with current") { VaultVersionComparison(itemID: item.id, revision: version.revision) }
                    Button("Restore this version") { restoring = version }.disabled(working)
                }
            }
          }
          if loading { ProgressView() }
          else if failed { Button("Retry") { Task { await load() } } }
          else if versions.isEmpty { ContentUnavailableView("No items", systemImage: "clock") }
        }.privateScreen().navigationTitle("Version history").task(id: model.contentRevision) { await load() }
        .refreshable { await load() }.confirmationDialog("Restore this version as a new current version?", isPresented: Binding(get: { restoring != nil }, set: { if !$0 { restoring = nil } }), titleVisibility: .visible) {
            Button("Restore") { if let version = restoring { Task { await restore(version) } } }
        }
    }

    private func load() async {
        loading = true; failed = false; defer { loading = false }; let epoch = model.epoch
        do {
            guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
            let result = try await engine.vaultHistory(id: item.id)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            versions = try model.decode(result)
        } catch { if epoch == model.epoch && !Task.isCancelled { failed = true } }
    }
    private func restore(_ version: Version) async {
        guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }; let epoch = model.epoch
        do {
            _ = try await engine.revertItem(id: item.id, revision: version.revision, expectedRevision: item.revision)
            guard epoch == model.epoch else { return }
            await model.updateSession(); dismiss()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

private struct VaultVersionComparison: View {
    let itemID: String
    let revision: String
    @Environment(SessionModel.self) private var model
    @State private var current: VaultItem?
    @State private var previous: VaultItem?
    @State private var failed = false
    var body: some View {
        List {
            if let current, let previous {
                Section("Title") { comparison(current.title, previous.title) }
                Section("Tags") { comparison(current.tags.joined(separator: ", "), previous.tags.joined(separator: ", ")) }
                Section("Notes") { comparison(current.notes, previous.notes) }
                ForEach(Array(Set(current.fields.map(\.id) + previous.fields.map(\.id))).sorted(), id: \.self) { id in
                    let a = current.fields.first { $0.id == id }, b = previous.fields.first { $0.id == id }
                    Section(a.map(vaultFieldTitle) ?? b.map(vaultFieldTitle) ?? id) {
                        if a?.concealed == true || b?.concealed == true {
                            Text("Your hidden fields stay concealed until you choose to reveal them.").font(.footnote).foregroundStyle(.secondary)
                        } else { comparison(a?.value ?? "—", b?.value ?? "—") }
                    }
                }
                NavigationLink("Current version") { VaultDetailView(id: itemID) }
                NavigationLink("Historical version") { VaultDetailView(id: itemID, revision: revision) }
            } else if failed { Button("Retry") { Task { await load() } } }
            else { ProgressView() }
        }.privateScreen().navigationTitle("Compare with current").task { await load() }
    }
    private func comparison(_ current: String, _ previous: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            LabeledContent("Current version", value: current)
            LabeledContent("Historical version", value: previous)
        }.foregroundStyle(current == previous ? Color.secondary : Color.primary)
    }
    private func load() async {
        failed = false; let epoch = model.epoch
        do {
            let current = try await model.readVaultItem(id: itemID)
            let previous = try await model.readVaultItem(id: itemID, revision: revision)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            self.current = current; self.previous = previous
        } catch { if epoch == model.epoch && !Task.isCancelled { failed = true } }
    }
}
