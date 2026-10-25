import SwiftUI

struct CloudBackupsView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var backups: [Backup] = []
    @State private var restoring: String?
    @State private var deleting: Backup?
    struct Backup: Decodable, Identifiable {
        let uploadId: String; let name: String?; let size: UInt64?; let bytes: UInt64
        let needsRoot: Bool; let local: Bool
        var id: String { uploadId }
    }
    var body: some View {
        List {
            NoticeBar(text: String(localized: "These are verified backups from your account's devices. Restore explicitly to keep a copy on this device."))
            ForEach(backups) { backup in
                VStack(alignment: .leading, spacing: 8) {
                    Label(backup.name ?? String(localized: "Encrypted backup"), systemImage: "doc.badge.icloud").font(.headline)
                    if let size = backup.size { Text(ByteCountFormatter.string(fromByteCount: Int64(size), countStyle: .file)).font(.caption).foregroundStyle(.secondary) }
                    if backup.needsRoot { Text("The required content key is not available yet.").font(.footnote).foregroundStyle(.secondary) }
                    else if backup.local { Label("On this device", systemImage: "checkmark.circle").font(.footnote).foregroundStyle(Theme.forest) }
                    else { Button(restoring == backup.id ? "Restoring…" : "Restore to this device") { Task { await restore(backup) } }.disabled(restoring != nil) }
                }.padding(.vertical, 4)
                    .contextMenu { Button("Delete cloud backup", role: .destructive) { deleting = backup } }
            }
            if backups.isEmpty { ContentUnavailableView("No cloud backups", systemImage: "icloud", description: Text("Files are backed up only when you explicitly choose to do so.")) }
        }.privateScreen().navigationTitle("Cloud backups")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .task { await load(false); await load(true) }
            .refreshable { await load(true) }
            .confirmationDialog("Delete this cloud backup?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
                Button("Delete cloud backup", role: .destructive) { if let deleting { Task {
                    guard let engine = model.engine else { return }
                    do { _ = try await coreOperation { try await engine.deleteBackup(upload: deleting.id, operation: $0) }; self.deleting = nil; await load(true) }
                    catch { model.report(error) }
                } } }
            } message: { Text("This removes only the selected cloud copy. Local files remain on their devices.") }
    }
    private func load(_ refresh: Bool) async {
        guard let engine = model.engine else { return }
        do { let result = try await coreOperation { try await engine.cloudBackups(refresh: refresh, operation: $0) }; backups = try model.decode(result) }
        catch { model.report(error) }
    }
    private func restore(_ backup: Backup) async {
        guard let engine = model.engine else { return }
        restoring = backup.id; defer { restoring = nil }; model.activity()
        do {
            if let files = model.files,
               let available = try files.root.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage,
               available < Int64(backup.bytes) + 8 * 1024 * 1024 { throw CocoaError(.fileWriteOutOfSpace) }
            _ = try await coreOperation { try await engine.restoreBackup(upload: backup.id, folder: model.filesFolder, operation: $0) }
            await load(false); await model.refresh()
        } catch { model.report(error) }
    }
}
