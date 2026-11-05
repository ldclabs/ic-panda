import AVFoundation
import SwiftUI
import PhotosUI

struct FilesView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var textSize
    @Environment(\.openURL) private var openURL
    @State private var selected: String?
    @State private var importing = false
    @State private var choosingPhotos = false
    @State private var photos: [PhotosPickerItem] = []
    @State private var capturing = false
    @State private var cameraDenied = false
    @State private var showingTransfers = false
    @State private var sheet: FileLibrarySheet?
    @State private var selecting = false
    @State private var selection: Set<String> = []
    @State private var removing: [LibraryItemTarget] = []
    @State private var working = false
    @State private var importingPhotos = false

    private var files: [LibraryFile] { model.filesSort.files(model.library.files) }
    private var targets: [LibraryItemTarget] {
        model.library.folders.map { LibraryItemTarget(id: $0.id, name: $0.name, folder: true) }
        + files.map { LibraryItemTarget(id: $0.id, name: $0.name, folder: false) }
    }
    private var chosen: [LibraryItemTarget] { targets.filter { selection.contains($0.id) } }

    var body: some View {
        @Bindable var model = model
        WorkspaceNavigation(selection: $selected, sidebarInitiallyVisible: true) { listSelection in
            // Large text scrolls the bar away on phones. A selectable list would swallow taps on its menus.
            let barInList = textSize.isAccessibilitySize && listSelection == nil
            VStack(spacing: 0) {
                if !barInList { FileBrowseBar(scope: $model.filesScope, sort: $model.filesSort, showingTransfers: $showingTransfers) }
                List(selection: selecting ? nil : listSelection) {
                    if barInList {
                        FileBrowseBar(scope: $model.filesScope, sort: $model.filesSort, showingTransfers: $showingTransfers).buttonStyle(.borderless)
                            .listRowInsets(EdgeInsets()).listRowBackground(Color.clear).listRowSeparator(.hidden)
                    }
                    transferEntry
                    if model.filesScope == .local { localContent }
                    else { CloudBackupSection(query: model.filesSearch, sort: model.filesSort) }
                }.listStyle(.plain)
                    .refreshable {
                        if model.filesScope == .cloud { await model.loadCloudBackups(refresh: true) }
                        else { await model.loadFiles() }
                        await model.fileTransfers.load(model)
                    }
                    .safeAreaInset(edge: .bottom) { if selecting { selectionBar } }
            }.privateScreen().navigationTitle(model.filesScope == .local ? model.library.folder?.name ?? String(localized: "Files") : String(localized: "Files"))
                .searchable(text: $model.filesSearch, placement: .navigationBarDrawer(displayMode: .always), prompt: model.filesScope == .cloud ? "Search cloud backups" : "Search files")
                .toolbar { toolbar }
                .navigationDestination(isPresented: $showingTransfers) { TransfersView() }
                .task(id: model.filesQueryKey) {
                    try? await Task.sleep(for: .milliseconds(200))
                    guard !Task.isCancelled else { return }
                    await model.loadFiles()
                }
                .task { await model.loadCloudBackups(refresh: true); await model.fileTransfers.load(model) }
        } detail: { id in
            if let id { FileDetailDestination(id: id).id(id) }
            else { ContentUnavailableView("Select a file", image: AppGlyph.file.assetName) }
        }
        .onChange(of: model.filesScope) { _, _ in selected = nil; model.filesSearch = ""; selecting = false; selection = [] }
        .onChange(of: model.filesSearch) { _, _ in selection = [] }
        .onChange(of: targets.map(\.id)) { _, ids in selection.formIntersection(ids) }
        .onChange(of: selected) { _, value in if value != nil { model.activity() } }
        .sheet(item: $sheet) { value in
            NavigationStack {
                switch value {
                case .trash: FileTrashView()
                case .help: FileLibraryHelpView()
                case .rename(let target): FileNameEditor(target: target, parent: model.filesFolder)
                case .create: FileNameEditor(target: nil, parent: model.filesFolder)
                case .backup(let files): FileBackupView(files: files)
                case .link(let target): FileLinkComposer(target: target)
                case .move(let targets): FileMoveView(items: targets)
                case .results(let values): FileResultsView(results: values)
                case .imports(let urls, let folder): FileImportView(urls: urls, folder: folder) { file in
                    model.filesScope = .local; model.filesFolder = folder; model.filesSearch = ""; selected = file.id
                }
                }
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            switch result {
            case .success(let urls): if !urls.isEmpty { sheet = .imports(urls, model.filesFolder) }
            case .failure(let error): model.reportFileError(error)
            }
        }
        // Presented from here because a picker placed inside the + menu closes with it.
        .photosPicker(isPresented: $choosingPhotos, selection: $photos, maxSelectionCount: 30, matching: .any(of: [.images, .videos]), preferredItemEncoding: .current)
        .onChange(of: photos) { _, values in if !values.isEmpty { Task { await importPhotos(values) } } }
        .fullScreenCover(isPresented: $capturing) {
            CameraCapture { result in
                capturing = false
                if let result { Task { await importCapture(result) } }
            }.ignoresSafeArea().task {
                // The camera's controls never reach the app, so a recording would run into
                // the idle lock and be lost. Bounded so a forgotten camera still locks.
                for _ in 0..<60 {
                    model.activity()
                    do { try await Task.sleep(for: .seconds(30)) } catch { return }
                }
            }
        }
        .alert("Camera access is unavailable. Allow camera access in Settings to take photos and videos.", isPresented: $cameraDenied) {
            Button("Open Settings") { if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) } }
            Button("Cancel", role: .cancel) {}
        }
        .confirmationDialog("Move to Trash", isPresented: Binding(get: { !removing.isEmpty }, set: { if !$0 { removing = [] } }), titleVisibility: .visible) {
            Button("Move to Trash", role: .destructive) { let values = removing; removing = []; Task { await trash(values) } }
        } message: { Text("Cloud backups are kept. This only changes the copy on this device.") }
    }

    @ViewBuilder private var localContent: some View {
        if model.filesQueryIsCurrent {
            if let folder = model.library.folder {
                Button { openFolder(folder.parent ?? "root") } label: {
                    Label("Parent folder", systemImage: "arrow.up").font(.subheadline)
                }.accessibilityIdentifier("files.parent")
                Text(([String(localized: "Files")] + (folder.path ?? []) + [folder.name]).joined(separator: " / "))
                    .font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                    .listRowSeparator(.hidden)
            }
            if let local = model.filesLocalOnly, local.unbackedVersions > 0 {
                VStack(alignment: .leading, spacing: 4) {
                    HStack(alignment: .top) {
                        Label { Text("Without a cloud backup") + Text(verbatim: " · \(local.unbackedVersions.formatted()) · \(fileSize(UInt64(max(0, local.unbackedBytes))))") }
                            icon: { Image(systemName: "exclamationmark.icloud") }
                        if textSize.isAccessibilitySize {
                            Button { sheet = .help } label: { Image(systemName: "info.circle").font(.system(size: 18)).frame(width: 44, height: 44) }
                                .buttonStyle(.borderless).accessibilityLabel("About files and backups")
                        }
                    }
                    if !textSize.isAccessibilitySize {
                        Text("Local files are not backed up automatically. Uninstalling dMsg removes files that have no cloud backup.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }.font(.footnote).accessibilityIdentifier("files.unbacked")
                    .listRowBackground(Color.clear).listRowSeparator(.hidden)
            }
            ForEach(model.library.folders) { folder in
                let target = LibraryItemTarget(id: folder.id, name: folder.name, folder: true)
                Button { if selecting { toggle(folder.id) } else { openFolder(folder.id) } } label: {
                    HStack(spacing: 10) {
                        if selecting { selectionIcon(folder.id) }
                        FileSummaryRow(name: folder.name, mime: "inode/directory", subtitle: String(localized: "Folder"))
                        if !selecting { Image(systemName: "chevron.forward").font(.caption).foregroundStyle(.tertiary) }
                    }.contentShape(Rectangle())
                }.buttonStyle(.plain).compactObjectRow().accessibilityIdentifier("folder.item.\(folder.id)")
                    .contextMenu { itemMenu(target) }
            }
            ForEach(files) { file in
                Group {
                    if selecting {
                        Button { toggle(file.id) } label: { HStack(spacing: 10) { selectionIcon(file.id); FileRow(file: file) } }
                            .buttonStyle(.plain).accessibilityAddTraits(selection.contains(file.id) ? .isSelected : [])
                    } else { NavigationLink(value: file.id) { FileRow(file: file) } }
                }.compactObjectRow().accessibilityIdentifier("file.item.\(file.id)")
                    .contextMenu {
                        Button("Back up to cloud", systemImage: "icloud.and.arrow.up") { sheet = .backup([file]) }
                        Button("Share by link", systemImage: "link") { sheet = .link(FileLinkTarget(file: file, remote: !file.canOpen)) }
                            .disabled(file.size > FileLinkTarget.maxBytes)
                        itemMenu(LibraryItemTarget(id: file.id, name: file.name, folder: false))
                    }
            }
        }
        if model.filesLoading || !model.filesQueryIsCurrent && !model.filesFailed { ProgressView("Loading…") }
        if model.filesFailed {
            FileInlineFailure(message: String(localized: "The operation could not be completed. Please try again.")) { Task { await model.loadFiles() } }
        } else if model.filesLoaded && model.filesQueryIsCurrent && targets.isEmpty && !model.filesLoading {
            if !model.filesSearch.isEmpty {
                ContentUnavailableView.search(text: model.filesSearch)
                Button("Clear search") { model.filesSearch = "" }
            } else {
                ContentUnavailableView("No files in this folder", image: AppGlyph.folder.assetName,
                    description: Text("Import a document to encrypt it on this device. Cloud backup is a separate action."))
                SettingsAction("Import file", systemImage: "plus") { importing = true }
                if !model.cloudBackups.isEmpty {
                    SettingsAction("Cloud backups", systemImage: "icloud") { model.filesScope = .cloud }
                }
            }
        }
    }

    private var transferEntry: some View {
        Section {
            TransferStatusView()
            if importingPhotos { ProgressView("Preparing and copying file…") }
            if model.filesPendingImports > 0 {
                NavigationLink { PendingImportsView() } label: { LabeledContent("Pending imports", value: model.filesPendingImports.formatted()) }
            }
            if model.filesSharedImports > 0 {
                NavigationLink { IncomingSharesView() } label: { LabeledContent("Shared with dMsg", value: model.filesSharedImports.formatted()) }
            }
        }
    }

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) { ApprovalButton() }
        ToolbarItemGroup(placement: .primaryAction) {
            Menu("Import", systemImage: "plus") {
                Button("Import file", systemImage: "doc") { importing = true }
                Button("Import photos or videos", systemImage: "photo.on.rectangle") { choosingPhotos = true }
                if cameraAvailable {
                    Button("Take photo or video", systemImage: "camera") { Task { await openCamera() } }
                }
                Button("New folder", systemImage: "folder.badge.plus") { sheet = .create }
            }.disabled(working || importingPhotos || model.transfer.running || !model.filesQueryIsCurrent)
                .accessibilityIdentifier("files.add")
            Menu("File tools", systemImage: "ellipsis.circle") {
                if model.filesScope == .local {
                    Button(selecting ? "Done" : "Select files", systemImage: "checkmark.circle") { selecting.toggle(); selection = [] }
                        .disabled(targets.isEmpty || !model.filesQueryIsCurrent)
                }
                Button("Trash", systemImage: "trash") { sheet = .trash }
                NavigationLink("Collect files") { FileRequestsView() }
                NavigationLink("Share links") { FileLinksView() }
                NavigationLink("Pending imports") { PendingImportsView() }
                NavigationLink("Shared with dMsg") { IncomingSharesView() }
                Button("About files and backups", systemImage: "info.circle") { sheet = .help }
            }.accessibilityIdentifier("files.tools")
        }
    }

    private var selectionBar: some View {
        HStack {
            Text(selection.count.formatted()).font(.subheadline.monospacedDigit()).accessibilityLabel("Selected files")
            Spacer()
            Menu("Actions", systemImage: "ellipsis.circle") {
                Button("Back up to cloud", systemImage: "icloud.and.arrow.up") {
                    sheet = .backup(files.filter { selection.contains($0.id) })
                }.disabled(chosen.contains { $0.folder })
                Button("Move to another folder", systemImage: "folder") { sheet = .move(chosen) }
                Button("Move to Trash", systemImage: "trash", role: .destructive) { removing = chosen }
            }.disabled(selection.isEmpty || working).frame(minWidth: 44, minHeight: 44).accessibilityIdentifier("files.selection.actions")
            Button("Done") { selecting = false; selection = [] }.frame(minHeight: 44)
        }.padding(.horizontal, 20).background(Theme.paper).accessibilityElement(children: .contain)
    }

    @ViewBuilder private func itemMenu(_ target: LibraryItemTarget) -> some View {
        Button("Rename", systemImage: "pencil") { sheet = .rename(target) }
        Button("Move to another folder", systemImage: "folder") { sheet = .move([target]) }
        Button("Move to Trash", systemImage: "trash", role: .destructive) { removing = [target] }
    }

    private func selectionIcon(_ id: String) -> some View {
        Image(systemName: selection.contains(id) ? "checkmark.circle.fill" : "circle").foregroundStyle(Theme.forest).accessibilityHidden(true)
    }

    private func toggle(_ id: String) { if !selection.insert(id).inserted { selection.remove(id) } }

    private func openFolder(_ id: String) {
        model.activity(); selected = nil; selection = []; model.filesSearch = ""; model.filesFolder = id
    }

    private func trash(_ values: [LibraryItemTarget]) async {
        guard !working else { return }; let stamp = model.epoch
        working = true; defer { working = false }
        var results: [FileActionResult] = []
        for target in values {
            guard stamp == model.epoch, !Task.isCancelled else { return }
            do {
                try await model.trashLibraryItem(target.id)
                results.append(FileActionResult(name: target.name, message: String(localized: "Moved to Trash")))
            } catch {
                guard stamp == model.epoch, let message = model.fileFailureMessage(error) else { return }
                results.append(FileActionResult(name: target.name, message: message, failed: true))
            }
        }
        guard stamp == model.epoch, !Task.isCancelled else { return }
        await model.loadFiles(); selection = []; selecting = false; sheet = .results(results)
    }

    private func importPhotos(_ values: [PhotosPickerItem]) async {
        guard !importingPhotos else { return }; let stamp = model.epoch, folder = model.filesFolder
        importingPhotos = true; defer { importingPhotos = false; photos = [] }
        var results: [FileActionResult] = []
        for value in values {
            guard stamp == model.epoch, !Task.isCancelled else { return }
            do {
                guard let copy = try await value.loadTransferable(type: PhotoImport.self) else { throw CocoaError(.fileReadUnknown) }
                defer { copy.finish() }
                guard stamp == model.epoch, !Task.isCancelled else { return }
                let file = try await model.importLibraryDocument(copy.file, folder: folder)
                results.append(FileActionResult(name: file.name, message: String(localized: "Imported to this device. Cloud backup is a separate action.")))
            } catch {
                guard stamp == model.epoch, let message = model.fileFailureMessage(error) else { return }
                results.append(FileActionResult(name: String(localized: "Photos and videos"), message: message, failed: true))
            }
        }
        guard stamp == model.epoch, !Task.isCancelled else { return }
        await model.loadFiles(); sheet = .results(results)
    }

    private var cameraAvailable: Bool {
        #if DEBUG
        if model.uiTesting { return !Set(ProcessInfo.processInfo.arguments).isDisjoint(with: ["--ui-camera", "--ui-camera-denied"]) }
        #endif
        return CameraCapture.isAvailable
    }

    private func openCamera() async {
        #if DEBUG
        if model.uiTesting {
            if ProcessInfo.processInfo.arguments.contains("--ui-camera-denied") { cameraDenied = true }
            else { await importCapture(Result { try model.cameraCaptureFixture() }) }
            return
        }
        #endif
        if await AVCaptureDevice.requestAccess(for: .video) { capturing = true } else { cameraDenied = true }
    }

    private func importCapture(_ capture: Result<PhotoImport, Error>) async {
        let stamp = model.epoch, folder = model.filesFolder
        importingPhotos = true; defer { importingPhotos = false }
        do {
            let copy = try capture.get()
            defer { copy.finish() }
            _ = try await model.importLibraryDocument(copy.file, folder: folder)
            guard stamp == model.epoch else { return }
            // The new file is listed even if the camera was opened from cloud backups or a search.
            model.filesScope = .local; model.filesSearch = ""
            await model.loadFiles()
            model.notice = String(localized: "Imported to this device. Cloud backup is a separate action.")
        } catch {
            if stamp == model.epoch { model.reportFileError(error) }
        }
    }
}
