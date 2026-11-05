import SwiftUI
import QuickLook
@preconcurrency import DMsgCore

struct FileDetailDestination: View {
    let id: String
    var version: String? = nil
    @Environment(SessionModel.self) private var model
    @State private var details: LibraryDetails?
    @State private var failure: String?
    @State private var loading = true
    var body: some View {
        Group {
            if let details {
                if details.file.trashedAt != nil {
                    List {
                        Text("This file is in Trash. Restore it before opening it.")
                        NavigationLink("Trash") { FileTrashView() }
                    }.privateScreen().navigationTitle("File")
                } else if let version {
                    if let item = details.versions.first(where: { $0.id == version }) {
                        FileDetailView(file: item.file(in: details.file), historical: !item.current)
                    } else { ContentUnavailableView("File not available on this device", systemImage: "doc.badge.ellipsis") }
                } else { FileDetailView(file: details.file) }
            } else {
                List {
                    if loading { ProgressView("Loading…") }
                    if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
                }.privateScreen().navigationTitle("File")
            }
        }.task(id: model.filesRevision) { await load() }
    }

    private func load() async {
        let stamp = model.epoch; loading = true; failure = nil; defer { loading = false }
        do {
            let value = try await model.fileDetails(id)
            guard stamp == model.epoch, !Task.isCancelled else { return }; details = value
        } catch { if stamp == model.epoch, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }
}

struct FileDetailView: View {
    let file: LibraryFile
    var historical = false
    @Environment(SessionModel.self) private var model
    @State private var preview: SharedFile?
    @State private var sharing: SharedFile?
    @State private var needsRestore = false
    @State private var confirmBackup = false
    @State private var confirmShare = false
    @State private var confirmEvict = false
    @State private var linking = false
    @State private var links: [FileLink] = []
    @State private var working = false
    @State private var failure: String?
    @State private var openedNonce: String?
    @State private var actionTask: Task<Void, Never>?
    @State private var visible = true
    struct Opened: Decodable { let path: String; let nonce: String }
    struct SharedFile: Identifiable { let url: URL; var id: URL { url } }
    private var available: Bool { file.canOpen && !needsRestore }
    private var backup: CloudBackup? { model.cloudBackups.first { $0.fileId == file.id && $0.versionId == file.current } }
    private var download: FileTransfers.Download? {
        guard let backup else { return nil }
        return model.fileTransfers.downloads.first { $0.id == backup.id && $0.stage != "done" }
    }

    var body: some View {
        List {
            FileSummaryRow(name: file.name, mime: file.mime, subtitle: fileSize(file.size) + " · " + (available ? file.statusLabel : String(localized: "Only in cloud")), expanded: true)
                .compactObjectRow().listRowBackground(Color.clear).accessibilityIdentifier("file.detail.title")
            Section {
                SettingsAction("Preview file", systemImage: "doc.text.magnifyingglass", prominent: available) { actionTask = Task { await open(share: false) } }
                    .disabled(!available)
                SettingsAction("Share a copy", systemImage: "square.and.arrow.up") { confirmShare = true }.disabled(!available)
                SettingsAction("Share by link", systemImage: "link") { linking = true }
                    .disabled(file.size > FileLinkTarget.maxBytes || !(available || file.backup != nil)).accessibilityIdentifier("file.link")
                if file.size > FileLinkTarget.maxBytes { Text("Files over 100 MiB cannot be shared by link.").font(.footnote).foregroundStyle(.secondary) }
                if !links.isEmpty {
                    NavigationLink { FileLinksView(file: file.id) } label: { LabeledContent("Share links", value: links.count.formatted()) }
                        .accessibilityIdentifier("file.links")
                }
                if !available {
                    SettingsAction("Restore to this device", systemImage: "icloud.and.arrow.down", prominent: true) { actionTask = Task { await restore() } }
                        .disabled(download != nil).accessibilityIdentifier("file.restore")
                }
                if !historical {
                    if file.backup?.verified == true {
                        Label("Backed up", systemImage: "checkmark.icloud").foregroundStyle(.secondary)
                    } else {
                        SettingsAction("Back up to cloud", systemImage: "icloud.and.arrow.up") { confirmBackup = true }
                            .disabled(!available)
                    }
                    NavigationLink("Manage file") { FileManagementView(file: file) }.accessibilityIdentifier("file.manage")
                    NavigationLink("File versions") { FileVersionsView(file: file) }.accessibilityIdentifier("file.versions")
                }
                if file.canOpen && file.backup?.verified == true {
                    SettingsAction("Remove local copy", systemImage: "internaldrive") { confirmEvict = true }.accessibilityIdentifier("file.evict")
                }
            }.disabled(working)
            if working { ProgressView("Loading…") }
            if let failure { Text(failure).font(.footnote).foregroundStyle(.red).accessibilityIdentifier("files.action.failure") }
            if let download {
                Section("File transfers") {
                    FileTransferProgressView(id: download.id, stage: download.stage, bytes: download.bytes, total: download.totalBytes, failure: download.error, retry: download.retryAt)
                    NavigationLink("File transfers") { TransfersView() }
                }
            }
            Section("Details") {
                LabeledContent("Type", value: file.mime)
                LabeledContent("Folder", value: (file.path ?? []).isEmpty ? String(localized: "Files") : (file.path ?? []).joined(separator: " / "))
                if let time = file.updatedAt ?? file.createdAt { LabeledContent("Modified") { Text(fileDate(time), format: .dateTime) } }
                if let backup = file.backup { LabeledContent("Backed up") { Text(fileDate(backup.backedUpAt), format: .dateTime) } }
                DisclosureGroup("Integrity") {
                    LabeledContent("Version", value: file.current).font(.caption).textSelection(.enabled)
                    Text(file.sha256).font(.caption.monospaced()).textSelection(.enabled)
                    Button("Copy SHA-256") { model.clipboard.copy(file.sha256); model.notice = String(localized: "Copied") }
                }
            }
        }.privateScreen().navigationTitle("File").navigationBarTitleDisplayMode(.inline)
            .sheet(item: $preview) { file in VerifiedPreview(url: file.url) }
            .sheet(item: $sharing) { file in ShareSheet(items: [file.url]) }
            .sheet(isPresented: $confirmBackup) { NavigationStack { FileBackupView(files: [file]) } }
            .sheet(isPresented: $linking) { NavigationStack { FileLinkComposer(target: FileLinkTarget(file: file, remote: !available)) } }
            .task(id: linking) { if !linking { await loadLinks() } }
            .confirmationDialog("Share a copy", isPresented: $confirmShare, titleVisibility: .visible) {
                Button("Share a copy") { actionTask = Task { await open(share: true) } }
            } message: { Text("Shared copies leave dMsg's protection and cannot be recalled.") }
            .confirmationDialog("Remove local copy?", isPresented: $confirmEvict, titleVisibility: .visible) {
                Button("Remove local copy", role: .destructive) { actionTask = Task { await evict() } }
            } message: { Text("The verified cloud backup and file record are kept. Download this version again to open it on this device.") }
            .onChange(of: file.canOpen) { _, value in if value { needsRestore = false } }
            .onChange(of: model.filesRevision) { _, _ in if file.canOpen { needsRestore = false } }
            .onChange(of: preview?.id) { before, after in if before != nil && after == nil { cleanup() } }
            .onChange(of: sharing?.id) { before, after in if before != nil && after == nil { cleanup() } }
            .onAppear { visible = true }
            .onDisappear { visible = false; if preview == nil && sharing == nil { actionTask?.cancel(); cleanup() } }
    }

    private func open(share: Bool) async {
        guard !working else { return }; model.activity()
        working = true; failure = nil; defer { working = false }; let stamp = model.epoch
        do {
            guard let engine = model.engine else { return }
            let result = try await coreOperation { try await engine.openFile(id: file.id, version: file.current, operation: $0) }
            let opened: Opened = try model.decode(result)
            guard stamp == model.epoch, !Task.isCancelled, !model.concealed, visible else {
                try? await engine.closeFile(nonce: opened.nonce); return
            }
            openedNonce = opened.nonce
            let url = URL(fileURLWithPath: opened.path)
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: url.path)
            if share { sharing = SharedFile(url: url) }
            else if QLPreviewController.canPreview(url as NSURL) { preview = SharedFile(url: url) }
            else { cleanup(); failure = String(localized: "Preview is unavailable for this file type. Share a copy to open it in another app.") }
        } catch {
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }
            if let error = error as? MobileError, ["NOT_FOUND", "INTEGRITY_FAILED"].contains(error.fileCode ?? "") { needsRestore = true }
            cleanup(); failure = model.fileFailureMessage(error)
        }
    }

    private func loadLinks() async {
        let stamp = model.epoch
        guard let saved = try? await model.fileLinks(refresh: false), stamp == model.epoch, !Task.isCancelled else { return }
        links = saved.filter { $0.file == file.id }
    }

    private func restore() async {
        guard !working else { return }; let stamp = model.epoch
        working = true; failure = nil; defer { working = false }
        await model.loadCloudBackups(refresh: true)
        guard stamp == model.epoch, visible, !Task.isCancelled else { return }
        guard let backup else {
            failure = model.cloudBackupsFailed ? model.cloudBackupsError : String(localized: "No cloud backup is available for this version.")
            return
        }
        do { try await model.restoreCloudFile(backup, folder: file.folder ?? model.filesFolder) }
        catch { if stamp == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func evict() async {
        guard !working else { return }; let stamp = model.epoch
        working = true; failure = nil; defer { working = false }
        do {
            #if DEBUG
            if model.uiTesting { try model.evictFileFixture(file.id, version: file.current) }
            else { try await evictRemote() }
            #else
            try await evictRemote()
            #endif
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }
            cleanup(); await model.loadFiles(); await model.loadCloudBackups(refresh: false)
        } catch { if stamp == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func evictRemote() async throws {
        guard let engine = model.engine else { throw CancellationError() }
        _ = try await coreOperation { try await engine.evictFile(id: file.id, version: file.current, operation: $0) }
    }

    private func cleanup() {
        guard let nonce = openedNonce else { return }; openedNonce = nil
        let engine = model.engine
        Task { try? await engine?.closeFile(nonce: nonce) }
    }
}

struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]
    func makeUIViewController(context: Context) -> UIActivityViewController { UIActivityViewController(activityItems: items, applicationActivities: nil) }
    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
