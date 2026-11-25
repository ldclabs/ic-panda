import SwiftUI
import UniformTypeIdentifiers
@preconcurrency import DMsgCore

/// Fullscreen file preview supporting horizontal swiping, type filtering, pinch/double-tap
/// image zoom, safe native Markdown reading/source views, plain text, and metadata fallback.
/// Privacy: `privateScreen` (privacy-sensitive, and the notice host while shown) plus
/// immediate conceal/dismiss on lock or epoch change.
struct FullscreenFilePreviewView: View {
    let launch: FullscreenPreviewLaunch
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.layoutDirection) private var layoutDirection

    enum LoadedContent {
        case loading
        case image(image: UIImage, bytes: Data, mime: String)
        case markdown(document: MarkdownDocument, text: String, original: Data?)
        case text(text: String, original: Data?)
        case fallback(reason: PreviewFallbackReason, mime: String, name: String, size: UInt64, sha256: String?)
        case error(String)

        func usesImageLayout(for item: FullscreenPreviewItem) -> Bool {
            switch self {
            case .loading:
                // Keep the status bar backdrop and safe area stable while the image decodes.
                PreviewFilterLogic.previewClass(for: item) == .image && PreviewFilterLogic.isPreviewable(item)
            case .image: true
            default: false
            }
        }
    }

    enum MarkdownMode { case reading, source }

    @State private var allItems: [FullscreenPreviewItem]
    @State private var filter: PreviewFilter = .all
    @State private var activeID: String
    @State private var loadedForID: String?
    @State private var loaded: LoadedContent = .loading
    @State private var markdownMode: MarkdownMode = .reading
    @State private var showChrome = true
    @State private var loadTask: Task<Void, Never>?
    @State private var activeEpoch: UInt64 = 0

    @State private var detailFile: LibraryFile?
    @State private var showingDetails = false
    @State private var confirmShare = false
    @State private var shareTask: Task<Void, Never>?
    @State private var shareFile: ShareCopy?
    /// The copy to remove once the share sheet closes; SwiftUI clears `shareFile` before `onDismiss`.
    @State private var sharedCopy: ShareCopy?
    @State private var savingToLibrary = false

    struct ShareCopy: Identifiable {
        let id = UUID()
        let url: URL
        let opened: OpenedFile?
    }

    init(launch: FullscreenPreviewLaunch) {
        self.launch = launch
        _allItems = State(initialValue: launch.items)
        _activeID = State(initialValue: launch.initialID)
    }

    private var counts: [PreviewFilter: Int] { PreviewFilterLogic.counts(for: allItems) }
    private var filteredItems: [FullscreenPreviewItem] {
        allItems.filter { PreviewFilterLogic.matches($0, filter: filter) }
    }
    private var activeIndex: Int {
        filteredItems.firstIndex(where: { $0.id == activeID }) ?? 0
    }
    private var activeItem: FullscreenPreviewItem? {
        let items = filteredItems
        return items.first { $0.id == activeID } ?? items.first
    }
    private var canShareCurrent: Bool {
        guard let item = activeItem else { return false }
        switch item.source {
        case .library:
            if case .fallback(let reason, _, _, _, _) = loaded, reason == .cloudOnly || reason == .unavailable { return false }
            return item.canOpen
        case .channel:
            switch loaded {
            case .image, .markdown, .text: return true
            default: return false
            }
        }
    }

    var body: some View {
        let isImagePage = activeItem.map { item in
            let content = loadedForID == item.id ? loaded : .loading
            return content.usesImageLayout(for: item)
        } ?? false

        NavigationStack {
            ZStack {
                (isImagePage ? Theme.viewerBackdrop : Theme.paper)
                    .ignoresSafeArea()

                if model.concealed {
                    Theme.paper.ignoresSafeArea()
                } else {
                    TabView(selection: $activeID) {
                        ForEach(filteredItems) { item in
                            pageView(for: item)
                                .tag(item.id)
                        }
                    }
                    .tabViewStyle(.page(indexDisplayMode: .never))
                    .environment(\.colorScheme, isImagePage ? .dark : colorScheme)
                    .ignoresSafeArea(edges: isImagePage ? .all : .bottom)
                }
            }
            .safeAreaInset(edge: .top, spacing: 0) {
                if !model.concealed && !isImagePage {
                    topBar
                }
            }
            .overlay(alignment: .top) {
                if !model.concealed && isImagePage && showChrome {
                    topBar
                        .environment(\.colorScheme, .dark)
                        .transition(.opacity)
                }
            }
            .motion(Motion.fade, value: showChrome)
            .background { keyboardShortcuts }
            .navigationDestination(isPresented: $showingDetails) {
                if let file = detailFile {
                    FileDetailDestination(id: file.id, version: file.current, fromFullscreen: true)
                }
            }
            .sheet(item: $shareFile, onDismiss: cleanupShareCopy) { copy in
                ShareSheet(items: [copy.url])
            }
            .confirmationDialog(String(localized: "Share a copy"), isPresented: $confirmShare, titleVisibility: .visible) {
                Button(String(localized: "Share a copy")) {
                    shareTask?.cancel()
                    shareTask = Task { await prepareShareCopy() }
                }
            } message: {
                Text("Shared copies leave dMsg's protection and cannot be recalled.")
            }
            .toolbar(.hidden, for: .navigationBar)
        }
        .privateScreen(.plain)
        .onAppear {
            activeEpoch = model.epoch
            model.activity()
            if !model.unlocked { dismiss(); return }
            startLoadingCurrent()
        }
        .task(id: launch.channelID) {
            if let channelID = launch.channelID { await loadAllChannelAttachments(channelID: channelID) }
        }
        .onDisappear {
            clearMemory()
        }
        .onChange(of: activeID) { _, _ in
            shareTask?.cancel()
            model.activity()
            markdownMode = .reading
            showChrome = true
            startLoadingCurrent()
        }
        .onChange(of: model.unlocked) { _, unlocked in
            if !unlocked {
                clearMemory()
                dismiss()
            }
        }
        .onChange(of: model.epoch) { _, nextEpoch in
            if nextEpoch != activeEpoch {
                clearMemory()
                dismiss()
            }
        }
    }

    // MARK: - Top Bar

    private var topBar: some View {
        HStack(spacing: 8) {
            Button {
                clearMemory()
                dismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.primary)
                    .frame(width: 40, height: 40)
                    .glassSurface(in: Circle(), interactive: true, fallback: Theme.card)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(String(localized: "Close"))
            .accessibilityIdentifier("preview.close")

            if let item = activeItem {
                VStack(alignment: .leading, spacing: 1) {
                    Text(item.name)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.primary)
                        .lineLimit(1)
                    if filteredItems.count > 1 {
                        Text(verbatim: "\(activeIndex + 1) / \(filteredItems.count)")
                            .font(.footnote.monospacedDigit())
                            .foregroundStyle(.secondary)
                    } else if let versionLabel = item.versionLabel {
                        Text(versionLabel)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassSurface(in: Capsule(), fallback: Theme.card)
            } else {
                Spacer()
            }

            if launch.showFilter && allItems.count > 1 {
                Menu {
                    ForEach(PreviewFilter.allCases) { option in
                        let count = counts[option] ?? 0
                        Button {
                            applyFilter(option)
                        } label: {
                            if filter == option {
                                Label {
                                    Text(verbatim: "\(option.title) (\(count))")
                                } icon: {
                                    Image(systemName: "checkmark")
                                }
                            } else {
                                Text(verbatim: "\(option.title) (\(count))")
                            }
                        }
                        .disabled(count == 0)
                    }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: filter == .all ? "line.3.horizontal.decrease.circle" : "line.3.horizontal.decrease.circle.fill")
                            .font(.body)
                            .foregroundStyle(filter == .all ? Color.primary : Theme.forest)
                        if filter != .all {
                            Text(verbatim: "\(filter.title) · \(counts[filter] ?? 0)")
                                .font(.footnote.weight(.medium).monospacedDigit())
                                .foregroundStyle(Theme.forest)
                                .lineLimit(1)
                        }
                    }
                    .padding(.horizontal, filter == .all ? 10 : 12)
                    .frame(minWidth: 40, minHeight: 40)
                    .glassSurface(in: Capsule(), interactive: true, fallback: Theme.card)
                }
                .accessibilityLabel(String(localized: "Filter files"))
                .accessibilityIdentifier("preview.filter")
            }

            if case .markdown = loaded {
                Button {
                    markdownMode = markdownMode == .reading ? .source : .reading
                } label: {
                    Image(systemName: markdownMode == .reading ? "chevron.left.forwardslash.chevron.right" : "doc.richtext")
                        .font(.body)
                        .foregroundStyle(.primary)
                        .frame(width: 40, height: 40)
                        .glassSurface(in: Circle(), interactive: true, fallback: Theme.card)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(markdownMode == .reading ? String(localized: "Markdown source") : String(localized: "Reading"))
                .accessibilityIdentifier("preview.markdown.mode")
            }

            if let file = activeItem?.libraryFile {
                Button {
                    detailFile = file
                    showingDetails = true
                } label: {
                    Image(systemName: "info.circle")
                        .font(.body)
                        .foregroundStyle(.primary)
                        .frame(width: 40, height: 40)
                        .glassSurface(in: Circle(), interactive: true, fallback: Theme.card)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(String(localized: "File details"))
                .accessibilityIdentifier("file.details")
            }

            actionMenu
        }
        .padding(.horizontal, 12)
        .padding(.top, 8)
        .padding(.bottom, 6)
    }

    @ViewBuilder
    private var actionMenu: some View {
        if let item = activeItem, canShareCurrent || item.libraryFile == nil {
            Menu {
                switch loaded {
                case .image(_, let bytes, let mime):
                    Button(String(localized: "Copy"), systemImage: "doc.on.doc") {
                        model.clipboard.copy(bytes, type: UTType(mimeType: mime) ?? .image)
                        model.notice = String(localized: "Copied")
                    }
                case .markdown(_, let text, _), .text(let text, _):
                    Button(String(localized: "Copy"), systemImage: "doc.on.doc") {
                        model.clipboard.copy(text)
                        model.notice = String(localized: "Copied")
                    }
                default:
                    EmptyView()
                }

                if case .channel = item.source {
                    Button(String(localized: "Save to Files"), systemImage: "folder.badge.plus") {
                        Task { await saveChannelAttachment(item) }
                    }
                    .disabled(savingToLibrary)
                }

                if canShareCurrent {
                    Button(String(localized: "Share a copy"), systemImage: "square.and.arrow.up") {
                        confirmShare = true
                    }
                }
            } label: {
                Image(systemName: "ellipsis")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.primary)
                    .frame(width: 40, height: 40)
                    .glassSurface(in: Circle(), interactive: true, fallback: Theme.card)
            }
            .accessibilityLabel(String(localized: "Actions"))
            .accessibilityIdentifier("preview.actions")
        }
    }

    // MARK: - Page Body

    @ViewBuilder
    private func pageView(for item: FullscreenPreviewItem) -> some View {
        if item.id != activeID || loadedForID != item.id {
            VStack(spacing: 12) {
                ProgressView()
                Text(item.name)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            switch loaded {
            case .loading:
                ProgressView(String(localized: "Loading…"))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)

            case .image(let image, _, _):
                ZoomableImageView(image: image) {
                    showChrome.toggle()
                }
                .accessibilityIdentifier("file.preview")

            case .markdown(let document, let raw, _):
                ScrollView {
                    Group {
                        if markdownMode == .reading {
                            MarkdownDocumentView(document: document)
                        } else {
                            Text(verbatim: raw)
                                .font(.footnote.monospaced())
                                .foregroundStyle(.primary)
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    .padding(.horizontal, 20)
                    .padding(.top, 16)
                    .padding(.bottom, 32)
                }
                .accessibilityIdentifier("file.preview")

            case .text(let value, _):
                ScrollView {
                    Text(verbatim: value)
                        .font(.footnote.monospaced())
                        .foregroundStyle(.primary)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 20)
                        .padding(.top, 16)
                        .padding(.bottom, 32)
                }
                .accessibilityIdentifier("file.preview")

            case .fallback(let reason, let mime, let name, let size, let sha256):
                ScrollView {
                    fallbackCard(item: item, reason: reason, mime: mime, name: name, size: size, sha256: sha256)
                        .padding(.horizontal, 20)
                        .padding(.top, 20)
                        .padding(.bottom, 32)
                }
                .accessibilityIdentifier("preview.fallback")

            case .error(let message):
                VStack(spacing: 14) {
                    NoticeBar(text: message)
                    Button(String(localized: "Retry")) {
                        startLoadingCurrent()
                    }
                    .buttonStyle(.borderedProminent)
                }
                .padding(.horizontal, 24)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    private func fallbackCard(
        item: FullscreenPreviewItem,
        reason: PreviewFallbackReason,
        mime: String,
        name: String,
        size: UInt64,
        sha256: String?
    ) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .top, spacing: 12) {
                ObjectIcon(name: fileSymbol(mime))
                VStack(alignment: .leading, spacing: 4) {
                    Text(name.isEmpty ? item.name : name)
                        .font(.headline)
                        .foregroundStyle(.primary)
                    Text(reason.message)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }
            Divider()
            VStack(alignment: .leading, spacing: 8) {
                LabeledContent(String(localized: "Type"), value: UTType(mimeType: mime)?.localizedDescription ?? (mime.isEmpty ? "—" : mime))
                LabeledContent(String(localized: "Size"), value: fileSize(size))
                if let updated = item.updatedAt, updated > 0 {
                    LabeledContent(String(localized: "Modified")) {
                        Text(fileDate(updated), format: .dateTime)
                    }
                }
                if let digest = sha256 ?? item.sha256, !digest.isEmpty {
                    LabeledContent {
                        Text(verbatim: String(digest.prefix(16)) + "…")
                            .font(.footnote.monospaced())
                            .foregroundStyle(.secondary)
                    } label: {
                        Text(verbatim: "SHA-256")
                    }
                }
            }
            .font(.subheadline)

            let layout = dynamicTypeSize.isAccessibilitySize
                ? AnyLayout(VStackLayout(spacing: 10))
                : AnyLayout(HStackLayout(spacing: 12))
            layout {
                if case .channel = item.source {
                    Button {
                        Task { await saveChannelAttachment(item) }
                    } label: {
                        Label(String(localized: "Save to Files"), systemImage: "folder.badge.plus")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(savingToLibrary)
                } else if let file = item.libraryFile {
                    if canShareCurrent {
                        Button {
                            confirmShare = true
                        } label: {
                            Label(String(localized: "Share a copy"), systemImage: "square.and.arrow.up")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    Button {
                        detailFile = file
                        showingDetails = true
                    } label: {
                        Label(String(localized: "File details"), systemImage: "info.circle")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                }
            }
        }
        .padding(20)
        .frame(maxWidth: 440)
        .background(Theme.card, in: RoundedRectangle(cornerRadius: Theme.radiusLarge, style: .continuous))
        .frame(maxWidth: .infinity)
    }

    // MARK: - Keyboard Shortcuts (iPadOS)

    private var keyboardShortcuts: some View {
        // Pages run right to left in RTL layouts, so the arrows follow them.
        let leftStep = layoutDirection == .rightToLeft ? 1 : -1
        return Group {
            Button(action: { step(leftStep) }) { EmptyView() }
                .keyboardShortcut(.leftArrow, modifiers: [])
            Button(action: { step(-leftStep) }) { EmptyView() }
                .keyboardShortcut(.rightArrow, modifiers: [])
            Button(action: { clearMemory(); dismiss() }) { EmptyView() }
                .keyboardShortcut(.escape, modifiers: [])
        }
        .accessibilityHidden(true)
    }

    // MARK: - Actions & Loading

    private func applyFilter(_ next: PreviewFilter) {
        guard next != filter, (counts[next] ?? 0) > 0 else { return }
        let picked = PreviewFilterLogic.pickNearest(items: allItems, currentID: activeID, filter: next)
        filter = next
        if let nextID = picked.activeID {
            activeID = nextID
        }
    }

    private func step(_ delta: Int) {
        let nextIndex = activeIndex + delta
        guard filteredItems.indices.contains(nextIndex) else { return }
        activeID = filteredItems[nextIndex].id
    }

    private func clearMemory() {
        shareTask?.cancel()
        shareTask = nil
        loadTask?.cancel()
        loadTask = nil
        loaded = .loading
        loadedForID = nil
        cleanupShareCopy()
    }

    private func startLoadingCurrent() {
        loadTask?.cancel()
        guard let item = activeItem else { return }
        loadedForID = item.id
        loaded = .loading
        let targetID = item.id
        let epoch = model.epoch
        loadTask = Task {
            await load(item: item, targetID: targetID, epoch: epoch)
        }
    }

    private func load(item: FullscreenPreviewItem, targetID: String, epoch: UInt64) async {
        #if DEBUG
        if model.uiTesting {
            let kind = PreviewFilterLogic.previewClass(for: item)
            if !item.canOpen {
                loaded = .fallback(reason: .cloudOnly, mime: item.mime, name: item.name, size: item.size, sha256: item.sha256)
                return
            }
            switch kind {
            case .markdown:
                let doc = MarkdownDocument(blocks: [
                    .heading(level: 1, inlines: [.text("dMsg 全屏预览架构说明")]),
                    .paragraph(inlines: [
                        .text("Supports "),
                        .strong([.text("Markdown reading")]),
                        .text(", "),
                        .emphasis([.text("italic")]),
                        .text(", "),
                        .strikethrough([.text("removed")]),
                        .text(" and "),
                        .code("dmsg preview"),
                        .text("."),
                    ]),
                    .blockQuote(blocks: [
                        .paragraph(inlines: [.text("Raw HTML is stripped and external images stay blocked.")]),
                    ]),
                    .list(ordered: false, start: 1, items: [
                        MarkdownListItem(checked: true, blocks: [.paragraph(inlines: [.text("Pinch & double-tap image zoom")])]),
                        MarkdownListItem(checked: true, blocks: [.paragraph(inlines: [.text("Filter by All, Previewable, Images, Documents")])]),
                        MarkdownListItem(checked: false, blocks: [.paragraph(inlines: [.text("Future formats on demand")])]),
                    ]),
                    .table(
                        alignments: [.left, .center, .right],
                        headers: [[.text("Type")], [.text("Limit")], [.text("Renderer")]],
                        rows: [
                            [[.text("Image")], [.text("20 MiB")], [.text("Bounded")]],
                            [[.text("Markdown")], [.text("2 MiB")], [.text("Safe AST")]],
                        ]
                    ),
                    .codeBlock(language: "swift", code: "let limit = PreviewFilterLogic.maxMarkdownBytes\nprint(limit)"),
                    .paragraph(inlines: [.blockedImage(alt: "External diagram")]),
                    .paragraph(inlines: [.link(url: "https://dmsg.net", children: [.text("dMsg Documentation")])]),
                ])
                loaded = .markdown(
                    document: doc,
                    text: "# dMsg 全屏预览架构说明\n\nSupports **Markdown reading**, *italic*, ~~removed~~ and `dmsg preview`.",
                    original: nil
                )
            case .text:
                loaded = .text(text: "A preview of this file.", original: nil)
            case .image:
                let img = UIGraphicsImageRenderer(size: CGSize(width: 640, height: 480)).image { ctx in
                    NativeTheme.forest.setFill()
                    ctx.fill(ctx.format.bounds)
                    UIColor(Theme.card).setFill()
                    UIBezierPath(roundedRect: CGRect(x: 170, y: 90, width: 300, height: 300), cornerRadius: Theme.radiusLarge).fill()
                    NativeTheme.forest.setFill()
                    UIBezierPath(ovalIn: CGRect(x: 255, y: 175, width: 130, height: 130)).fill()
                }
                loaded = .image(image: img, bytes: img.pngData() ?? Data(), mime: "image/png")
            case .unsupported:
                loaded = .fallback(reason: .unsupportedType, mime: item.mime, name: item.name, size: item.size, sha256: item.sha256)
            }
            return
        }
        #endif

        guard let engine = model.engine else {
            loaded = .error(String(localized: "The operation could not be completed. Please try again."))
            return
        }
        do {
            let payload: PreviewPayload
            switch item.source {
            case .library(let id, let version):
                payload = try await coreOperation { try await engine.previewLibraryFile(id: id, version: version, operation: $0) }
            case .channel(let channel, let sequence):
                payload = try await coreOperation { try await engine.previewChannelAttachment(channel: channel, sequence: sequence, operation: $0) }
            }
            guard !Task.isCancelled, epoch == model.epoch, activeID == targetID else { return }

            if let reasonRaw = payload.fallbackReason {
                let reason = PreviewFallbackReason(rawValue: reasonRaw) ?? .unsupportedType
                loaded = .fallback(reason: reason, mime: payload.mime, name: payload.name, size: payload.size, sha256: payload.sha256)
                return
            }

            switch payload.kind {
            case "image":
                guard let data = payload.imageBytes, let image = await PreviewImageDecoder.decode(data) else {
                    guard !Task.isCancelled, epoch == model.epoch, activeID == targetID else { return }
                    loaded = .fallback(reason: .decodeFailed, mime: payload.mime, name: payload.name, size: payload.size, sha256: payload.sha256)
                    return
                }
                guard !Task.isCancelled, epoch == model.epoch, activeID == targetID else { return }
                loaded = .image(image: image, bytes: data, mime: payload.mime)

            case "markdown":
                guard let json = payload.markdownJson,
                      let doc = try? JSONDecoder().decode(MarkdownDocument.self, from: Data(json.utf8)) else {
                    loaded = .fallback(reason: .decodeFailed, mime: payload.mime, name: payload.name, size: payload.size, sha256: payload.sha256)
                    return
                }
                loaded = .markdown(document: doc, text: payload.text ?? "", original: payload.originalTextBytes)

            case "text":
                loaded = .text(text: payload.text ?? "", original: payload.originalTextBytes)

            default:
                loaded = .fallback(reason: .unsupportedType, mime: payload.mime, name: payload.name, size: payload.size, sha256: payload.sha256)
            }
        } catch {
            guard !Task.isCancelled, epoch == model.epoch, activeID == targetID else { return }
            if let message = model.fileFailureMessage(error) {
                loaded = .error(message)
            }
        }
    }

    private func loadAllChannelAttachments(channelID: String) async {
        guard let engine = model.engine else { return }
        let epoch = model.epoch
        guard let snapshot = try? await engine.channelAttachments(channel: channelID),
              let records: [ChannelAttachmentRecord] = try? model.decode(snapshot),
              epoch == model.epoch, !Task.isCancelled else { return }
        let items = records.map { FullscreenPreviewItem.fromAttachment(channel: channelID, record: $0) }
        guard !items.isEmpty else { dismiss(); return }
        if !items.contains(where: { PreviewFilterLogic.matches($0, filter: filter) }) { filter = .all }
        let picked = PreviewFilterLogic.pickNearest(items: items, currentID: activeID, filter: filter)
        allItems = items
        if let id = picked.activeID { activeID = id }
    }

    private func saveChannelAttachment(_ item: FullscreenPreviewItem) async {
        guard case .channel(let channelID, let sequence) = item.source,
              let engine = model.engine, !savingToLibrary else { return }
        let epoch = model.epoch
        savingToLibrary = true
        defer { savingToLibrary = false }
        do {
            _ = try await model.transfer.run(item.name) {
                try await engine.saveAttachment(channel: channelID, sequence: sequence, operation: $0)
            }
            guard epoch == model.epoch, !Task.isCancelled else { return }
            model.notice = String(localized: "Verified attachment saved to Files.")
            await model.loadFiles(quiet: true)
        } catch {
            if epoch == model.epoch, !Task.isCancelled {
                model.reportFileError(error)
            }
        }
    }

    private func prepareShareCopy() async {
        guard let item = activeItem, !model.concealed, !Task.isCancelled else { return }
        let epoch = model.epoch
        do {
            switch item.source {
            case .library(let id, let version):
                struct Opened: Decodable { let path: String; let nonce: String }
                guard let engine = model.engine, let targetVersion = version ?? item.libraryFile?.current else { return }
                let result = try await coreOperation { try await engine.openFile(id: id, version: targetVersion, operation: $0) }
                let opened = try JSONDecoder().decode(Opened.self, from: Data(result.json.utf8))
                let url = URL(fileURLWithPath: opened.path)
                let copy = OpenedFile(url: url, version: targetVersion, nonce: opened.nonce, engine: engine)
                guard epoch == model.epoch, !Task.isCancelled, !model.concealed, activeID == item.id else { return }
                try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: url.path)
                present(ShareCopy(url: url, opened: copy))

            case .channel:
                let bytes: Data
                switch loaded {
                case .image(_, let data, _): bytes = data
                case .markdown(_, let raw, let original), .text(let raw, let original): bytes = original ?? Data(raw.utf8)
                default: return
                }
                let fileURL = try await Task.detached { try PrivateFiles.previewShareCopy(bytes, name: item.name) }.value
                guard epoch == model.epoch, !Task.isCancelled, !model.concealed, activeID == item.id else {
                    try? FileManager.default.removeItem(at: fileURL.deletingLastPathComponent())
                    return
                }
                present(ShareCopy(url: fileURL, opened: nil))
            }
        } catch {
            if epoch == model.epoch, !Task.isCancelled {
                model.reportFileError(error)
            }
        }
    }

    private func present(_ copy: ShareCopy) {
        cleanupShareCopy()
        sharedCopy = copy
        shareFile = copy
    }

    private func cleanupShareCopy() {
        shareFile = nil
        guard let copy = sharedCopy else { return }
        sharedCopy = nil
        if copy.opened == nil {
            let dir = copy.url.deletingLastPathComponent()
            try? FileManager.default.removeItem(at: dir)
        }
    }
}
