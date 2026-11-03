import SwiftUI

struct TransfersView: View {
    @Environment(SessionModel.self) private var model
    @State private var working: Set<String> = []
    @State private var failure: String?
    @State private var visible = true
    @State private var backgroundActive = false
    var body: some View {
        List {
            TransferStatusView()
            if let failure { Text(failure).font(.footnote).foregroundStyle(.red) }
            if model.fileTransfers.loadFailed {
                FileInlineFailure(message: String(localized: "Transfer status could not be refreshed. Saved tasks are kept.")) { Task { await model.fileTransfers.load(model) } }
            }
            if !model.fileTransfers.backups.isEmpty {
                Section("Pending backups") {
                    ForEach(model.fileTransfers.backups) { job in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(job.name ?? String(localized: "Encrypted backup")).font(.body.weight(.medium)).fixedSize(horizontal: false, vertical: true)
                            FileTransferProgressView(id: job.id, stage: job.stage, bytes: job.bytes, total: job.totalBytes, failure: job.error, retry: job.retryAt)
                            if job.error != nil {
                                Button("Retry") { Task { await act(job.id) { try await model.engine?.retryBackup(id: job.file, version: job.version) } } }
                            }
                            if job.stage != "committing" {
                                Menu("Actions") {
                                    Button("Continue this backup in background") { Task { await model.startBackgroundBackup(file: job.file, version: job.version) } }
                                    Button("Cancel", role: .destructive) { Task { await act(job.id) {
                                        guard let engine = model.engine else { return }
                                        try await coreOperation { try await engine.cancelBackup(id: job.file, version: job.version, operation: $0) }
                                    } } }
                                }.disabled(model.busy)
                            }
                        }.padding(.vertical, 4).disabled(working.contains(job.id))
                    }
                }
            }
            let pending = model.fileTransfers.downloads.filter { $0.stage != "done" }
            if !pending.isEmpty {
                Section("Downloads") {
                    ForEach(pending) { job in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(job.name ?? model.cloudBackups.first(where: { $0.id == job.id })?.name ?? String(localized: "Encrypted backup")).font(.body.weight(.medium)).fixedSize(horizontal: false, vertical: true)
                            FileTransferProgressView(id: job.id, stage: job.stage, bytes: job.bytes, total: job.totalBytes, failure: job.error, retry: job.retryAt)
                            HStack {
                                if job.error != nil { Button("Retry") { Task { await act(job.id) { try await model.engine?.retryDownload(upload: job.id) } } } }
                                Button("Cancel", role: .destructive) { Task { await act(job.id) { try await model.engine?.cancelDownload(upload: job.id) } } }
                            }.buttonStyle(.bordered)
                        }.padding(.vertical, 4).disabled(working.contains(job.id))
                    }
                }
            }
            BackgroundTransferSection(active: $backgroundActive)
            if model.fileTransfers.activeCount == 0 && !backgroundActive && !model.transfer.running && !model.fileTransfers.loadFailed {
                ContentUnavailableView("No active transfers", systemImage: "checkmark.circle")
            }
            let completed = model.fileTransfers.downloads.filter { $0.stage == "done" }
            if !completed.isEmpty {
                DisclosureGroup("Completed downloads") {
                    ForEach(completed) { job in
                        if let backup = model.cloudBackups.first(where: { $0.id == job.id }), let file = backup.fileId {
                            NavigationLink { FileDetailDestination(id: file, version: backup.versionId) } label: { Label(job.name ?? backup.displayName, systemImage: "checkmark.circle") }
                        } else { Label(job.name ?? String(localized: "Encrypted backup"), systemImage: "checkmark.circle") }
                    }
                }
            }
            Section {
                NavigationLink { IncomingSharesView() } label: { LabeledContent("Shared with dMsg", value: model.filesSharedImports > 0 ? model.filesSharedImports.formatted() : "") }
                NavigationLink { PendingImportsView() } label: { LabeledContent("Pending imports", value: model.filesPendingImports > 0 ? model.filesPendingImports.formatted() : "") }
                NavigationLink("About files and backups") { FileLibraryHelpView() }
            } footer: { Text("Locking pauses transfers; unlocking resumes them.") }
        }.privateScreen().navigationTitle("File transfers")
            .task { await model.fileTransfers.load(model); await model.loadFileSummary() }
            .refreshable { await model.fileTransfers.load(model); await model.loadFileSummary() }
            .onAppear { visible = true }.onDisappear { visible = false }
    }

    private func act(_ id: String, action: () async throws -> Void) async {
        guard working.insert(id).inserted else { return }; let stamp = model.epoch
        failure = nil; defer { working.remove(id) }
        do { try await action() }
        catch { if stamp == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
        guard stamp == model.epoch else { return }; await model.fileTransfers.load(model); await model.loadFiles()
    }
}

struct FileTransferProgressView: View {
    let id: String
    let stage: String
    let bytes: UInt64
    let total: UInt64
    let failure: String?
    let retry: UInt64?
    var compact = false

    static func title(stage: String, failed: Bool) -> String {
        if failed { return String(localized: "Transfer needs attention") }
        switch stage {
        case "done": return String(localized: "Verified")
        case "committing": return String(localized: "Confirming cloud backup…")
        case "verifying": return String(localized: "Verifying…")
        case "retrying", "queued": return String(localized: "Waiting")
        case "paused": return String(localized: "Transfer paused")
        case "uploading": return String(localized: "Backing up…")
        default: return String(localized: "Restoring…")
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: compact ? 3 : 6) {
            let title = Self.title(stage: stage, failed: failure != nil)
            if failure == nil && ["committing", "verifying"].contains(stage) {
                HStack(spacing: 8) { ProgressView().controlSize(.small); Text(title).font(.footnote).accessibilityIdentifier("transfer.stage.\(id)") }
            } else {
                Text(title).font(.footnote).accessibilityIdentifier("transfer.stage.\(id)")
                if failure == nil { ProgressView(value: min(Double(bytes), Double(total)), total: Double(max(1, total))).accessibilityLabel(Text(title)) }
            }
            if !compact { Text(verbatim: fileSize(bytes) + " / " + fileSize(total)).font(.caption).foregroundStyle(.secondary) }
            if let failure { Text(fileFailureCode(failure)).font(.caption).foregroundStyle(.secondary) }
            if let retry {
                (Text("Next retry") + Text(verbatim: " · " + fileDate(retry).formatted(date: .omitted, time: .shortened))).font(.caption).foregroundStyle(.secondary)
            }
        }.accessibilityElement(children: .contain)
    }
}
