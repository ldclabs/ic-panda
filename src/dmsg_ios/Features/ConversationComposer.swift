import PhotosUI
import SwiftUI
@preconcurrency import DMsgCore

/// The only reader of the draft, so typing redraws this bar and not the messages.
struct ConversationComposer: View {
    let channel: ChannelItem
    let restriction: String
    let receiving: Bool
    @Binding var replying: MessageItem?
    let willQueue: () -> Void
    let failed: () async -> Void
    @Environment(SessionModel.self) private var model
    @FocusState private var focused: Bool
    @State private var attachments: [LibraryFile] = []
    @State private var queuing = false
    @State private var importing = false
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
    private static let maxTextBytes = 16 * 1024
    private var draft: String { model.drafts[channel.id] ?? "" }
    private var tooLong: Bool { draft.utf8.count > Self.maxTextBytes }
    private var empty: Bool { draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && attachments.isEmpty }

    var body: some View {
        VStack(spacing: 6) {
            if !channel.canPublish { NoticeBar(text: restriction).padding(.horizontal) }
            if let replying {
                HStack(alignment: .top) {
                    VStack(alignment: .leading) {
                        Text(model.personName(replying.account)).font(.caption.bold())
                        Text(replying.text.isEmpty ? replying.attachmentName ?? "" : replying.text).font(.caption).lineLimit(2)
                    }
                    Spacer()
                    Button("Cancel reply", systemImage: "xmark.circle") { self.replying = nil }.labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44)
                }.padding(.horizontal)
            }
            if !attachments.isEmpty { attachmentStrip }
            if receiving || working { TransferStatusView().padding(.horizontal) }
            HStack(alignment: .bottom, spacing: 10) {
                attachMenu
                TextField("Write a message", text: Binding(get: { draft }, set: { model.setDraft(channel: channel.id, text: $0) }), axis: .vertical)
                    .lineLimit(1...5).focused($focused)
                    .padding(12).background(Theme.surface, in: RoundedRectangle(cornerRadius: 16))
                    .accessibilityIdentifier("message.draft").disabled(!channel.canPublish)
                Button { actionTask = Task { await send() } } label: { Image(systemName: "arrow.up").font(.headline).frame(width: 44, height: 44) }
                    .buttonStyle(.borderedProminent).buttonBorderShape(.circle).accessibilityLabel("Send message")
                    .accessibilityIdentifier("message.send").keyboardShortcut(.return, modifiers: .command)
                    .disabled(!channel.canPublish || empty || tooLong || queuing)
            }.padding(12).background(.bar)
            if tooLong { Text("This message is too long.").font(.caption).foregroundStyle(.red) }
        }
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
            if model.drafts[channel.id] == nil { model.setDraft(channel: channel.id, text: channel.draft) }
        }
        .onChange(of: draft) { _, value in
            model.activity()
            let previous = persistTask; previous?.cancel()
            let epoch = model.epoch, channelID = channel.id
            persistTask = Task {
                await previous?.value
                try? await Task.sleep(for: .milliseconds(500))
                guard !Task.isCancelled, epoch == model.epoch else { return }
                do {
                    try await model.engine?.saveDraft(channel: channelID, text: value)
                    if epoch == model.epoch { model.draftSaved(channel: channelID, text: value) }
                } catch { if epoch == model.epoch { model.report(error) } }
            }
        }
        .onDisappear {
            actionTask?.cancel()
            let persistence = persistTask; persistence?.cancel()
            guard ownerEpoch == model.epoch else { return }
            let value = draft, channelID = channel.id, engine = model.engine, epoch = ownerEpoch
            Task {
                await persistence?.value
                if epoch == model.epoch { try? await engine?.saveDraft(channel: channelID, text: value) }
            }
        }
    }

    private var attachMenu: some View {
        Menu("Attach", systemImage: "paperclip") {
            Button("Import photos or videos", systemImage: "photo.on.rectangle") { choosingPhotos = true }
            if model.cameraAvailable { Button("Take photo or video", systemImage: "camera") { openingCamera = true } }
            Button("Import file", systemImage: "doc.badge.plus") { importing = true }
            Button("Attach file", systemImage: "doc") { choosingFile = true }
            if canPasteImage { Button("Paste image", systemImage: "doc.on.clipboard") { actionTask = Task { await pasteImage() } } }
            Button("Share secret snapshot", systemImage: "lock.doc") { sharingSecret = true }
        }.labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44).disabled(!channel.canPublish || queuing || working)
    }

    private var attachmentStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(attachments) { file in
                    HStack(spacing: 8) {
                        Group {
                            if ThumbnailStore.supports(file) {
                                LibraryThumbnail(file: file).frame(width: 36, height: 36).clipShape(RoundedRectangle(cornerRadius: 8))
                            } else { ObjectIcon(name: fileSymbol(file.mime)) }
                        }
                        VStack(alignment: .leading, spacing: 1) {
                            Text(file.name).font(.caption.weight(.medium)).lineLimit(1).truncationMode(.middle)
                            Text(fileSize(file.size)).font(.caption2).foregroundStyle(.secondary)
                        }.frame(maxWidth: 160, alignment: .leading)
                        Button("Remove", systemImage: "xmark.circle") { attachments.removeAll { $0.id == file.id } }
                            .labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44)
                    }.padding(.leading, 8).background(Theme.surface, in: RoundedRectangle(cornerRadius: 12))
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
        let file = try await model.importLibraryDocument(url, folder: "root")
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
        guard !queuing, channel.canPublish else { return }
        willQueue()
        var text = draft, reply = replying
        let persistence = persistTask; persistence?.cancel()
        queuing = true; defer { queuing = false }
        let epoch = model.epoch
        do {
            await persistence?.value
            guard epoch == model.epoch, !Task.isCancelled else { return }
            for file in attachments.isEmpty ? [nil] : attachments.map(Optional.some) {
                let id: String
                #if DEBUG
                if model.uiTesting { id = UUID().uuidString }
                else { id = try await queue(text, reply: reply, file: file) }
                #else
                id = try await queue(text, reply: reply, file: file)
                #endif
                guard epoch == model.epoch, !Task.isCancelled else { return }
                model.clearQueuedDraft(channel: channel.id, sent: text); replying = nil
                attachments.removeAll { $0.id == file?.id }
                model.deliverMessage(channel: channel.id, item: OutgoingMessage(id: id, text: text, attachmentName: file?.name, attempted: false, error: nil))
                text = ""; reply = nil
            }
        } catch { if epoch == model.epoch, !Task.isCancelled { Haptics.failure(); await failed(); model.report(error) } }
    }

    private func queue(_ text: String, reply: MessageItem?, file: LibraryFile?) async throws -> String {
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        if let file { return try await engine.queueAttachmentReply(channel: channel.id, file: file.id, text: text, sequence: reply?.sequence, digest: reply?.digest) }
        if let reply { return try await engine.queueMessageAction(channel: channel.id, action: "reply", sequence: reply.sequence, digest: reply.digest, text: text) }
        return try await engine.queueMessage(channel: channel.id, text: text)
    }
}
