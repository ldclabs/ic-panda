import PhotosUI
import SwiftUI
@preconcurrency import DMsgCore

/// The only reader of the draft, so typing redraws this bar and not the messages.
struct ConversationComposer: View {
    let channel: ChannelItem
    let restriction: String
    let receiving: Bool
    let willQueue: () -> Void
    let failed: () async -> Void
    @Environment(SessionModel.self) private var model
    @FocusState private var focused: Bool
    private var attachments: [LibraryFile] {
        get { model.attachmentDrafts[channel.id] ?? [] }
        nonmutating set { model.attachmentDrafts[channel.id] = newValue.isEmpty ? nil : newValue }
    }
    private var replying: MessageItem? {
        get { model.replyDrafts[channel.id] }
        nonmutating set { model.replyDrafts[channel.id] = newValue }
    }
    private var queuing: Bool { model.queuingChannels.contains(channel.id) }
    @State private var importing = false
    @State private var sendFailures = 0
    @State private var working = false
    @State private var choosingFile = false
    @State private var choosingPhotos = false
    @State private var photos: [PhotosPickerItem] = []
    @State private var openingCamera = false
    @State private var sharingSecret = false
    @State private var persistTask: Task<Void, Never>?
    @State private var actionTask: Task<Void, Never>?
    @State private var ownerEpoch: UInt64 = 0
    @State private var canPasteImage = false

    private static let maxAttachmentBytes: UInt64 = 100 * 1024 * 1024
    private var draft: String { model.drafts[channel.id] ?? "" }
    private var tooLong: Bool { draft.utf8.count > ComposerQueue.maxTextBytes }
    private var empty: Bool { draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && attachments.isEmpty }

    var body: some View {
        VStack(spacing: 6) {
            if !channel.canPublish || replying != nil || !attachments.isEmpty || receiving || working || tooLong { extras }
            HStack(alignment: .bottom, spacing: 8) {
                attachMenu
                HStack(alignment: .bottom, spacing: 0) {
                    TextField("Write a message", text: Binding(get: { draft }, set: { model.activity(); model.setDraft(channel: channel.id, text: $0) }), axis: .vertical)
                        .lineLimit(1...5).focused($focused)
                        .padding(.vertical, 11).padding(.leading, 16).padding(.trailing, empty ? 16 : 0)
                        .accessibilityIdentifier("message.draft").disabled(!channel.canPublish)
                    // Like Messages, the send button appears once there is something to send.
                    if !empty {
                        Button { actionTask = Task { await send() } } label: {
                            Image(systemName: "arrow.up.circle.fill").font(.title).symbolRenderingMode(.palette)
                                .foregroundStyle(.white, Theme.forestFill).frame(width: 44, height: 44).contentShape(Rectangle())
                        }.buttonStyle(.plain).accessibilityLabel("Send message")
                            .accessibilityIdentifier("message.send").keyboardShortcut(.return, modifiers: .command)
                            .disabled(!channel.canPublish || tooLong || queuing)
                            .transition(.scale.combined(with: .opacity))
                    }
                }.frame(minHeight: 44)
                    .glassSurface(in: RoundedRectangle(cornerRadius: Theme.radiusField, style: .continuous), fallback: Theme.card)
                    .animation(.snappy(duration: 0.2), value: empty)
            }.padding(.horizontal, 12).padding(.vertical, 8)
        }
        .sensoryFeedback(.error, trigger: sendFailures)
        .sheet(isPresented: $choosingFile) {
            NavigationStack { AttachmentPicker(folder: "root") { file in
                choosingFile = false
                add(file)
            } }
        }
        .sheet(isPresented: $sharingSecret) { NavigationStack { SecretSnapshotItemPicker(channel: channel.id) } }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            switch result {
            case .success(let urls): actionTask = Task { await attach(urls) }
            case .failure(let error): model.reportFileError(error)
            }
        }
        // Presented from here because a picker placed inside a menu closes with it.
        .photosPicker(isPresented: $choosingPhotos, selection: $photos, maxSelectionCount: 10, matching: .any(of: [.images, .videos]), preferredItemEncoding: .current)
        .onChange(of: photos) { _, values in if !values.isEmpty { actionTask = Task { await attach(values) } } }
        .cameraImport(requested: $openingCamera) { await attach($0) }
        .onChange(of: replying?.id) { _, id in if id != nil { focused = true } }
        // Asking the pasteboard crosses into another process, so not on every keystroke.
        .onAppear { canPasteImage = UIPasteboard.general.hasImages }
        .onReceive(NotificationCenter.default.publisher(for: UIPasteboard.changedNotification)) { _ in canPasteImage = UIPasteboard.general.hasImages }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in canPasteImage = UIPasteboard.general.hasImages }
        .task(id: "\(channel.id):\(model.epoch)") {
            ownerEpoch = model.epoch
            if model.drafts[channel.id] == nil { model.drafts[channel.id] = channel.draft }
        }
        .onChange(of: draft) { _, _ in
            persistDraft(epoch: model.epoch, after: .milliseconds(500))
        }
        .onDisappear {
            actionTask?.cancel()
            let persistence = persistTask; persistence?.cancel()
            guard ownerEpoch == model.epoch else { return }
            let channelID = channel.id, epoch = ownerEpoch
            Task {
                await persistence?.value
                await model.saveEditedDraft(channel: channelID, epoch: epoch)
            }
        }
    }

    /// What goes with the next message, on a card above the field: restrictions, the
    /// reply, attachments, transfers and the length warning.
    private var extras: some View {
        VStack(spacing: 6) {
            if !channel.canPublish { NoticeBar(text: restriction).padding(.horizontal, 8) }
            if let replying {
                HStack(alignment: .top) {
                    VStack(alignment: .leading) {
                        Text(model.personName(replying.account)).font(.footnote.bold())
                            .loadMessagePerson(replying.account, model: model)
                        Text(replying.text.isEmpty ? replying.attachmentName ?? "" : replying.text).font(.footnote).lineLimit(2)
                    }
                    Spacer()
                    Button("Cancel reply", systemImage: "xmark.circle") { self.replying = nil }.labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44)
                }.padding(.leading, 16).padding(.trailing, 4)
            }
            if !attachments.isEmpty { attachmentStrip }
            if receiving || working { TransferStatusView().padding(.horizontal) }
            if tooLong {
                VStack {
                    Text("This message is too long.")
                    Text(utf8ByteCounter(draft, limit: Int64(ComposerQueue.maxTextBytes)))
                }.font(.footnote).foregroundStyle(.red).accessibilityElement(children: .combine)
            }
        }.padding(.vertical, 6).frame(maxWidth: .infinity)
            .glassSurface(in: RoundedRectangle(cornerRadius: Theme.radiusLarge, style: .continuous), fallback: Color.clear)
            .padding(.horizontal, 12).padding(.top, 6)
    }

    @discardableResult
    private func persistDraft(epoch: UInt64, after delay: Duration) -> Task<Void, Never> {
        let previous = persistTask; previous?.cancel()
        let channelID = channel.id
        let save = Task {
            await previous?.value
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            await model.saveEditedDraft(channel: channelID, epoch: epoch)
        }
        persistTask = save
        return save
    }

    private var attachMenu: some View {
        Menu("Attach", systemImage: "paperclip") {
            Button("Import photos or videos", systemImage: "photo.on.rectangle") { choosingPhotos = true }
            if model.cameraAvailable { Button("Take photo or video", systemImage: "camera") { openingCamera = true } }
            Button("Import file", systemImage: "doc.badge.plus") { importing = true }
            Button("Attach file", systemImage: "doc") { choosingFile = true }
            if canPasteImage { Button("Paste image", systemImage: "doc.on.clipboard") { actionTask = Task { await pasteImage() } } }
            Button("Share secret snapshot", systemImage: "lock.doc") { sharingSecret = true }
        }.labelStyle(.iconOnly).frame(width: 44, height: 44)
            .glassSurface(in: Circle(), interactive: true, fallback: Color.clear)
            .disabled(!channel.canPublish || queuing || working)
    }

    private var attachmentStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(attachments) { file in
                    HStack(spacing: 8) {
                        Group {
                            if ThumbnailStore.supports(file) {
                                LibraryThumbnail(file: file).frame(width: 36, height: 36).clipShape(RoundedRectangle(cornerRadius: Theme.radiusSmall))
                            } else { ObjectIcon(name: fileSymbol(file.mime)) }
                        }
                        VStack(alignment: .leading, spacing: 1) {
                            Text(file.name).font(.footnote.weight(.medium)).lineLimit(1).truncationMode(.middle)
                            Text(fileSize(file.size)).font(.footnote).foregroundStyle(.secondary)
                        }.frame(maxWidth: 160, alignment: .leading)
                        Button("Remove", systemImage: "xmark.circle") { attachments.removeAll { $0.id == file.id } }
                            .labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44)
                    }.padding(.leading, 8).background(Theme.card, in: RoundedRectangle(cornerRadius: Theme.radius))
                }
            }.padding(.horizontal)
        }.accessibilityIdentifier("message.attachment.draft")
    }

    private func add(_ file: LibraryFile) {
        if !attachments.contains(where: { $0.id == file.id }) { attachments.append(file) }
    }

    private func attach(_ urls: [URL]) async {
        await importing { for url in urls { try await attach(url: url) } }
    }

    private func attach(_ values: [PhotosPickerItem]) async {
        defer { photos = [] }
        await importing {
            for value in values {
                guard let copy = try await value.loadTransferable(type: PhotoImport.self) else { throw CocoaError(.fileReadUnknown) }
                defer { copy.finish() }
                try await attach(url: copy.file)
            }
        }
    }

    private func attach(_ capture: Result<PhotoImport, Error>) async {
        await importing {
            let copy = try capture.get()
            defer { copy.finish() }
            try await attach(url: copy.file)
        }
    }

    private func pasteImage() async {
        guard let image = UIPasteboard.general.image else { return }
        await attach(Result { try PhotoImport.capturedPhoto(image, at: .now) })
    }

    private func attach(url: URL) async throws {
        let epoch = model.epoch
        let file = try await model.importLibraryDocument(url, folder: "root", use: .attachment)
        guard file.size <= Self.maxAttachmentBytes else { throw CocoaError(.fileReadTooLarge) }
        if epoch == model.epoch { add(file) }
    }

    private func importing(_ work: () async throws -> Void) async {
        guard !working else { return }
        let epoch = model.epoch
        working = true; defer { working = false }
        do { try await work() }
        catch { if epoch == model.epoch { model.reportFileError(error) } }
    }

    /// One message per file; the text and the reply go with the first.
    private func send() async {
        model.activity()
        guard channel.canPublish, !empty, !tooLong, model.queuingChannels.insert(channel.id).inserted else { return }
        let workspace = model.viewGeneration
        willQueue()
        var text = draft, reply = replying
        let persistence = persistTask; persistence?.cancel()
        let files = attachments
        let epoch = model.epoch
        do {
            await persistence?.value
            for start in stride(from: 0, to: max(1, files.count), by: ComposerQueue.batchLimit) {
                try Task.checkCancellation()
                guard epoch == model.epoch else { break }
                let batch = Array(files.dropFirst(start).prefix(ComposerQueue.batchLimit))
                let prepared = model.composerQueue.prepare(channel: channel.id, text: text, files: batch, reply: reply)
                let ids = try await queue(prepared)
                guard workspace == model.viewGeneration else { return }
                guard ids.count == max(1, batch.count) else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
                model.composerQueue.finish(prepared)
                model.clearQueuedDraft(channel: channel.id, sent: text)
                if replying?.id == reply?.id { replying = nil }
                attachments.removeAll { file in batch.contains { $0.id == file.id && $0.current == file.current } }
                guard epoch == model.epoch else { break }
                for (index, id) in ids.enumerated() {
                    model.deliverMessage(channel: channel.id, item: OutgoingMessage(id: id, text: index == 0 ? text : "",
                        attachmentName: batch.isEmpty ? nil : batch[index].name, attempted: false, error: nil))
                }
                text = ""; reply = nil
            }
        } catch { if epoch == model.epoch, !Task.isCancelled { sendFailures &+= 1; await failed(); model.report(error) } }
        guard workspace == model.viewGeneration else { return }
        model.queuingChannels.remove(channel.id)
        // Use the same draft task as autosave so another send waits for this write.
        await persistDraft(epoch: epoch, after: .zero).value
    }

    private func queue(_ draft: MessageDraft) async throws -> [String] {
        #if DEBUG
        if model.uiTesting { return (0..<max(1, draft.attachments.count)).map { _ in UUID().uuidString } }
        #endif
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        return try await engine.queueAuthoredMessages(draft: draft)
    }
}
