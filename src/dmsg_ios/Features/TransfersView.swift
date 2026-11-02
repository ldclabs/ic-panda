import SwiftUI

struct TransfersView: View {
    @Environment(SessionModel.self) private var model
    var body: some View {
        List {
            TransferStatusView()
            BackgroundTransferSection()
            NavigationLink("Shared with dMsg") { IncomingSharesView() }
            NavigationLink("Pending imports") { PendingImportsView() }
            Section("Pending backups") {
                Text("Locking pauses transfers; unlocking resumes them.").font(.footnote).foregroundStyle(.secondary)
                ForEach(model.fileTransfers.backups) { job in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(model.library.files.first(where: { $0.id == job.file })?.name ?? String(job.file.prefix(12)) + "…").font(.headline).lineLimit(2)
                        transferProgress(id: job.id, stage: job.stage, bytes: job.bytes, total: job.totalBytes, error: job.error, retry: job.retryAt)
                        Button("Continue this backup in background") { Task { await model.startBackgroundBackup(file: job.file, version: job.version) } }.disabled(model.busy || job.stage == "committing")
                    }
                }
                if model.fileTransfers.backups.isEmpty { Text("No pending backups").foregroundStyle(.secondary) }
            }
            Section("Downloads") {
                ForEach(model.fileTransfers.downloads) { job in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(model.cloudBackups.first(where: { $0.id == job.id })?.name ?? String(localized: "Encrypted backup")).font(.headline)
                        transferProgress(id: job.id, stage: job.stage, bytes: job.bytes, total: job.totalBytes, error: job.error, retry: job.retryAt)
                        if job.stage != "done" {
                            Button("Cancel", role: .destructive) { Task {
                                do { try await model.engine?.cancelDownload(upload: job.id); await model.fileTransfers.load(model) }
                                catch { model.report(error) }
                            } }
                        }
                    }
                }
            }
            if model.fileTransfers.backups.contains(where: { $0.error != nil }) || model.fileTransfers.downloads.contains(where: { $0.error != nil }) {
                Button("Retry") { Task {
                    do { try await model.engine?.retryFileTransfers(); await model.fileTransfers.load(model) }
                    catch { model.report(error) }
                } }
            }
        }.privateScreen().navigationTitle("File transfers").task { await model.fileTransfers.load(model) }
            .refreshable { await model.fileTransfers.load(model) }
    }

    @ViewBuilder
    private func transferProgress(id: String, stage: String, bytes: UInt64, total: UInt64, error: String?, retry: UInt64?) -> some View {
        if stage == "committing" || stage == "verifying" { ProgressView().controlSize(.small) }
        else { ProgressView(value: Double(bytes), total: Double(max(1, total))) }
        Text(verbatim: "\(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)) / \(ByteCountFormatter.string(fromByteCount: Int64(total), countStyle: .file))").font(.caption).foregroundStyle(.secondary)
        Text(LocalizedStringKey(stage == "done" ? "Verified" : stage == "committing" ? "Confirming cloud backup…" : stage == "verifying" ? "Verifying…" : stage == "retrying" ? "Waiting" : stage == "paused" ? "Locking pauses transfers; unlocking resumes them." : stage == "queued" ? "Waiting" : stage == "uploading" ? "Backing up…" : "Restoring…")).font(.footnote).accessibilityIdentifier("transfer.stage.\(id)")
        if let error { Text(error).font(.caption).foregroundStyle(.secondary) }
        if let retry { Text(Date(timeIntervalSince1970: Double(retry) / 1000), style: .time).font(.caption) }
    }
}
