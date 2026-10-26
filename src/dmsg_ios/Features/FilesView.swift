import SwiftUI
import UniformTypeIdentifiers
import QuickLook
import PhotosUI

struct FilesView: View {
    @State private var columns: NavigationSplitViewVisibility = .all
    @Environment(SessionModel.self) private var model
    @State private var selected: String?
    @State private var importing = false
    @State private var photo: PhotosPickerItem?
    @State private var newFolder = false
    @State private var folderName = ""
    @State private var trash = false
    @State private var removing: LibraryFile?
    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            List(selection: $selected) {
                Section {
                    HStack {
                        Label("Files in this folder", systemImage: "internaldrive")
                        Spacer()
                        Text(ByteCountFormatter.string(fromByteCount: Int64(model.library.files.reduce(0) { $0 + $1.size }), countStyle: .file)).foregroundStyle(.secondary)
                    }.font(.subheadline).padding(.vertical, 4)
                    if !model.library.folders.isEmpty {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 150))]) {
                            ForEach(model.library.folders) { folder in
                                Button { openFolder(folder.id) } label: {
                                    Label(folder.name, systemImage: "folder").font(.subheadline).frame(maxWidth: .infinity, alignment: .leading)
                                        .padding(14).background(Theme.forest.opacity(0.07), in: RoundedRectangle(cornerRadius: 14))
                                }.buttonStyle(.plain)
                            }
                        }.listRowBackground(Color.clear)
                    }
                }
                Section("On this device") {
                    ForEach(model.library.files) { file in
                        NavigationLink(value: file.id) { FileRow(file: file) }
                            .contextMenu { Button("Move to Trash", role: .destructive) { removing = file } }
                    }
                    if model.library.files.isEmpty && model.cloudBackups.isEmpty && !model.loadingCloudBackups && !model.cloudBackupsFailed {
                        Text("Import a document to encrypt it on this device. Cloud backup is a separate action.")
                            .font(.footnote).foregroundStyle(.secondary)
                    }
                }
                CloudBackupSection()
                NoticeBar(text: String(localized: "Local files are not backed up automatically. Uninstalling dMsg removes files that have no cloud backup."))
                    .listRowBackground(Color.clear)
            }.listStyle(.plain).privateScreen().navigationTitle(model.library.folder?.name ?? String(localized: "Files"))
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        if let folder = model.library.folder { Button("Back", systemImage: "chevron.backward") { openFolder(folder.parent ?? "root") } }
                        else { ApprovalButton() }
                    }
                    ToolbarItem(placement: .primaryAction) {
                        Menu("Import", systemImage: "plus") {
                            Button("Import file", systemImage: "doc") { importing = true }
                            PhotosPicker(selection: $photo, matching: .images) { Label("Import photo", systemImage: "photo") }
                            Button("New folder", systemImage: "folder.badge.plus") { newFolder = true }
                            Button("Trash", systemImage: "trash") { trash = true }
                        }.disabled(model.filesFolder != (model.library.folder?.id ?? "root"))
                    }
                }.task { await model.refresh(); await model.loadCloudBackups(refresh: true) }
                .refreshable { await model.synchronize() }
                .fileImporter(isPresented: $importing, allowedContentTypes: [.item]) { result in
                    Task { await importFile(result) }
                }
                .onChange(of: photo) { _, selected in Task {
                    let epoch = model.epoch
                    do {
                        guard let selected, let imported = try await selected.loadTransferable(type: PhotoImport.self) else { return }
                        defer { imported.finish() }
                        guard epoch == model.epoch else { return }
                        await importFile(.success(imported.file))
                    } catch { if epoch == model.epoch { model.report(error) } }
                } }
        } detail: {
            if let file = model.library.files.first(where: { $0.id == selected }) { FileDetailView(file: file) }
            else { ContentUnavailableView("Select a file", systemImage: "doc") }
        }
        .navigationSplitViewStyle(.balanced)
        .onAppear { if selected == nil { columns = .all } }
        .onChange(of: selected) { _, value in if value != nil { columns = .automatic } }
        .sheet(isPresented: $trash) { NavigationStack { FileTrashView() } }
        .confirmationDialog("Move this local file to Trash?", isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } })) {
            Button("Move to Trash", role: .destructive) { if let removing { Task {
                do { try await model.engine?.trashFile(id: removing.id); self.removing = nil; await model.refresh() }
                catch { model.report(error) }
            } } }
        } message: { Text("Cloud backups are kept. This only changes the copy on this device.") }
        .alert("New folder", isPresented: $newFolder) {
            TextField("Folder name", text: $folderName)
            Button("Cancel", role: .cancel) { folderName = "" }
            Button("Create") { Task {
                do { _ = try await model.engine?.createFolder(parent: model.filesFolder, name: folderName); folderName = ""; await model.refresh() }
                catch { model.report(error) }
            } }
        }
    }

    private func openFolder(_ id: String) { model.activity(); selected = nil; model.library = .empty; model.filesFolder = id; Task { await model.refresh() } }

    private func importFile(_ result: Result<URL, Error>) async {
        let epoch = model.epoch
        do {
            guard let paths = model.files, let engine = model.engine, let account = model.session?.account else { return }
            let folder = model.filesFolder
            let url = try result.get()
            let staged = try await Task.detached { try paths.importDocument(url, account: account) }.value
            guard epoch == model.epoch else { return }
            let handle = account + "/" + staged.deletingLastPathComponent().lastPathComponent + "/" + staged.lastPathComponent
            _ = try await coreOperation { try await engine.importFile(name: handle, folder: folder, operation: $0) }
            try paths.finishImport(staged)
            if epoch == model.epoch { await model.refresh() }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct FileRow: View {
    let file: LibraryFile
    var body: some View {
        HStack(spacing: 12) {
            ObjectIcon(name: file.mime.hasPrefix("image/") ? "photo" : "doc.text")
            VStack(alignment: .leading, spacing: 4) {
                Text(file.name).font(.body.weight(.medium)).lineLimit(2)
                Text(ByteCountFormatter.string(fromByteCount: Int64(file.size), countStyle: .file) + " · " + (file.status == "backed_up" ? String(localized: "Backed up") : String(localized: "Local")))
                    .font(.caption).foregroundStyle(.secondary)
            }.padding(.vertical, 5)
        }
    }
}

struct FileDetailView: View {
    let file: LibraryFile
    @Environment(SessionModel.self) private var model
    @State private var preview: SharedFile?
    @State private var sharing: SharedFile?
    @State private var confirmBackup = false
    @State private var working = false
    @State private var details: DisplayValue?
    @State private var openedNonce: String?
    struct Opened: Decodable { let path: String; let nonce: String }
    struct BackupProgress: Decodable { let pending: UInt64; let failed: UInt64; let paused: Bool }
    struct SharedFile: Identifiable { let url: URL; var id: URL { url } }
    var body: some View {
        List {
            HStack(spacing: 14) {
                Image(systemName: "doc.richtext").font(.system(size: 36)).foregroundStyle(Theme.forest).frame(width: 80, height: 94)
                    .background(Theme.forest.opacity(0.07), in: RoundedRectangle(cornerRadius: 14))
                VStack(alignment: .leading, spacing: 6) {
                    Text(file.name).font(.title3.bold()).fixedSize(horizontal: false, vertical: true)
                    Text(ByteCountFormatter.string(fromByteCount: Int64(file.size), countStyle: .file)).font(.subheadline).foregroundStyle(.secondary)
                    Text(file.status == "backed_up" ? "Backed up" : "On this device").font(.caption).foregroundStyle(Theme.forest)
                }
            }.listRowBackground(Color.clear).listRowSeparator(.hidden)
            Section {
                Button("Preview file", systemImage: "doc.text.magnifyingglass") { Task { await open(share: false) } }
                Button("Share a copy", systemImage: "square.and.arrow.up") { Task { await open(share: true) } }
                Button("Back up to cloud", systemImage: "icloud.and.arrow.up") { confirmBackup = true }
            }.disabled(working)
            Section("Integrity") { LabeledContent("SHA-256") { Text(file.sha256).font(.caption.monospaced()).textSelection(.enabled) } }
            if let details { Section("Versions") { CanonicalDetails(value: details) } }
            NoticeBar(text: String(localized: "Shared copies leave dMsg's protection and cannot be recalled."))
                .listRowBackground(Color.clear)
        }.privateScreen().navigationTitle("File").navigationBarTitleDisplayMode(.inline)
            .sheet(item: $preview) { file in VerifiedPreview(url: file.url) }
            .sheet(item: $sharing) { file in ShareSheet(items: [file.url]) }
            .onChange(of: preview?.id) { before, after in if before != nil && after == nil { cleanup() } }
            .onChange(of: sharing?.id) { before, after in if before != nil && after == nil { cleanup() } }
            .onDisappear { if preview == nil && sharing == nil { cleanup() } }
            .confirmationDialog("Back up this file?", isPresented: $confirmBackup) {
                Button("Back up encrypted copy") { Task { await backup() } }
            } message: { Text("The encrypted version will use cloud storage. Your local copy stays on this device.") }
            .task(id: file.id) {
                details = nil; let epoch = model.epoch
                do { if let result = try await model.engine?.fileDetails(id: file.id), epoch == model.epoch, !Task.isCancelled { details = try model.decode(result) } }
                catch { model.report(error) }
            }
    }

    private func open(share: Bool) async {
        model.activity()
        working = true; defer { working = false }; let epoch = model.epoch
        do {
            guard let result = try await model.engine?.openFile(id: file.id, version: file.current), epoch == model.epoch else { return }
            let opened: Opened = try model.decode(result)
            openedNonce = opened.nonce
            let url = URL(fileURLWithPath: opened.path)
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: url.path)
            if share { sharing = SharedFile(url: url) } else { preview = SharedFile(url: url) }
        } catch { model.report(error) }
    }

    private func backup() async {
        working = true; defer { working = false }; let epoch = model.epoch
        do {
            guard let engine = model.engine else { return }
            _ = try await engine.backupFile(id: file.id, expectedVersion: file.current)
            // Continue only this explicit confirmation while the scene is active.
            repeat {
                let snapshot = try await coreOperation { try await engine.runBackups(operation: $0) }
                guard epoch == model.epoch else { return }
                let progress: BackupProgress = try model.decode(snapshot)
                if progress.pending == 0 || progress.failed > 0 || !progress.paused { break }
            } while !Task.isCancelled && epoch == model.epoch && !model.concealed
            model.notice = String(localized: "Backup progress saved. Check the file status for completion.")
            await model.refresh()
        }
        catch { if epoch == model.epoch { model.report(error) } }
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
