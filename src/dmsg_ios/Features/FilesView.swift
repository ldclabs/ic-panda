import SwiftUI
import PhotosUI

/// Folders and files are pushed onto a real stack, so the back button, the swipe
/// and the path all work as they do elsewhere on the system.
struct FilesView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.dynamicTypeSize) private var textSize
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var columns: NavigationSplitViewVisibility = .automatic
    @State private var importing = false
    @State private var choosingPhotos = false
    @State private var photos: [PhotosPickerItem] = []
    @State private var openingCamera = false
    @State private var sheet: FileLibrarySheet?
    /// A file opens out of its row's thumbnail on phones.
    @Namespace private var zoom
    @State private var selecting = false
    @State private var selection: Set<String> = []
    @State private var removing: [LibraryItemTarget] = []
    @State private var working = false
    @State private var importingPhotos = false
    @State private var searchScope: WorkspaceSearchScope = .current
    @State private var globalSearch = GlobalSearchStore()

    private var compact: Bool { sizeClass == .compact }
    private var filesFiltered: Bool {
        !model.filesSearch.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.filesFavoritesOnly || !model.filesTag.isEmpty
    }
    private var searchingAll: Bool {
        searchScope == .all && !model.filesSearch.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
    private var current: LibraryListing { listing(model.filesFolder) ?? .empty }
    private func files(_ listing: LibraryListing) -> [LibraryFile] {
        // Preserve relevance order from Rust when searching with the default Name sort.
        if !model.filesSearch.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && model.filesSort == .name {
            return listing.files
        }
        return model.filesSort.files(listing.files)
    }
    private func targets(_ listing: LibraryListing) -> [LibraryItemTarget] {
        listing.folders.map { LibraryItemTarget(id: $0.id, name: $0.name, folder: true) }
        + files(listing).map { LibraryItemTarget(id: $0.id, name: $0.name, folder: false) }
    }
    private var chosen: [LibraryItemTarget] { targets(current).filter { selection.contains($0.id) } }

    /// Search and filter results belong to the folder on top; otherwise each folder keeps what it last held.
    private func listing(_ folder: String) -> LibraryListing? {
        if filesFiltered { return folder == model.filesFolder && model.filesQueryIsCurrent ? model.library : nil }
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
            await model.loadFileTags()
            guard !model.localAccount else { return }
            // Coming back to Files often should not reach the network each time.
            let recent = model.cloudBackupsObservedAt.map { Date.now.timeIntervalSince1970 * 1000 - Double($0) < 300_000 } ?? false
            await model.loadCloudBackups(refresh: !recent)
            await model.fileTransfers.load(model)
        }
        .onChange(of: model.filesScope) { _, _ in model.route.file = nil; model.filesSearch = ""; searchScope = .current; selecting = false; selection = [] }
        .onChange(of: model.filesSearch) { _, value in
            selection = []
            if value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { searchScope = .current }
        }
        .onChange(of: model.route.filesPath) { _, _ in model.activity(); selecting = false; selection = [] }
        .onChange(of: targets(current).map(\.id)) { _, ids in selection.formIntersection(ids) }
        .onChange(of: model.route.file) { _, value in if value != nil { model.activity() } }
        .sheet(item: $sheet) { value in
            NavigationStack {
                switch value {
                case .trash: FileTrashView()
                case .help: FileLibraryHelpView()
                case .rename(let target): FileNameEditor(target: target, parent: model.filesFolder).presentationDetents([.medium, .large])
                case .create: FileNameEditor(target: nil, parent: model.filesFolder).presentationDetents([.medium, .large])
                case .backup(let files): FileBackupView(files: files)
                case .link(let target): FileLinkComposer(target: target)
                case .move(let targets): FileMoveView(items: targets)
                case .results(let values): FileResultsView(results: values)
                case .imports(let urls, let folder): FileImportView(urls: urls, folder: folder) { file in
                    model.revealLocalFileListing(in: folder); show(file.id)
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
        } message: { Text(fileTrashConfirmation(removing.map(\.name), localAccount: model.localAccount)) }
    }

    private var stackRoot: some View {
        folderScreen("root")
            .workspaceRootTitle()
            .navigationDestination(for: FilesRoute.self) { route in
                switch route {
                case .folder(let id): folderScreen(id).navigationBarTitleDisplayMode(.inline)
                case .file(let id):
                    let detail = FileDetailDestination(id: id).id(id)
                    if reduceMotion { detail }
                    else { detail.navigationTransition(.zoom(sourceID: id, in: zoom)) }
                case .transfers: TransfersView()
                case .unbacked: UnbackedVersionsView()
                }
            }
    }

    private func browseBar(root: Bool) -> some View {
        @Bindable var model = model
        return FileBrowseBar(showsScopes: root, scope: $model.filesScope, sort: $model.filesSort) { model.route.filesPath.append(.transfers) }
    }

    private func folderScreen(_ id: String) -> some View {
        let root = id == "root", shown = listing(id)
        // Large text scrolls the bar away on phones.
        let barInList = textSize.isAccessibilitySize && compact
        return List {
            if searchingAll {
                GlobalSearchResultsContent(store: globalSearch, query: model.filesSearch)
            } else {
            if barInList {
                browseBar(root: root).buttonStyle(.borderless)
                    .listRowInsets(EdgeInsets()).listRowBackground(Color.clear).listRowSeparator(.hidden)
            }
            Group {
                transferEntry
                if !root || model.filesScope == .local { localContent(id, listing: shown) }
                else { CloudBackupSection(query: model.filesSearch, sort: model.filesSort) }
            }.plainListRows(selectable: !compact)
            }
        }.listStyle(.plain)
            .globalSearch(store: globalSearch, query: model.filesSearch, active: searchingAll)
            .refreshable {
                if root && model.filesScope == .cloud { await model.loadCloudBackups(refresh: true) }
                else { await model.loadFiles(quiet: true) }
                await model.fileTransfers.load(model)
            }
            .scopeBar { if !barInList && !searchingAll { browseBar(root: root) } }
            .safeAreaInset(edge: .bottom) { if selecting { selectionBar } }
            .dropDestination(for: URL.self) { urls, _ in
                let files = urls.filter(\.isFileURL)
                guard !files.isEmpty else { return false }
                sheet = .imports(files, id)
                return true
            }
            .privateScreen(.plain).navigationTitle(root ? String(localized: "Files") : model.filesListings[id]?.folder?.name ?? "")
            .searchable(text: Binding(get: { model.filesSearches[id] ?? "" }, set: { model.setFileSearch($0, in: id) }),
                        placement: .navigationBarDrawer(displayMode: .always), prompt: root && model.filesScope == .cloud ? "Search cloud backups" : "Search files")
            .searchScopes($searchScope) {
                Text("Current").tag(WorkspaceSearchScope.current)
                Text("All").tag(WorkspaceSearchScope.all)
            }
            .toolbar { toolbar(root: root) }
    }

    @ViewBuilder private func localContent(_ id: String, listing shown: LibraryListing?) -> some View {
        if let shown {
            if id == "root" && !filesFiltered && !model.localAccount { backupHint }
            ForEach(shown.folders) { folder in
                let target = LibraryItemTarget(id: folder.id, name: folder.name, folder: true)
                let meta = filesFiltered ? model.folderSearchMeta[folder.id] : nil
                let subtitle = !filesFiltered ? String(localized: "Folder") : fileFolderPath(folder.path)
                Group {
                    if selecting {
                        Button { toggle(folder.id) } label: {
                            HStack(spacing: 10) {
                                selectionIcon(folder.id)
                                FileSummaryRow(name: folder.name, mime: "inode/directory", subtitle: subtitle, nameSpans: meta?.nameSpans, subtitleSpans: meta?.pathSpans)
                            }.contentShape(Rectangle())
                        }.buttonStyle(.plain).accessibilityAddTraits(selection.contains(folder.id) ? .isSelected : [])
                    } else {
                        NavigationLink(value: FilesRoute.folder(folder.id)) {
                            FileSummaryRow(name: folder.name, mime: "inode/directory", subtitle: subtitle, nameSpans: meta?.nameSpans, subtitleSpans: meta?.pathSpans)
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
                        if !model.localAccount { Button("Share by link", systemImage: "link") { sheet = .link(FileLinkTarget(file: file, remote: !file.canOpen)) }
                            .disabled(file.size > FileLinkTarget.maxBytes) }
                    }
                    .contextMenu {
                        if !model.localAccount { Button("Back up to cloud", glyph: .cloudUp) { sheet = .backup([file]) } }
                        if !model.localAccount { Button("Share by link", systemImage: "link") { sheet = .link(FileLinkTarget(file: file, remote: !file.canOpen)) }
                            .disabled(file.size > FileLinkTarget.maxBytes) }
                        itemMenu(LibraryItemTarget(id: file.id, name: file.name, folder: false))
                    }
            }
        }
        if id == model.filesFolder {
            if shown == nil && !model.filesFailed { PlaceholderRows(identifier: "files.loading") }
            if model.filesFailed {
                FileInlineFailure(message: String(localized: "The operation could not be completed. Please try again.")) { Task { await model.loadFiles() } }
            } else if let shown, targets(shown).isEmpty, !model.filesLoading {
                if filesFiltered {
                    ContentUnavailableView(model.filesSearch.isEmpty ? "No matching results" : LocalizedStringKey(model.filesSearch), systemImage: "magnifyingglass")
                    if !model.filesSearch.isEmpty { Button("Clear search") { model.filesSearch = "" } }
                    if model.filesFavoritesOnly || !model.filesTag.isEmpty {
                        Button("Clear filters") { model.filesFavoritesOnly = false; model.filesTag = "" }
                    }
                } else {
                    ContentUnavailableView("No files in this folder", image: AppGlyph.folder.assetName,
                        description: model.localAccount ? nil : Text("Import a document to encrypt it on this device. Cloud backup is a separate action."))
                    SettingsAction("Import file", systemImage: "plus") { importing = true }
                    if !model.localAccount && !model.cloudBackups.isEmpty && id == "root" {
                        SettingsAction("Cloud backups", glyph: .sync) { model.filesScope = .cloud }
                    }
                }
            }
        }
    }

    /// Files stay on this device unless backed up: one row at the top leads to the
    /// backup page, which explains it in its footer.
    @ViewBuilder private var backupHint: some View {
        if let local = model.filesLocalOnly, local.unbackedVersions > 0, let account = model.session?.account, !model.backupHintDismissed.contains(account) {
            HStack(spacing: 0) {
                Button { model.route.filesPath.append(.unbacked) } label: {
                    let count = Text(verbatim: local.unbackedVersions.formatted() + " · " + fileSize(UInt64(max(0, local.unbackedBytes))))
                        .foregroundStyle(.secondary)
                    if textSize.isAccessibilitySize {
                        VStack(alignment: .leading, spacing: 4) { Label("Without a cloud backup", glyph: .cloudOff); count }
                            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).contentShape(Rectangle())
                    } else {
                        HStack(spacing: 8) {
                            Label("Without a cloud backup", glyph: .cloudOff)
                            Spacer(minLength: 8)
                            count
                            Image(systemName: "chevron.forward").font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
                        }.frame(maxWidth: .infinity, minHeight: 44).contentShape(Rectangle())
                    }
                }.buttonStyle(.plain).accessibilityIdentifier("files.unbacked.review")
                Button { model.dismissBackupHint() } label: {
                    Label("Dismiss", systemImage: "xmark").labelStyle(.iconOnly).font(.footnote)
                        .foregroundStyle(.secondary).frame(minWidth: 44, minHeight: 44).contentShape(Rectangle())
                }.buttonStyle(.borderless).accessibilityIdentifier("files.unbacked.dismiss")
            }.font(.subheadline).accessibilityElement(children: .contain).accessibilityIdentifier("files.unbacked")
                .listRowInsets(EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 4))
        }
    }

    @ViewBuilder private func fileRow(_ file: LibraryFile) -> some View {
        let row = FileRow(file: file, showsLocation: filesFiltered)
        if selecting {
            Button { toggle(file.id) } label: { HStack(spacing: 10) { selectionIcon(file.id); row } }
                .buttonStyle(.plain).accessibilityAddTraits(selection.contains(file.id) ? .isSelected : [])
        } else if compact {
            NavigationLink(value: FilesRoute.file(file.id)) { row }.matchedTransitionSource(id: file.id, in: zoom)
        } else {
            Button { model.route.file = file.id } label: { row.contentShape(Rectangle()) }
                .buttonStyle(.plain)
                .listRowBackground(model.route.file == file.id ? Theme.fill : nil)
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
                    Section {
                        Button(selecting ? "Done" : "Select files", systemImage: "checkmark.circle") { selecting.toggle(); selection = [] }
                            .disabled(targets(current).isEmpty || !model.filesQueryIsCurrent)
                    }
                }
                Section {
                    if !model.localAccount { NavigationLink { FileRequestsView() } label: { Label("Collect files", systemImage: "tray.and.arrow.down") } }
                    if !model.localAccount { NavigationLink { FileLinksView() } label: { Label("Share links", systemImage: "link") } }
                }
                Section {
                    Button("Trash", systemImage: "trash") { sheet = .trash }
                    if !model.localAccount { Button("Without a cloud backup", glyph: .cloudOff) { model.route.filesPath.append(.unbacked) } }
                    Button(model.localAccount ? "About" : "About files and backups", systemImage: "info.circle") { sheet = .help }
                }
            }.accessibilityIdentifier("files.tools")
        }
    }

    private var selectionBar: some View {
        HStack {
            Text(selection.count.formatted()).font(.subheadline.monospacedDigit()).accessibilityLabel("Selected files").accessibilityValue(selection.count.formatted())
            Spacer()
            Menu("Actions", systemImage: "ellipsis.circle") {
                if !model.localAccount { Button("Back up to cloud", glyph: .cloudUp) {
                    sheet = .backup(files(current).filter { selection.contains($0.id) })
                }.disabled(chosen.contains { $0.folder }) }
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
        var failedIDs: Set<String> = []
        for target in values {
            guard stamp == model.epoch, !Task.isCancelled else { return }
            do {
                try await model.trashLibraryItem(target.id)
                results.append(FileActionResult(name: target.name, message: String(localized: "Moved to Trash")))
            } catch {
                guard stamp == model.epoch, let message = model.fileFailureMessage(error) else { return }
                failedIDs.insert(target.id)
                results.append(FileActionResult(name: target.name, message: message, failed: true))
            }
        }
        guard stamp == model.epoch, !Task.isCancelled else { return }
        await model.loadFiles()
        if selecting { selection = failedIDs; selecting = !failedIDs.isEmpty }
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
                results.append(FileActionResult(name: file.name, message: model.fileImportNotice))
            } catch {
                guard stamp == model.epoch, let message = model.fileFailureMessage(error) else { return }
                results.append(FileActionResult(name: String(localized: "Photos and videos"), message: message, failed: true))
            }
        }
        guard stamp == model.epoch, !Task.isCancelled else { return }
        if results.contains(where: { !$0.failed }) { model.revealLocalFileListing(in: folder) }
        await model.loadFiles()
        report(results, success: model.fileImportNotice)
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
            model.revealLocalFileListing(in: folder)
            await model.loadFiles()
            model.notice = model.fileImportNotice
        } catch {
            if stamp == model.epoch { model.reportFileError(error) }
        }
    }
}
