import SwiftUI

struct CloudBackupSection: View {
    @Environment(SessionModel.self) private var model
    @State private var restoring: String?
    @State private var deleting: CloudBackup?

    var body: some View {
        Section("Cloud backups") {
            NoticeBar(text: String(localized: "These are verified backups from your account's devices. Restore explicitly to keep a copy on this device."))
            if model.loadingCloudBackups { ProgressView("Syncing…") }
            if model.cloudBackupsFailed {
                NoticeBar(text: String(localized: "Showing saved content. The latest sync did not finish."))
                Button("Sync now") { Task { await model.loadCloudBackups(refresh: true) } }.disabled(model.loadingCloudBackups)
            }
            ForEach(model.cloudBackups) { backup in
                VStack(alignment: .leading, spacing: 8) {
                    Label(backup.name ?? String(localized: "Encrypted backup"), systemImage: "doc").font(.headline)
                    if let size = backup.size { Text(ByteCountFormatter.string(fromByteCount: Int64(size), countStyle: .file)).font(.caption).foregroundStyle(.secondary) }
                    if backup.needsRoot { Text("The required content key is not available yet.").font(.footnote).foregroundStyle(.secondary) }
                    else if backup.local { Label("On this device", systemImage: "checkmark.circle").font(.footnote).foregroundStyle(Theme.forest) }
                    else {
                        Button { Task { await restore(backup) } } label: {
                            HStack {
                                if restoring == backup.id { ProgressView().controlSize(.small) }
                                Text(restoring == backup.id ? "Restoring…" : "Restore to this device")
                            }
                        }.buttonStyle(.bordered).disabled(restoring != nil)
                            .accessibilityIdentifier("backup.restore.\(backup.id)")
                    }
                }.padding(.vertical, 4)
                    .contextMenu { Button("Delete cloud backup", role: .destructive) { deleting = backup } }
            }
            if model.cloudBackups.isEmpty && !model.loadingCloudBackups && !model.cloudBackupsFailed {
                Text("No cloud backups").foregroundStyle(.secondary)
            }
        }
            .confirmationDialog("Delete this cloud backup?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
                Button("Delete cloud backup", role: .destructive) { if let deleting { Task {
                    guard let engine = model.engine else { return }
                    let epoch = model.epoch
                    do {
                        _ = try await coreOperation { try await engine.deleteBackup(upload: deleting.id, operation: $0) }
                        guard epoch == model.epoch else { return }
                        self.deleting = nil; await model.loadCloudBackups(refresh: true)
                    } catch { if epoch == model.epoch { model.report(error) } }
                } } }
            } message: { Text("This removes only the selected cloud copy. Local files remain on their devices.") }
    }
    private func restore(_ backup: CloudBackup) async {
        guard let engine = model.engine else { return }
        let epoch = model.epoch, folder = model.filesFolder
        restoring = backup.id; defer { restoring = nil }; model.activity()
        do {
            if let files = model.files,
               let available = try files.root.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage,
               available < Int64(backup.bytes) + 8 * 1024 * 1024 { throw CocoaError(.fileWriteOutOfSpace) }
            _ = try await coreOperation { try await engine.restoreBackup(upload: backup.id, folder: folder, operation: $0) }
            guard epoch == model.epoch else { return }
            await model.loadCloudBackups(refresh: false); await model.refresh()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
