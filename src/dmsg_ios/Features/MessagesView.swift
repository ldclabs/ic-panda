import SwiftUI
@preconcurrency import DMsgCore

struct MessagesView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var selected: String?
    @State private var mode = 0
    @State private var creating = false
    @State private var inbox = InboxLoading()
    @State private var archived = false
    var body: some View {
        WorkspaceNavigation(selection: $selected) { listSelection in
            List(selection: listSelection) {
                Picker("Messages", selection: $mode) { Text("Channels").tag(0); Text("Inbox").tag(1) }
                    .pickerStyle(.segmented).listRowBackground(Color.clear)
                if mode == 0 {
                    ForEach(model.channels, id: \.id) { channel in
                        NavigationLink(value: channel.id) {
                            HStack(spacing: 10) {
                                ObjectIcon(glyph: .channels)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(channel.name).font(.body.weight(.medium)).lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 1)
                                    Text(channel.draft.isEmpty ? channel.preview : String(localized: "Draft: ") + channel.draft)
                                        .font(.caption).foregroundStyle(.secondary).lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 1)
                                }.frame(maxWidth: .infinity, alignment: .leading)
                                if channel.unread > 0 { Text(channel.unread.formatted()).font(.caption.bold()).padding(6).background(Theme.forest.opacity(0.1), in: Circle()) }
                            }
                        }.compactObjectRow().accessibilityIdentifier("channel.row.\(channel.id)")
                    }
                    if model.channels.isEmpty { ContentUnavailableView("No conversations yet", image: AppGlyph.channels.assetName, description: Text("Your authorized channels appear after synchronization.")) }
                } else {
                    Toggle("Archived", isOn: $archived).accessibilityIdentifier("inbox.archived")
                    let visible = inbox.letters.filter { $0.archived == archived && !model.blockedAccounts.contains($0.sender) }
                    ForEach(visible) { letter in
                        NavigationLink(value: letter.id) { InboxRow(letter: letter) }.compactObjectRow()
                            .accessibilityIdentifier("inbox.row.\(letter.id)")
                    }
                    if let failure = inbox.failure {
                        NoticeBar(text: failure).accessibilityIdentifier("inbox.failure")
                        Button("Retry") { Task { await loadLetters(refresh: true) } }
                            .disabled(inbox.refreshing).accessibilityIdentifier("inbox.retry")
                    } else if visible.isEmpty && inbox.known && !inbox.refreshing {
                        ContentUnavailableView("No letters yet", image: AppGlyph.inbox.assetName).accessibilityIdentifier("inbox.empty")
                    }
                    if inbox.refreshing || !inbox.known && inbox.failure == nil {
                        ProgressView("Syncing…").accessibilityIdentifier("inbox.loading")
                    }
                }
            }.listStyle(.plain).privateScreen().navigationTitle("Messages")
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { ApprovalButton() }
                    ToolbarItem(placement: .primaryAction) {
                        if mode == 0 { Button("New conversation", systemImage: "plus") { creating = true } }
                        else { NavigationLink { LetterComposerView() } label: { Label("Write a letter", systemImage: "square.and.pencil") } }
                    }
                }
                .task { await model.refresh() }
                .task(id: "\(mode):\(model.contentRevision):\(model.epoch)") {
                    if mode == 1 { await loadLetters(refresh: false) }
                }
                .task(id: "inbox:\(mode):\(model.epoch):\(model.concealed)") {
                    guard mode == 1, !model.concealed else { return }
                    await loadLetters(refresh: false)
                    while !Task.isCancelled {
                        await loadLetters(refresh: true)
                        do { try await Task.sleep(for: .seconds(30)) } catch { break }
                    }
                }
                .refreshable {
                    if mode == 1 { await loadLetters(refresh: true) } else { await model.synchronize() }
                }
        } detail: { id in
            if mode == 1, let letter = inbox.letters.first(where: { $0.id == id }) {
                InboxDetail(letter: letter) { await loadLetters(refresh: false) }
            } else if mode == 0, let channel = model.channels.first(where: { $0.id == id }) { ConversationView(channel: channel).id(channel.id) }
            else { ContentUnavailableView("Select a conversation", image: AppGlyph.channels.assetName) }
        }
        .onChange(of: selected) { _, value in if value != nil { model.activity() } }
        .sheet(isPresented: $creating) { NavigationStack { ConversationSetupView() } }
    }

    private func loadLetters(refresh: Bool) async {
        guard model.unlocked, !model.concealed else { return }
        if refresh { await inbox.refresh(read: readLetters) }
        else { await inbox.cached(read: readLetters) }
    }

    private func readLetters(refresh: Bool) async throws -> InboxSnapshot {
        let epoch = model.epoch
        #if DEBUG
        if model.uiTesting { return try await model.inboxFixture(refresh: refresh) }
        #endif
        guard let engine = model.engine else { throw CancellationError() }
        do {
            let snapshot = try await engine.inbox(refresh: refresh)
            let incoming: [InboxLetter] = try model.decode(snapshot)
            struct Observation: Decodable { let complete: Bool }
            let known: Observation = try model.decode(try await engine.cachedInboxStatus())
            guard epoch == model.epoch, !Task.isCancelled else { throw CancellationError() }
            return InboxSnapshot(letters: incoming, complete: known.complete)
        } catch {
            guard epoch == model.epoch, !Task.isCancelled else { throw CancellationError() }
            if case MobileError.Failure(let code) = error, code == "DEVICE_NOT_APPROVED" { model.report(error) }
            throw error
        }
    }
}

struct ConversationView: View {
    let channel: ChannelItem
    @Environment(SessionModel.self) private var model
    @State private var messages: [MessageItem] = []
    private var draft: String {
        get { model.drafts[channel.id] ?? "" }
        nonmutating set { model.setDraft(channel: channel.id, text: newValue) }
    }
    @State private var queuing = false
    @State private var queuedAt: ContinuousClock.Instant?
    @State private var outbox: [OutgoingMessage] = []
    @State private var ownerEpoch: UInt64 = 0
    @State private var choosingAttachment = false
    @State private var inviting = false
    @State private var sharingSecret = false
    @State private var secretPackage: SecretPackageImportView.ChannelSource?
    @State private var hasEarlier = false
    @State private var nextBefore: UInt64?
    @State private var oldestLoaded: UInt64?
    @State private var preserveMessage: String?
    @State private var nearBottom = true
    @State private var loadedQuery = ""
    @State private var messageRequest: UUID?
    @State private var persistTask: Task<Void, Never>?
    @State private var actionTask: Task<Void, Never>?
    @State private var loadingEarlier = false
    @State private var messagesKnown = false
    @State private var loadFailed = false
    @State private var refreshFailed = false
    @State private var refreshing = false
    @State private var query = ""
    @State private var replying: MessageItem?
    @State private var editingMessage: MessageEdit?
    @State private var retracting: MessageItem?
    @State private var reporting: ReportTarget?
    struct MessageEdit: Identifiable { let message: MessageItem; var id: String { message.id } }
    private var shownMessages: [MessageItem] {
        model.messageSending.messages(channel: channel.id, stored: messages)
            .filter { !model.blockedAccounts.contains($0.account) && (query.isEmpty || $0.text.localizedCaseInsensitiveContains(query)) }
    }
    private var shownOutbox: [OutgoingMessage] {
        // A verified local copy can arrive before the original send's ACK.
        let received = Set(shownMessages.filter { $0.account == model.session?.account }.map(\.id))
        return model.messageSending.items(channel: channel.id, stored: outbox, received: received)
    }
    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if refreshing || !messagesKnown && !loadFailed {
                        ProgressView("Syncing…").accessibilityIdentifier("conversation.loading")
                    }
                    if loadFailed || refreshFailed {
                        NoticeBar(text: String(localized: "Showing saved content. The latest sync did not finish."))
                        Button("Retry") { actionTask = Task { await refreshMessages() } }
                            .disabled(refreshing).accessibilityIdentifier("conversation.retry")
                    } else if messagesKnown && shownMessages.isEmpty && shownOutbox.isEmpty && !hasEarlier && !refreshing {
                        ContentUnavailableView("No messages", image: AppGlyph.channels.assetName)
                            .accessibilityIdentifier("conversation.empty")
                    }
                    if hasEarlier && query.isEmpty {
                        Button("Load earlier messages") { actionTask = Task { await loadEarlier() } }
                            .disabled(loadingEarlier).accessibilityIdentifier("conversation.earlier")
                    }
                    ForEach(shownMessages, id: \.id) { message in
                        messageBubble(message)
                            .reminderVisible(.channel(channel.id), watermark: message.sequence,
                                ready: query.isEmpty && !message.unreadable && message.id == shownMessages.last?.id) {
                                try? await model.engine?.markRead(channel: channel.id, through: message.sequence)
                            }
                            .id(message.id).accessibilityIdentifier("message.row.\(message.sequence)").contextMenu {
                            if message.account != model.session?.account {
                                Button("Report content") { reporting = ReportTarget(kind: "channel", account: message.account, resource: channel.id, sequence: message.sequence) }
                                Button("Block this account", role: .destructive) { Task { await model.blockAccount(message.account, blocked: true) } }
                            }
                            if !message.unreadable && !message.retracted {
                                Button("Reply") { replying = message }
                                if message.account == model.session?.account {
                                    Button("Edit message") { editingMessage = MessageEdit(message: message) }
                                    Button("Withdraw message", role: .destructive) { retracting = message }
                                }
                            }
                        }
                    }
                    // Pending and confirmed rows need distinct IDs so
                    // LazyVStack replaces the sending UI after an ACK.
                    ForEach(shownOutbox, id: \.id) { pending in
                        VStack(alignment: .leading, spacing: 8) {
                            if model.messageSending.active.contains(pending.id) {
                                ProgressView("Sending")
                                Text(pending.text)
                            } else if model.messageSending.contains(pending.id) {
                                Text("Waiting to submit").font(.caption).foregroundStyle(.secondary)
                                Text(pending.text)
                            } else if pending.error == "EXECUTION_UNKNOWN" || pending.attempted && pending.error == nil {
                                NoticeBar(text: String(localized: "Waiting for confirmation: ") + pending.text)
                            } else if pending.error != nil {
                                NoticeBar(text: String(localized: "The operation could not be completed. Please try again."))
                                Text(pending.text)
                            } else {
                                Text("Waiting to submit").font(.caption).foregroundStyle(.secondary)
                                Text(pending.text)
                            }
                            if let name = pending.attachmentName { Label(name, systemImage: "doc") }
                            if !model.messageSending.contains(pending.id) {
                                Button(LocalizedStringKey(pending.attempted ? "Reconcile original send" : "Send queued message")) {
                                    model.deliverMessage(channel: channel.id, item: pending)
                                }.accessibilityIdentifier("message.retry.\(pending.id)")
                            }
                        }.id("pending:\(pending.id)").accessibilityElement(children: .contain)
                            .accessibilityIdentifier("message.pending.\(pending.id)")
                    }
                }.padding(16)
            }.defaultScrollAnchor(.bottom)
                .onScrollGeometryChange(for: Bool.self) { geometry in
                    geometry.contentOffset.y + geometry.containerSize.height >= geometry.contentSize.height - 100
                } action: { _, value in nearBottom = value }
                .onChange(of: shownMessages.count) { _, _ in
                    if let preserveMessage { proxy.scrollTo(preserveMessage, anchor: .top); self.preserveMessage = nil }
                    else if nearBottom, let id = shownMessages.last?.id { proxy.scrollTo(id, anchor: .bottom) }
                }
                .onChange(of: shownOutbox.count) { old, count in
                    if count > old, let id = shownOutbox.last?.id {
                        proxy.scrollTo("pending:\(id)", anchor: .bottom)
                        if let queuedAt { MessageTiming.record(.queueVisible, since: queuedAt); self.queuedAt = nil }
                    }
                }
                .onChange(of: model.messageSending.revision) { _, _ in
                    if let confirmation = model.messageSending.lastConfirmation, confirmation.channel == channel.id {
                        MessageTiming.record(.ackVisible, since: confirmation.at)
                    }
                }
        }.background(Theme.paper).navigationTitle(channel.name).navigationBarTitleDisplayMode(.inline)
            .safeAreaInset(edge: .bottom) {
                VStack(spacing: 6) {
                if let replying { HStack { Text("Reply to message \(replying.sequence)").font(.caption); Spacer(); Button("Cancel reply") { self.replying = nil } }.padding(.horizontal) }
                HStack(alignment: .bottom, spacing: 10) {
                    Menu("Attach", systemImage: "paperclip") {
                        Button("Attach file", systemImage: "doc") { choosingAttachment = true }
                        Button("Share secret snapshot", systemImage: "lock.doc") { sharingSecret = true }
                    }.labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44)
                    TextField("Write a message", text: Binding(get: { draft }, set: { draft = $0 }), axis: .vertical).lineLimit(1...5)
                        .padding(12).background(Theme.surface, in: RoundedRectangle(cornerRadius: 16))
                        .accessibilityIdentifier("message.draft")
                    Button { actionTask = Task { await send() } } label: { Image(systemName: "arrow.up").font(.headline).frame(width: 44, height: 44) }
                        .buttonStyle(.borderedProminent).buttonBorderShape(.circle).accessibilityLabel("Send message")
                        .accessibilityIdentifier("message.send")
                        .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || queuing)
                }.padding(12).background(.bar)
                }
            }
            .toolbar {
                Button("Invite member", systemImage: "person.badge.plus") { inviting = true }
                NavigationLink { ChannelSettingsView(channel: channel.id) } label: { Label("Conversation settings", systemImage: "gearshape") }
            }
            .searchable(text: $query, prompt: "Search messages")
            .sheet(item: $reporting) { target in NavigationStack { ReportContentView(target: target) } }
            .sheet(item: $editingMessage) { target in NavigationStack { MessageEditView(channel: channel.id, message: target.message) { await load() } } }
            .confirmationDialog("Withdraw this message?", isPresented: Binding(get: { retracting != nil }, set: { if !$0 { retracting = nil } })) {
                Button("Withdraw", role: .destructive) { if let message = retracting { actionTask = Task { await withdraw(message) } } }
            }
            .sheet(isPresented: $choosingAttachment) {
                NavigationStack { AttachmentPicker(folder: "root") { file in
                    choosingAttachment = false
                    actionTask = Task { await attach(file) }
                } }
            }
            .sheet(isPresented: $inviting) { NavigationStack { ChannelInvitationView(channel: channel.id) } }
            .sheet(isPresented: $sharingSecret) { NavigationStack { SecretSnapshotItemPicker(channel: channel.id) } }
            .sheet(item: $secretPackage) { source in NavigationStack { SecretPackageImportView(source: source) } }
            .task(id: "\(channel.id):\(model.contentRevision):\(model.epoch):\(query)") {
                ownerEpoch = model.epoch; model.activeChannel = channel.id
                if model.drafts[channel.id] == nil { draft = channel.draft }
                if !query.isEmpty { try? await Task.sleep(for: .milliseconds(200)) }
                if !Task.isCancelled { await load() }
            }
            .task(id: "refresh:\(channel.id):\(model.epoch):\(model.concealed)") {
                guard !model.concealed else { return }
                while !Task.isCancelled {
                    await refreshMessages()
                    do { try await Task.sleep(for: .seconds(30)) } catch { break }
                }
            }
            .task(id: "attachments:\(channel.id):\(model.epoch):\(model.concealed)") {
                guard !model.concealed, let engine = model.engine else { return }
                while !Task.isCancelled {
                    // Optional attachment maintenance has its own lifecycle;
                    // it cannot keep text loading or mark a send unconfirmed.
                    do {
                        try await coreOperation { try await engine.refreshChannelAttachments(channel: channel.id, operation: $0) }
                        if !Task.isCancelled { await load() }
                    } catch { /* Keep saved metadata; retry on the next visible pass. */ }
                    do { try await Task.sleep(for: .seconds(30)) } catch { break }
                }
            }
            .onChange(of: draft) { _, value in
                model.activity(); let previous = persistTask; previous?.cancel()
                let epoch = model.epoch, channelID = channel.id
                persistTask = Task {
                    await previous?.value
                    guard !Task.isCancelled, epoch == model.epoch else { return }
                    do { try await model.engine?.saveDraft(channel: channelID, text: value); if epoch == model.epoch { model.draftSaved(channel: channelID, text: value) } }
                    catch { if epoch == model.epoch { model.report(error) } }
                }
            }
            .onDisappear {
                actionTask?.cancel(); let persistence = persistTask; persistence?.cancel()
                if model.activeChannel == channel.id { model.activeChannel = nil }
                guard ownerEpoch == model.epoch else { return }
                let value = draft, channelID = channel.id
                let engine = model.engine, epoch = ownerEpoch
                Task { await persistence?.value; if epoch == model.epoch { try? await engine?.saveDraft(channel: channelID, text: value) } }
            }
    }

    private func messageBubble(_ message: MessageItem) -> some View {
                        HStack {
                            if message.account == model.session?.account { Spacer(minLength: 36) }
                            VStack(alignment: .leading, spacing: 8) {
                                Text(message.account).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                                Text(message.retracted ? String(localized: "Message withdrawn") : message.unreadable ? String(localized: "This message could not be verified.") : message.text).textSelection(.enabled)
                                if let reference = message.replySequence { Text("Reply to message \(reference)").font(.caption).foregroundStyle(.secondary) }
                                if message.edited { Text("Edited").font(.caption).foregroundStyle(.secondary) }
                                if let name = message.attachmentName {
                                    Button {
                                        if message.attachmentMime == "application/vnd.dmsg.secret-package" {
                                            secretPackage = .init(channel: channel.id, sequence: message.sequence)
                                        } else { actionTask = Task { await receiveAttachment(message) } }
                                    } label: {
                                        VStack(alignment: .leading, spacing: 4) {
                                            Label(name, systemImage: message.attachmentMime == "application/vnd.dmsg.secret-package" ? "lock.doc" : "doc.badge.arrow.down")
                                            if message.attachmentMime == "application/vnd.dmsg.secret-package" { Text("Preview and choose what to import").font(.caption) }
                                        }.font(.subheadline).padding(10)
                                    }.disabled(message.unreadable || message.attachmentDeleted)
                                        .background(Theme.forest.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
                                }
                                Text(Date(timeIntervalSince1970: Double(message.createdAtMs) / 1000), style: .time).font(.caption2).foregroundStyle(.secondary)
                            }.padding(12).background(message.account == model.session?.account ? Theme.forest.opacity(0.12) : Theme.surface,
                                                      in: RoundedRectangle(cornerRadius: 16))
                            if message.account != model.session?.account { Spacer(minLength: 36) }
                        }
    }

    private func refreshMessages() async {
        guard !refreshing, !model.concealed, !Task.isCancelled else { return }
        let epoch = model.epoch
        refreshing = true; refreshFailed = false
        // The core commits verified pages as it receives them. Read those
        // pages while the remaining history or attachment metadata is pending.
        // A slow later request must not leave the whole conversation blank.
        let local = Task {
            while !Task.isCancelled {
                await load()
                do { try await Task.sleep(for: .seconds(1)) } catch { break }
            }
        }
        defer { local.cancel(); refreshing = false }
        do {
            #if DEBUG
            if model.uiTesting { try await model.refreshConversationFixture() }
            else { try await refreshFromCore() }
            #else
            try await refreshFromCore()
            #endif
        } catch {
            if epoch == model.epoch, !Task.isCancelled {
                refreshFailed = true
                model.report(error)
            }
        }
        local.cancel()
        await local.value
        if epoch == model.epoch, !Task.isCancelled { await load(); await model.loadChannels() }
    }

    private func refreshFromCore() async throws {
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        _ = try await coreOperation { try await engine.refreshChannel(channel: channel.id, operation: $0) }
    }

    private func readMessages(before: UInt64? = nil) async throws -> MessagePage {
        #if DEBUG
        if model.uiTesting { return model.conversationFixture(before: before, query: query) }
        #endif
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        return query.isEmpty ? try await engine.messages(channel: channel.id, before: before, limit: 100)
            : try await engine.searchMessages(channel: channel.id, query: query)
    }

    private func load() async {
        guard !loadingEarlier || query != loadedQuery else { return }
        let started = ContinuousClock.now
        let epoch = model.epoch, query = query, request = UUID(), sendRevision = model.messageSending.revision
        messageRequest = request
        let earlier = query.isEmpty && loadedQuery.isEmpty
            ? oldestLoaded ?? (!nearBottom ? messages.first?.sequence : nil) : nil
        do {
            var page = try await readMessages()
            var fresh = page.items
            // Keep explicitly loaded history even after scrolling back down.
            // An initial sync still follows only the latest bounded page.
            if let earlier {
                while page.hasMore, let first = fresh.first, first.sequence > earlier {
                    guard epoch == model.epoch, !Task.isCancelled else { return }
                    page = try await readMessages(before: first.sequence)
                    guard !page.items.isEmpty else { break }
                    fresh = page.items + fresh
                }
            }
            guard epoch == model.epoch, self.query == query, messageRequest == request, !Task.isCancelled else { return }
            messages = fresh; hasEarlier = page.hasMore; nextBefore = page.nextBefore
            loadedQuery = query; messagesKnown = true; loadFailed = false
            MessageTiming.record(.localPage, since: started)
            guard let engine = model.engine else {
                model.messageSending.loaded(channel: channel.id, items: fresh, since: sendRevision)
                return
            }
            let pending = try await engine.outbox(channel: channel.id)
            guard epoch == model.epoch, self.query == query, messageRequest == request, !Task.isCancelled else { return }; outbox = pending.items
            model.messageSending.loaded(channel: channel.id, items: fresh, since: sendRevision)
        } catch {
            if epoch == model.epoch, self.query == query, messageRequest == request, !Task.isCancelled {
                loadFailed = true; model.report(error)
            }
        }
    }

    private func loadEarlier() async {
        guard hasEarlier, let before = nextBefore, before > 1, !loadingEarlier, query.isEmpty else { return }
        let epoch = model.epoch, request = UUID(), first = messages.first
        messageRequest = request
        loadingEarlier = true; defer { loadingEarlier = false }
        do {
            let page: MessagePage
            #if DEBUG
            if model.uiTesting { page = model.conversationFixture(before: before, query: "") }
            else { page = try await readEarlier(before: before) }
            #else
            page = try await readEarlier(before: before)
            #endif
            guard epoch == model.epoch, query.isEmpty, messageRequest == request, !Task.isCancelled else { return }
            preserveMessage = first?.id
            let rows = Dictionary((page.items + messages).map { ($0.sequence, $0) }, uniquingKeysWith: { first, _ in first })
            messages = rows.values.sorted { $0.sequence < $1.sequence }
            oldestLoaded = messages.first?.sequence; hasEarlier = page.hasMore; nextBefore = page.nextBefore
        } catch { if epoch == model.epoch, messageRequest == request, !Task.isCancelled { model.report(error) } }
    }

    private func readEarlier(before: UInt64) async throws -> MessagePage {
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        return try await coreOperation { try await engine.earlierMessages(channel: channel.id, before: before, limit: 100, operation: $0) }
    }

    private func send() async {
        model.activity()
        guard !queuing else { return }
        queuedAt = .now
        let text = draft, reply = replying
        let persistence = persistTask; persistence?.cancel()
        queuing = true; defer { queuing = false }
        let epoch = model.epoch
        do {
            await persistence?.value
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let id: String
            #if DEBUG
            if model.uiTesting { id = UUID().uuidString }
            else { id = try await queueText(text, reply: reply) }
            #else
            id = try await queueText(text, reply: reply)
            #endif
            guard epoch == model.epoch, !Task.isCancelled else { return }
            model.clearQueuedDraft(channel: channel.id, sent: text); replying = nil
            model.deliverMessage(channel: channel.id, item: OutgoingMessage(id: id, text: text, attachmentName: nil, attempted: false, error: nil))
        } catch { if epoch == model.epoch, !Task.isCancelled { await load(); model.report(error) } }
    }

    private func queueText(_ text: String, reply: MessageItem?) async throws -> String {
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        if let reply { return try await engine.queueMessageAction(channel: channel.id, action: "reply", sequence: reply.sequence, digest: reply.digest, text: text) }
        return try await engine.queueMessage(channel: channel.id, text: text)
    }

    private func attach(_ file: LibraryFile) async {
        guard let engine = model.engine else { return }
        let epoch = model.epoch, text = draft
        let persistence = persistTask; persistence?.cancel()
        do {
            await persistence?.value
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let id = try await engine.queueAttachment(channel: channel.id, file: file.id, text: text)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            model.clearQueuedDraft(channel: channel.id, sent: text)
            model.deliverMessage(channel: channel.id, item: OutgoingMessage(id: id, text: text, attachmentName: file.name, attempted: false, error: nil))
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func withdraw(_ message: MessageItem) async {
        guard let engine = model.engine else { return }; let epoch = model.epoch
        retracting = nil
        do {
            let id = try await engine.queueMessageAction(channel: channel.id, action: "retract", sequence: message.sequence, digest: message.digest, text: "")
            guard epoch == model.epoch else { return }
            model.deliverMessage(channel: channel.id, item: OutgoingMessage(id: id, text: "", attachmentName: nil, attempted: false, error: nil))
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func receiveAttachment(_ message: MessageItem) async {
        guard let engine = model.engine else { return }
        let epoch = model.epoch
        do {
            _ = try await coreOperation { try await engine.saveAttachment(channel: channel.id, sequence: message.sequence, operation: $0) }
            guard epoch == model.epoch, !Task.isCancelled else { return }
            model.notice = String(localized: "Verified attachment saved to Files.")
        } catch { model.report(error) }
    }
}

struct MessageEditView: View {
    let channel: String
    let message: MessageItem
    let changed: () async -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var working = false
    var body: some View {
        Form { TextEditor(text: $text).frame(minHeight: 180) }
            .privateScreen().navigationTitle("Edit message").onAppear { text = message.text }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Save") { Task { await save() } }.disabled(working || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty) }
            }
    }
    private func save() async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            let id = try await engine.queueMessageAction(channel: channel, action: "edit", sequence: message.sequence, digest: message.digest, text: text)
            guard epoch == model.epoch else { return }
            model.deliverMessage(channel: channel, item: OutgoingMessage(id: id, text: text, attachmentName: nil, attempted: false, error: nil))
            await changed(); dismiss()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
