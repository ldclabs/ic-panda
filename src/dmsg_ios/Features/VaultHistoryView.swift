import SwiftUI
@preconcurrency import DMsgCore

struct VaultHistoryView: View {
    let item: VaultItem
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var versions: [Version] = []
    @State private var restoring: Version?
    @State private var working = false
    struct Version: Decodable, Identifiable {
        let revision: String; let at: UInt64; let current: Bool; let deleted: Bool; let conflict: Bool
        var id: String { revision }
    }
    var body: some View {
        List(versions) { version in
            VStack(alignment: .leading, spacing: 8) {
                NavigationLink { VaultDetailView(id: item.id, revision: version.revision).id(version.revision) } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(Date(timeIntervalSince1970: Double(version.at) / 1000), format: .dateTime)
                        if version.current { Text("Current version").font(.caption).foregroundStyle(.secondary) }
                        if version.deleted { Label("In Trash", systemImage: "trash").font(.caption) }
                    }
                }
                if !version.current && !version.deleted && !item.reserved {
                    Button("Restore this version") { restoring = version }.disabled(working)
                }
            }
        }.privateScreen().navigationTitle("Version history").task {
            do { if let result = try await model.engine?.vaultHistory(id: item.id) { versions = try model.decode(result) } }
            catch { model.report(error) }
        }.confirmationDialog("Restore this version as a new current version?", isPresented: Binding(get: { restoring != nil }, set: { if !$0 { restoring = nil } })) {
            Button("Restore") { if let version = restoring { Task { await restore(version) } } }
        }
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
