import SwiftUI
import PhotosUI

/// Folders and files are pushed onto a real stack, so the back button, the swipe
/// and the path all work as they do elsewhere on the system.
struct FilesView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var textSize
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var columns: NavigationSplitViewVisibility = .automatic
    @State private var importing = false
    @State private var choosingPhotos = false
    @State private var photos: [PhotosPickerItem] = []
    @State private var openingCamera = false
    @State private var sheet: FileLibrarySheet?
    @State private var selecting = false
    @State private var selection: Set<String> = []
    @State private var removing: [LibraryItemTarget] = []
    @State private var working = false
    @State private var importingPhotos = false

    private var compact: Bool { sizeClass == .compact }
    private var current: LibraryListing { listing(model.filesFolder) ?? .empty }
    private func files(_ listing: LibraryListing) -> [LibraryFile] { model.filesSort.files(listing.files) }
    private func targets(_ listing: LibraryListing) -> [LibraryItemTarget] {
        listing.folders.map { LibraryItemTarget(id: $0.id, name: $0.name, folder: true) }
        + files(listing).map { LibraryItemTarget(id: $0.id, name: $0.name, folder: false) }
    }
    private var chosen: [LibraryItemTarget] { targets(current).filter { selection.contains($0.id) } }

    /// Search results belong to the folder on top; otherwise each folder keeps what it last held.
    private func listing(_ folder: String) -> LibraryListing? {
        if !model.filesSearch.isEmpty { return folder == model.filesFolder && model.filesQueryIsCurrent ? model.library : nil }
        return model.filesListings[folder]
    }

    var body: some View {
        @Bindable var model = model
        Group {
            if compact {
                NavigationStack(path: $model.route.filesPath) { stackRoot }
            } else {
                NavigationSplitView(columnVisibility: $columns) {
                    NavigationStack(path: $model.route.filesPath) { stackRoot }
                } detail: {
                    // A file's own pages push inside this column, and go when another file is selected.
                    NavigationStack {
                        if let id = model.route.file { FileDetailDestination(id: id).navigationBarTitleDisplayMode(.inline) }
                        else { ContentUnavailableView("Select a file", image: AppGlyph.file.assetName) }
                    }.id(model.route.file)
                }
                .navigationSplitViewStyle(.balanced)
                .onAppear { if model.route.file == nil { columns = .all } }
            }
        }
        .task(id: model.filesQueryKey) {
            try? await Task.sleep(for: .milliseconds(200))
            guard !Task.isCancelled else { return }
            await model.loadFiles(quiet: true)
        }
        .task {
            // Coming back to Files often should not reach the network each time.
            let recent = model.cloudBackupsObservedAt.map { Date.now.timeIntervalSince1970 * 1000 - Double($0) < 300_000 } ?? false
            await model.loadCloudBackups(refresh: !recent)
            await model.fileTransfers.load(model)
        }
        .onChange(of: model.filesScope) { _, _ in model.route.file = nil; model.filesSearch = ""; selecting = false; selection = [] }
        .onChange(of: model.filesSearch) { _, _ in selection = [] }
        .onChange(of: model.route.filesPath) { _, _ in model.activity(); model.filesSearch = ""; selecting = false; selection = [] }
        .onChange(of: targets(current).map(\.id)) { _, ids in selection.formIntersection(ids) }
        .onChange(of: model.route.file) { _, value in if value != nil { model.activity() } }
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
                    model.filesScope = .local; model.filesSearch = ""; show(file.id)
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
        .cameraImport(requested: $openingCamera) { await importCapture($0) }
        .confirmationDialog("Move to Trash", isPresented: Binding(get: { !removing.isEmpty }, set: { if !$0 { removing = [] } }), titleVisibility: .visible) {
            Button("Move to Trash", role: .destructive) { let values = removing; removing = []; Task { await trash(values) } }
        } message: { Text("Cloud backups are kept. This only changes the copy on this device.") }
    }

    private var stackRoot: some View {
        folderScreen("root")
            .workspaceRootTitle()
            .navigationDestination(for: FilesRoute.self) { route in
                switch route {
                case .folder(let id): folderScreen(id).navigationBarTitleDisplayMode(.inline)
                case .file(let id): FileDetailDestination(id: id).id(id)
                case .transfers: TransfersView()
                }
            }
    }

    private func browseBar(root: Bool) -> some View {
        @Bindable var model = model
        return FileBrowseBar(showsScopes: root, scope: $model.filesScope, sort: $model.filesSort) { model.route.filesPath.append(.transfers) }
    }

    private func folderScreen(_ id: String) -> some View {
        @Bindable var model = model
        let root = id == "root", shown = listing(id)
        // Large text scrolls the bar away on phones.
        let barInList = textSize.isAccessibilitySize && compact
        return VStack(spacing: 0) {
            if !barInList { browseBar(root: root) }
            List {
                if barInList {
                    browseBar(root: root).buttonStyle(.borderless)
                        .listRowInsets(EdgeInsets()).listRowBackground(Color.clear).listRowSeparator(.hidden)
                }
                Group {
                    transferEntry
                    if !root || model.filesScope == .local { localContent(id, listing: shown) }
                    else { CloudBackupSection(query: model.filesSearch, sort: model.filesSort) }
                }.plainListRows(selectable: !compact)
            }.listStyle(.plain)
                .refreshable {
                    if root && model.filesScope == .cloud { await model.loadCloudBackups(refresh: true) }
                    else { await model.loadFiles(quiet: true) }
                    await model.fileTransfers.load(model)
                }
                .safeAreaInset(edge: .bottom) { if selecting { selectionBar } }
                .dropDestination(for: URL.self) { urls, _ in
                    let files = urls.filter(\.isFileURL)
                    guard !files.isEmpty else { return false }
                    sheet = .imports(files, id)
                    return true
                }
        }.privateScreen().navigationTitle(root ? String(localized: "Files") : model.filesListings[id]?.folder?.name ?? "")
            .searchable(text: $model.filesSearch, placement: .navigationBarDrawer(displayMode: .always), prompt: root && model.filesScope == .cloud ? "Search cloud backups" : "Search files")
            .toolbar { toolbar(root: root) }
    }

    @ViewBuilder private func localContent(_ id: String, listing shown: LibraryListing?) -> some View {
        if let shown {
            if id == "root" { backupHint }
            ForEach(shown.folders) { folder in
                let target = LibraryItemTarget(id: folder.id, name: folder.name, folder: true)
                Group {
                    if selecting {
                        Button { toggle(folder.id) } label: {
                            HStack(spacing: 10) {
                                selectionIcon(folder.id)
                                FileSummaryRow(name: folder.name, mime: "inode/directory", subtitle: String(localized: "Folder"))
                            }.contentShape(Rectangle())
                        }.buttonStyle(.plain).accessibilityAddTraits(selection.contains(folder.id) ? .isSelected : [])
                    } else {
                        NavigationLink(value: FilesRoute.folder(folder.id)) {
                            FileSummaryRow(name: folder.name, mime: "inode/directory", subtitle: String(localized: "Folder"))
                        }
                    }
                }.compactObjectRow().accessibilityIdentifier("folder.item.\(folder.id)")
                    .contextMenu { itemMenu(target) }
            }
            ForEach(files(shown)) { file in
                fileRow(file)
                    .compactObjectRow().accessibilityIdentifier("file.item.\(file.id)")
                    .swipeActions {
                        Button("Move to Trash", systemImage: "trash") { removing = [LibraryItemTarget(id: file.id, name: file.name, folder: false)] }.tint(.red)
                        Button("Share by link", systemImage: "link") { sheet = .link(FileLinkTarget(file: file, remote: !file.canOpen)) }
                            .disabled(file.size > FileLinkTarget.maxBytes)
                    }
                    .contextMenu {
                        Button("Back up to cloud", glyph: .cloudUp) { sheet = .backup([file]) }
                        Button("Share by link", systemImage: "link") { sheet = .link(FileLinkTarget(file: file, remote: !file.canOpen)) }
                            .disabled(file.size > FileLinkTarget.maxBytes)
                        itemMenu(LibraryItemTarget(id: file.id, name: file.name, folder: false))
                    }
            }
        }
        if id == model.filesFolder {
            if shown == nil && !model.filesFailed { ProgressView("Loading…") }
            if model.filesFailed {
                FileInlineFailure(message: String(localized: "The operation could not be completed. Please try again.")) { Task { await model.loadFiles() } }
            } else if let shown, targets(shown).isEmpty, !model.filesLoading {
                if !model.filesSearch.isEmpty {
                    ContentUnavailableView.search(text: model.filesSearch)
                    Button("Clear search") { model.filesSearch = "" }
                } else {
                    ContentUnavailableView("No files in this folder", image: AppGlyph.folder.assetName,
                        description: Text("Import a document to encrypt it on this device. Cloud backup is a separate action."))
                    SettingsAction("Import file", systemImage: "plus") { importing = true }
                    if !model.cloudBackups.isEmpty && id == "root" {
                        SettingsAction("Cloud backups", glyph: .sync) { model.filesScope = .cloud }
                    }
                }
            }
        }
    }

    /// Files stay on this device unless backed up, which is worth saying once, at the top.
    @ViewBuilder private var backupHint: some View {
        if let local = model.filesLocalOnly, local.unbackedVersions > 0, let account = model.session?.account, !model.backupHintDismissed.contains(account) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .top) {
                    Label { Text("Without a cloud backup") + Text(verbatim: " · \(local.unbackedVersions.formatted()) · \(fileSize(UInt64(max(0, local.unbackedBytes))))") }
                        icon: { AppGlyph.cloudOff.image }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Button("About files and backups", systemImage: "info.circle") { sheet = .help }
                        .labelStyle(.iconOnly).buttonStyle(.borderless).frame(width: 44, height: 44)
                    Button("Dismiss", systemImage: "xmark") { model.dismissBackupHint() }
                        .labelStyle(.iconOnly).buttonStyle(.borderless).frame(width: 44, height: 44)
                        .accessibilityIdentifier("files.unbacked.dismiss")
                }
                if !textSize.isAccessibilitySize {
                    Text("Local files are not backed up automatically. Uninstalling dMsg removes files that have no cloud backup.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }.font(.footnote).accessibilityIdentifier("files.unbacked")
                .listRowBackground(Color.clear).listRowSeparator(.hidden)
        }
    }

    @ViewBuilder private func fileRow(_ file: LibraryFile) -> some View {
        if selecting {
            Button { toggle(file.id) } label: { HStack(spacing: 10) { selectionIcon(file.id); FileRow(file: file) } }
                .buttonStyle(.plain).accessibilityAddTraits(selection.contains(file.id) ? .isSelected : [])
        } else if compact {
            NavigationLink(value: FilesRoute.file(file.id)) { FileRow(file: file) }
        } else {
            Button { model.route.file = file.id } label: { FileRow(file: file).contentShape(Rectangle()) }
                .buttonStyle(.plain)
                .listRowBackground(model.route.file == file.id ? Theme.forest.opacity(0.1) : nil)
                .accessibilityAddTraits(model.route.file == file.id ? .isSelected : [])
        }
    }

    private func show(_ file: String) {
        if compact { model.route.filesPath.append(.file(file)) } else { model.route.file = file }
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

    @ToolbarContentBuilder private func toolbar(root: Bool) -> some ToolbarContent {
        if root { ToolbarItem(placement: .topBarLeading) { ActivityButton() } }
        ToolbarItemGroup(placement: .primaryAction) {
            Menu("Import", systemImage: "plus") {
                Button("Import file", systemImage: "doc") { importing = true }
                Button("Import photos or videos", systemImage: "photo.on.rectangle") { choosingPhotos = true }
                if model.cameraAvailable {
                    Button("Take photo or video", systemImage: "camera") { openingCamera = true }
                }
                Button("New folder", systemImage: "folder.badge.plus") { sheet = .create }
            }.disabled(working || importingPhotos || model.transfer.running || !model.filesQueryIsCurrent)
                .accessibilityIdentifier("files.add")
            Menu("File tools", systemImage: "ellipsis.circle") {
                if !root || model.filesScope == .local {
                    Button(selecting ? "Done" : "Select files", systemImage: "checkmark.circle") { selecting.toggle(); selection = [] }
                        .disabled(targets(current).isEmpty || !model.filesQueryIsCurrent)
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
                Button("Back up to cloud", glyph: .cloudUp) {
                    sheet = .backup(files(current).filter { selection.contains($0.id) })
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
        await model.loadFiles(); selection = []; selecting = false
        report(results, success: String(localized: "Moved to Trash"))
    }

    /// A clean run says so briefly; a results page is for what went wrong.
    private func report(_ results: [FileActionResult], success: String) {
        if results.contains(where: \.failed) { sheet = .results(results) } else { model.notice = success }
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
        await model.loadFiles()
        report(results, success: String(localized: "Imported to this device. Cloud backup is a separate action."))
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
