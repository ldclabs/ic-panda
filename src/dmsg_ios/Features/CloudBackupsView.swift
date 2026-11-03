import SwiftUI

struct CloudBackupSection: View {
    var query = ""
    var sort = FileSort.date
    @Environment(SessionModel.self) private var model
    private var backups: [CloudBackup] { sort.backups(model.cloudBackups, query: query) }

    var body: some View {
        Section {
            CloudBackupStatusView()
            ForEach(backups) { backup in
                NavigationLink { CloudBackupDetailView(backup: backup) } label: {
                    VStack(alignment: .leading, spacing: 6) {
                        FileSummaryRow(name: backup.displayName, mime: backup.mime ?? "", subtitle: subtitle(backup))
                        if let job = model.fileTransfers.downloads.first(where: { $0.id == backup.id && $0.stage != "done" }) {
                            FileTransferProgressView(id: job.id, stage: job.stage, bytes: job.bytes, total: job.totalBytes, failure: job.error, retry: job.retryAt, compact: true)
                        } else {
                            Label(backup.needsRoot ? "The required content key is not available yet." : backup.local ? "On this device" : "Only in cloud",
                                  systemImage: backup.needsRoot ? "key" : backup.local ? "checkmark.circle" : "icloud")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }.compactObjectRow().accessibilityIdentifier("backup.item.\(backup.id)")
            }
            if !model.loadingCloudBackups && !model.cloudBackupsFailed && backups.isEmpty {
                if !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    ContentUnavailableView.search(text: query)
                    Button("Clear search") { model.filesSearch = "" }
                } else if model.cloudBackupsComplete {
                    ContentUnavailableView("No cloud backups", systemImage: "icloud",
                        description: Text("Choose Back up to cloud from a local file to create your first backup."))
                }
            }
        }
    }

    private func subtitle(_ backup: CloudBackup) -> String {
        var parts = [backup.size.map(fileSize) ?? ""]
        if backup.date > 0 { parts.append(fileDate(backup.date).formatted(date: .abbreviated, time: .shortened)) }
        if let device = backup.device { parts.append(model.fileSourceDevice(device)) }
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

struct CloudBackupStatusView: View {
    @Environment(SessionModel.self) private var model
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 3) {
                    if model.loadingCloudBackups { ProgressView("Refreshing backups…") }
                    else if model.cloudBackupsFailed {
                        Label("Backup list could not be refreshed", systemImage: "exclamationmark.icloud").font(.subheadline)
                    } else if !model.cloudBackupsKnown { Text("Refresh to check cloud backups").font(.subheadline) }
                    if let at = model.cloudBackupsObservedAt {
                        (Text("Last refreshed") + Text(verbatim: " · " + fileDate(at).formatted(date: .abbreviated, time: .shortened)))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    if model.cloudBackupsFailed {
                        Text(model.cloudBackupsError ?? String(localized: "Showing saved content. The latest sync did not finish."))
                            .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
                Button { Task { await model.loadCloudBackups(refresh: true) } } label: {
                    Image(systemName: "arrow.clockwise").font(.system(size: 18)).frame(width: 44, height: 44)
                }.buttonStyle(.borderless).disabled(model.loadingCloudBackups)
                    .accessibilityLabel(model.cloudBackupsFailed ? "Retry" : "Refresh backup list")
                    .accessibilityIdentifier("backups.refresh")
            }
        }.listRowBackground(Color.clear).listRowSeparator(.hidden).accessibilityElement(children: .contain)
    }
}

struct CloudBackupDetailView: View {
    let backup: CloudBackup
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var folder = "root"
    @State private var folderName = String(localized: "Files")
    @State private var choosing = false
    @State private var working = false
    @State private var failure: String?
    @State private var deleting = false
    @State private var background = false
    @State private var localDetails: LibraryDetails?
    @State private var initialized = false
    @State private var visible = true
    private var current: CloudBackup { model.cloudBackups.first { $0.id == backup.id } ?? backup }
    private var job: FileTransfers.Download? { model.fileTransfers.downloads.first { $0.id == backup.id && $0.stage != "done" } }
    private var pending: Bool { job != nil || model.fileOperations.contains("restore:\(backup.id)") }

    var body: some View {
        List {
            FileSummaryRow(name: current.displayName, mime: current.mime ?? "", subtitle: current.size.map(fileSize) ?? "", expanded: true)
                .compactObjectRow().listRowBackground(Color.clear)
            if let job {
                Section("File transfers") {
                    FileTransferProgressView(id: job.id, stage: job.stage, bytes: job.bytes, total: job.totalBytes, failure: job.error, retry: job.retryAt)
                    NavigationLink("File transfers") { TransfersView() }
                }
            }
            Section {
                if current.needsRoot {
                    Label("The required content key is not available yet.", systemImage: "key")
                    NavigationLink("Account synchronization") { WorkspaceStatusView() }
                    SettingsAction("Refresh backup list", systemImage: "arrow.clockwise") { Task { await model.loadCloudBackups(refresh: true) } }
                } else if current.local {
                    if let file = localDetails?.file, file.trashedAt != nil {
                        NavigationLink("Trash") { FileTrashView() }
                    } else if let file = current.fileId {
                        NavigationLink("Open local file") { FileDetailDestination(id: file, version: current.versionId) }
                    }
                } else {
                    LabeledContent("Destination folder", value: localDetails.map { ($0.file.path ?? []).joined(separator: " / ").nonEmpty ?? String(localized: "Files") } ?? folderName)
                    if localDetails == nil { Button("Choose folder") { choosing = true }.disabled(working || pending) }
                    SettingsAction("Restore to this device", systemImage: "icloud.and.arrow.down", prominent: true) { Task { await restore() } }
                        .disabled(working || pending).accessibilityIdentifier("backup.restore.\(backup.id)")
                    Text("Restoring downloads and verifies the selected version before it can be opened.")
                        .font(.footnote).foregroundStyle(.secondary)
                    DisclosureGroup("Background download") {
                        Text("Only the selected encrypted file can transfer while dMsg is locked. iOS controls scheduling; force quitting stops delivery until you open the app again. Open the original account to verify and finish.")
                            .font(.footnote).foregroundStyle(.secondary)
                        Button("Download encrypted copy in background") { background = true }.disabled(working || pending || model.busy)
                    }
                }
                if working { ProgressView() }
                if let failure { Text(failure).font(.footnote).foregroundStyle(.red).accessibilityIdentifier("files.action.failure") }
            }
            Section("Details") {
                if current.date > 0 { LabeledContent("Backed up") { Text(fileDate(current.date), format: .dateTime) } }
                if let device = current.device { LabeledContent("Source device", value: model.fileSourceDevice(device)) }
                LabeledContent("Original folder", value: (current.path ?? []).joined(separator: " / ").nonEmpty ?? String(localized: "Files"))
                if let version = current.versionId { LabeledContent("Version", value: version).font(.caption).textSelection(.enabled) }
                if let hash = current.sha256 { DisclosureGroup("Integrity") { Text(hash).font(.caption.monospaced()).textSelection(.enabled) } }
            }
            Section {
                SettingsAction("Delete cloud backup", systemImage: "trash", role: .destructive) { deleting = true }.disabled(working || pending)
            }
        }.privateScreen().navigationTitle("Cloud backup").navigationBarTitleDisplayMode(.inline)
            .task { await loadLocal() }
            .onAppear { visible = true }
            .onDisappear { visible = false }
            .onChange(of: current.local) { _, _ in Task { await loadLocal() } }
            .sheet(isPresented: $choosing) { NavigationStack { FileFolderPicker { id, name in folder = id; folderName = name } } }
            .confirmationDialog("Background download", isPresented: $background, titleVisibility: .visible) {
                Button("Download encrypted copy in background") { Task { await model.startBackgroundDownload(upload: backup.id, folder: folder) } }
            } message: { Text("Open the original account to verify and finish this download.") }
            .confirmationDialog("Delete this cloud backup?", isPresented: $deleting, titleVisibility: .visible) {
                Button("Delete cloud backup", role: .destructive) { Task { await delete() } }
            } message: {
                Text(current.local ? "This removes only the selected cloud copy. Local files remain on their devices." : "This device has no local copy. Deleting this backup may remove your only recoverable copy.")
            }
    }

    private func loadLocal() async {
        if !initialized { folder = model.filesFolder; folderName = model.library.folder?.name ?? String(localized: "Files"); initialized = true }
        let stamp = model.epoch
        guard let id = current.fileId else { return }
        let details = try? await model.fileDetails(id)
        if stamp == model.epoch, !Task.isCancelled { localDetails = details }
    }

    private func restore() async {
        guard !working, !pending else { return }; let stamp = model.epoch
        working = true; failure = nil; defer { working = false }
        do { try await model.restoreCloudFile(current, folder: folder) }
        catch { if stamp == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func delete() async {
        guard !working else { return }; let stamp = model.epoch
        working = true; failure = nil; defer { working = false }
        do {
            #if DEBUG
            if model.uiTesting { model.deleteFileCloudFixture(backup.id) }
            else { try await deleteRemote() }
            #else
            try await deleteRemote()
            #endif
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }
            await model.loadCloudBackups(refresh: true); await model.loadFiles(); dismiss()
        } catch { if stamp == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func deleteRemote() async throws {
        guard let engine = model.engine else { throw CancellationError() }
        _ = try await coreOperation { try await engine.deleteBackup(upload: backup.id, operation: $0) }
    }
}

private extension String { var nonEmpty: String? { isEmpty ? nil : self } }
