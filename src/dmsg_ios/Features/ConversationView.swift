import SwiftUI
@preconcurrency import DMsgCore

struct ConversationView: View {
    let channel: ChannelItem
    @Environment(SessionModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var messages: [MessageItem] = []
    @State private var queuedAt: ContinuousClock.Instant?
    @State private var outbox: [OutgoingMessage] = []
    @State private var receivedFiles: [UInt64: LibraryFile] = [:]
    @State private var checkedAttachments: Set<UInt64> = []
    @State private var openingFile: LibraryFile?
    @State private var receiving: UInt64?
    @State private var firstUnread: UInt64?
    @State private var lastSeen: UInt64 = 0
    @State private var inviting = false
    @State private var showingSettings = false
    @State private var secretPackage: SecretPackageImportView.ChannelSource?
    @State private var hasEarlier = false
    @State private var nextBefore: UInt64?
    @State private var oldestLoaded: UInt64?
    @State private var preserveMessage: String?
    @State private var nearBottom = true
    @State private var loadedQuery = ""
    @State private var messageRequest: UUID?
    @State private var actionTask: Task<Void, Never>?
    @State private var loadingEarlier = false
    @State private var messagesKnown = false
    @State private var loadFailed = false
    @State private var refreshFailed = false
    @State private var refreshing = false
    @State private var refreshRequested = false
    @State private var query = ""
    @State private var searching = false
    @FocusState private var searchFocused: Bool
    @State private var replying: MessageItem?
    @State private var editingMessage: MessageEdit?
    @State private var selectingText: MessageEdit?
    @State private var retracting: MessageItem?
    private var currentChannel: ChannelItem { model.channels.first { $0.id == channel.id } ?? channel }
    private var searchQuery: String { query.trimmingCharacters(in: .whitespacesAndNewlines) }
    private static let bottom = "conversation.bottom"
    struct MessageEdit: Identifiable { let message: MessageItem; var id: String { message.id } }

    private func matches(_ text: String, _ attachment: String?) -> Bool {
        searchQuery.isEmpty || text.localizedCaseInsensitiveContains(searchQuery) || attachment?.localizedCaseInsensitiveContains(searchQuery) == true
    }

    var body: some View {
        let own = model.session?.account
        let merged = model.messageSending.messages(channel: channel.id, stored: messages)
        let shown = merged.filter { !model.blockedAccounts.contains($0.account) && matches($0.text, $0.attachmentName) }
        // A verified local copy can arrive before the original send's ACK.
        let received = Set(merged.filter { $0.account == own }.map(\.id))
        let pending = model.messageSending.items(channel: channel.id, stored: outbox, received: received).filter { matches($0.text, $0.attachmentName) }
        let rows = ConversationTimeline.rows(shown)
        let references = Dictionary(merged.map { ($0.sequence, $0) }, uniquingKeysWith: { _, next in next })
        let last = shown.last
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 4) {
                    // Saved messages stay on screen without comment while newer ones arrive.
                    if shown.isEmpty && (refreshing || !messagesKnown && !loadFailed) {
                        ProgressView("Syncing…").frame(maxWidth: .infinity).accessibilityIdentifier("conversation.loading")
                    }
                    if loadFailed || refreshFailed {
                        NoticeBar(text: String(localized: "Showing saved content. The latest sync did not finish."))
                        Button("Retry") { actionTask = Task { await refreshMessages() } }
                            .disabled(refreshing).accessibilityIdentifier("conversation.retry")
                    } else if messagesKnown && shown.isEmpty && pending.isEmpty && !hasEarlier && !refreshing {
                        ContentUnavailableView(searchQuery.isEmpty ? "No messages" : "No matching results", image: AppGlyph.channels.assetName)
                            .accessibilityIdentifier("conversation.empty")
                    }
                    if !searchQuery.isEmpty {
                        Text("Searches messages saved on this device.").font(.caption).foregroundStyle(.secondary)
                        LabeledContent("Messages", value: (shown.count + pending.count).formatted() + (hasEarlier ? "+" : "")).font(.caption).foregroundStyle(.secondary)
                    }
                    if hasEarlier {
                        LoadMoreRow(title: searchQuery.isEmpty ? "Load earlier messages" : "Load more", working: loadingEarlier, trigger: shown.count) { await loadEarlier() }
                            .accessibilityIdentifier("conversation.earlier")
                    }
                    ForEach(rows) { row in
                        VStack(alignment: .leading, spacing: 8) {
                            if row.startsDay {
                                Text(ConversationTimeline.dayTitle(ConversationTimeline.date(row.message.createdAtMs)))
                                    .font(.caption).foregroundStyle(.secondary).frame(maxWidth: .infinity).padding(.top, 8)
                            }
                            if row.message.sequence == firstUnread && searchQuery.isEmpty {
                                Label("Unread messages", systemImage: "circle.fill").font(.caption).foregroundStyle(Theme.forest).frame(maxWidth: .infinity)
                            }
                            bubble(row, own: row.message.account == own, references: references) { sequence in
                                actionTask = Task { await reveal(sequence, proxy: proxy) }
                            }
                        }.padding(.top, row.firstOfGroup ? 8 : 0).id(row.id)
                    }
                    // Pending and confirmed rows need distinct IDs so
                    // LazyVStack replaces the sending UI after an ACK.
                    ForEach(pending, id: \.id) { item in
                        pendingBubble(item).id("pending:\(item.id)").padding(.top, 8)
                    }
                    // The end of the conversation being on screen is what counts as read and as following along.
                    Color.clear.frame(height: 1).id(Self.bottom)
                        .reminderVisible(.channel(channel.id), watermark: last?.sequence, ready: query.isEmpty && last != nil && last?.unreadable == false) {
                            if let last { await model.channelRead(channel.id, through: last.sequence) }
                        }
                        .onScrollVisibilityChange(threshold: 0.01) { nearBottom = $0 }
                }.padding(16)
            }.defaultScrollAnchor(.bottom).scrollDismissesKeyboard(.interactively)
                .onChange(of: shown.count) { _, _ in
                    if let preserveMessage { proxy.scrollTo(preserveMessage, anchor: .top); self.preserveMessage = nil }
                    else if nearBottom { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                }
                .onChange(of: pending.count) { old, count in
                    if count > old {
                        proxy.scrollTo(Self.bottom, anchor: .bottom)
                        if let queuedAt { MessageTiming.record(.queueVisible, since: queuedAt); self.queuedAt = nil }
                    }
                }
                .onChange(of: last?.sequence) { _, sequence in if nearBottom, let sequence { lastSeen = sequence } }
                .onChange(of: nearBottom) { _, value in if value, let sequence = last?.sequence { lastSeen = sequence } }
                // Opening with unread messages starts at the first of them, once.
                .onChange(of: firstUnread) { _, sequence in
                    guard let sequence, let id = shown.first(where: { $0.sequence == sequence })?.id else { return }
                    lastSeen = sequence - 1; nearBottom = false
                    proxy.scrollTo(id, anchor: .top)
                }
                .overlay(alignment: .bottomTrailing) {
                    if !nearBottom && searchQuery.isEmpty {
                        let unseen = shown.filter { $0.sequence > lastSeen && $0.account != own }.count
                        Button { withAnimation { proxy.scrollTo(Self.bottom, anchor: .bottom) } } label: {
                            HStack(spacing: 4) {
                                Image(systemName: "arrow.down").accessibilityHidden(true)
                                if unseen > 0 { Text(unseen.formatted()).font(.subheadline.monospacedDigit()).accessibilityHidden(true) }
                            }.frame(minWidth: 24, minHeight: 24)
                        }.buttonStyle(.borderedProminent).padding(12)
                            .accessibilityLabel("Latest messages").accessibilityValue(unseen > 0 ? unseen.formatted() : "")
                            .accessibilityIdentifier("conversation.latest")
                    }
                }
                .onChange(of: model.messageSending.revision) { _, _ in
                    if let confirmation = model.messageSending.lastConfirmation, confirmation.channel == channel.id {
                        MessageTiming.record(.ackVisible, since: confirmation.at)
                    }
                }
        }.privateScreen().navigationTitle(channel.name).navigationBarTitleDisplayMode(.inline)
            .toolbar(sizeClass == .compact ? .hidden : .automatic, for: .tabBar)
            .safeAreaInset(edge: .top, spacing: 0) { if searching { searchBar } }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                ConversationComposer(channel: currentChannel, restriction: channelRestriction(currentChannel.status), receiving: receiving != nil, replying: $replying,
                                     willQueue: { queuedAt = .now }, failed: { await load() })
            }
            .toolbar {
                ToolbarItem(placement: .principal) {
                    Button { showingSettings = true } label: { Text(currentChannel.name).font(.headline).lineLimit(1) }
                        .buttonStyle(.plain).accessibilityHint("Conversation settings").accessibilityIdentifier("conversation.title")
                }
                ToolbarItem(placement: .primaryAction) {
                    Menu("Actions", systemImage: "ellipsis.circle") {
                        Button("Search messages", systemImage: "magnifyingglass") { openSearch() }
                        if currentChannel.canInvite { Button("Invite member", systemImage: "person.badge.plus") { inviting = true } }
                        Button("Conversation settings", systemImage: "gearshape") { showingSettings = true }
                    }.accessibilityIdentifier("conversation.more")
                }
            }
            // A shortcut inside a closed menu does not fire, so it has its own empty button.
            .background { Button(action: openSearch) { EmptyView() }.keyboardShortcut("f", modifiers: .command).accessibilityHidden(true) }
            .navigationDestination(isPresented: $showingSettings) { ChannelSettingsView(channel: channel.id) }
            .sheet(item: $editingMessage) { target in NavigationStack { MessageEditView(channel: channel.id, message: target.message) { await load() } } }
            .sheet(item: $selectingText) { target in NavigationStack { MessageTextView(text: target.message.text) } }
            .confirmationDialog("Withdraw this message?", isPresented: Binding(get: { retracting != nil }, set: { if !$0 { retracting = nil } })) {
                Button("Withdraw", role: .destructive) { if let message = retracting { actionTask = Task { await withdraw(message) } } }
            }
            .sheet(item: $openingFile) { file in NavigationStack { FileDetailDestination(id: file.id, version: file.current).toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { openingFile = nil } } } } }
            .sheet(isPresented: $inviting) { NavigationStack { ChannelInvitationView(channel: channel.id) } }
            .sheet(item: $secretPackage) { source in NavigationStack { SecretPackageImportView(source: source) } }
            .task(id: model.profileRevision) { requestPeople() }
            .onChange(of: model.filesRevision) { _, _ in receivedFiles = [:]; checkedAttachments = [] }
            .task(id: "\(channel.id):\(model.contentRevision):\(model.filesRevision):\(model.epoch):\(query)") {
                model.activeChannel = channel.id
                if !query.isEmpty { try? await Task.sleep(for: .milliseconds(200)) }
                if !Task.isCancelled { await load() }
            }
            .task(id: "refresh:\(channel.id):\(model.epoch):\(model.concealed)") {
                guard !model.concealed else { return }
                while !Task.isCancelled {
                    await refreshMessages()
                    do { try await Task.sleep(for: .seconds(model.pollSeconds(30))) } catch { break }
                }
            }
            .onChange(of: model.channelHints[channel.id]) { _, _ in Task { await refreshMessages() } }
            .task(id: "attachments:\(channel.id):\(model.epoch):\(model.concealed)") {
                guard !model.concealed, let engine = model.engine else { return }
                while !Task.isCancelled {
                    // Optional attachment maintenance has its own lifecycle;
                    // it cannot keep text loading or mark a send unconfirmed.
                    do {
                        try await coreOperation { try await engine.refreshChannelAttachments(channel: channel.id, operation: $0) }
                        if !Task.isCancelled { await load() }
                    } catch { /* Keep saved metadata; retry on the next visible pass. */ }
                    do { try await Task.sleep(for: .seconds(model.pollSeconds(30))) } catch { break }
                }
            }
            .onDisappear {
                refreshRequested = false
                actionTask?.cancel()
                if model.activeChannel == channel.id { model.activeChannel = nil }
            }
    }

    private var searchBar: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
            TextField("Search messages", text: Binding(get: { query }, set: { query = String($0.prefix(200)) }))
                .focused($searchFocused).submitLabel(.search).autocorrectionDisabled().accessibilityIdentifier("conversation.search")
            Button("Cancel") { query = ""; searching = false }.frame(minHeight: 44)
        }.padding(.horizontal, 16).background(.bar)
    }

    private func openSearch() {
        searching = true; searchFocused = true
    }

    private func channelRestriction(_ status: String) -> String {
        switch status {
        case "archived": String(localized: "This conversation is archived.")
        case "rotation_required": String(localized: "Publishing is paused while conversation keys are updated.")
        case "active": String(localized: "You have read-only access to this conversation.")
        default: String(localized: "Sync this conversation before sending.")
        }
    }

    private func reveal(_ sequence: UInt64, proxy: ScrollViewProxy) async {
        query = ""; searching = false; await load()
        while !Task.isCancelled && !messages.contains(where: { $0.sequence == sequence }) && hasEarlier {
            let before = nextBefore; await loadEarlier(); if nextBefore == before { break }
        }
        if let message = messages.first(where: { $0.sequence == sequence }) { proxy.scrollTo(message.id, anchor: .center) }
        else if !Task.isCancelled { model.notice = String(localized: "The original message is not available on this device.") }
    }

    private func bubble(_ row: ConversationRow, own: Bool, references: [UInt64: MessageItem], reveal: @escaping (UInt64) -> Void) -> some View {
        let message = row.message
        return ConversationMessageRow(own: own, name: model.personName(message.account), avatar: model.people.avatar(message.account), showsPortrait: row.firstOfGroup) {
            VStack(alignment: .leading, spacing: 6) {
                if !own && row.firstOfGroup { Text(model.personName(message.account)).font(.caption.weight(.semibold)).foregroundStyle(Theme.forest).lineLimit(1) }
                if message.hidden {
                    Text("Hidden message").foregroundStyle(.secondary)
                    Button("Show") { actionTask = Task { await hide(message, hidden: false) } }
                } else {
                    if let reference = message.replySequence { replyPreview(reference, original: references[reference], reveal: reveal) }
                    if message.retracted { Text("Message withdrawn").italic().foregroundStyle(.secondary) }
                    else if message.unreadable { Text("This message could not be verified.").foregroundStyle(.secondary) }
                    else if !message.text.isEmpty { Text(message.text) }
                    if let name = message.attachmentName { attachment(message, name: name) }
                    if message.edited || row.lastOfGroup {
                        HStack(spacing: 4) {
                            if message.edited { Text("Edited") }
                            if row.lastOfGroup {
                                Text(ConversationTimeline.date(message.createdAtMs), style: .time)
                                if own { Image(systemName: "checkmark").accessibilityLabel("Sent") }
                            }
                        }.font(.caption2).foregroundStyle(.secondary)
                    }
                }
            }.accessibilityIdentifier("message.content.\(message.sequence)")
        } actions: {
            messageActions(message, own: own)
        }.accessibilityIdentifier("message.row.\(message.sequence)")
    }

    private func replyPreview(_ reference: UInt64, original: MessageItem?, reveal: @escaping (UInt64) -> Void) -> some View {
        Button { reveal(reference) } label: {
            VStack(alignment: .leading, spacing: 3) {
                if let original {
                    Text(model.personName(original.account)).font(.caption.bold())
                    Text(original.hidden ? String(localized: "Hidden message") : original.retracted ? String(localized: "Message withdrawn") : original.unreadable ? String(localized: "This message could not be verified.") : original.text.isEmpty ? original.attachmentName ?? "" : original.text).font(.caption).lineLimit(2)
                } else { Text("Reply to message \(reference)").font(.caption) }
            }.padding(8).frame(maxWidth: .infinity, alignment: .leading).background(Theme.forest.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
        }.buttonStyle(.plain).accessibilityLabel("Open original message")
    }

    @ViewBuilder private func attachment(_ message: MessageItem, name: String) -> some View {
        let secret = message.attachmentMime == "application/vnd.dmsg.secret-package"
        let saved = receivedFiles[message.sequence]
        Button {
            if secret { secretPackage = .init(channel: channel.id, sequence: message.sequence) }
            else if let saved { openingFile = saved }
            else { actionTask = Task { await receiveAttachment(message) } }
        } label: {
            VStack(alignment: .leading, spacing: 4) {
                if let saved, ThumbnailStore.supports(saved) {
                    LibraryThumbnail(file: saved).frame(width: 220, height: 150).clipShape(RoundedRectangle(cornerRadius: 8))
                }
                Label(name, systemImage: secret ? "lock.doc" : fileSymbol(message.attachmentMime ?? ""))
                if let size = message.attachmentSize { Text(fileSize(size)).font(.caption).foregroundStyle(.secondary) }
                if saved != nil { Text("On this device").font(.caption) }
                if receiving == message.sequence { ProgressView() }
                if secret { Text("Preview and choose what to import").font(.caption) }
            }.font(.subheadline).padding(10)
        }.disabled(message.unreadable || message.attachmentDeleted || receiving != nil)
            .background(Theme.forest.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
    }

    @ViewBuilder private func messageActions(_ message: MessageItem, own: Bool) -> some View {
        if !message.hidden && !message.unreadable && !message.retracted {
            if currentChannel.canPublish { Button("Reply") { replying = message } }
            if !message.text.isEmpty {
                Button("Copy") { model.clipboard.copy(message.text); model.notice = String(localized: "Copied") }
                Button("Select text") { selectingText = MessageEdit(message: message) }
            }
            if own && currentChannel.canPublish {
                if !message.text.isEmpty { Button("Edit message") { editingMessage = MessageEdit(message: message) } }
                Button("Withdraw message", role: .destructive) { retracting = message }
            }
        }
        Button(LocalizedStringKey(message.hidden ? "Show" : "Hide on this device")) { actionTask = Task { await hide(message, hidden: !message.hidden) } }
    }

    /// The text comes first and stays put, whatever state its delivery is in.
    private func pendingBubble(_ item: OutgoingMessage) -> some View {
        let active = model.messageSending.active.contains(item.id), queued = model.messageSending.contains(item.id)
        let unconfirmed = item.error == "EXECUTION_UNKNOWN" || item.attempted && item.error == nil
        return ConversationMessageRow(own: true, name: "", avatar: nil, showsPortrait: false) {
            VStack(alignment: .leading, spacing: 6) {
                if !item.text.isEmpty { Text(item.text) }
                if let name = item.attachmentName { Label(name, systemImage: "doc").font(.subheadline) }
                HStack(spacing: 6) {
                    if active { ProgressView().controlSize(.small); Text("Sending") }
                    else if queued { Text("Waiting to submit") }
                    else if unconfirmed { Image(systemName: "clock").accessibilityHidden(true); Text("Waiting for confirmation") }
                    else if let error = item.error { Image(systemName: "exclamationmark.circle").foregroundStyle(.red).accessibilityHidden(true); Text(MessageFailure.text(error)) }
                    else { Text("Waiting to submit") }
                }.font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                if !queued {
                    Button(LocalizedStringKey(item.attempted ? "Retry" : "Send message")) {
                        model.deliverMessage(channel: channel.id, item: item)
                    }.font(.subheadline.weight(.medium)).accessibilityIdentifier("message.retry.\(item.id)")
                }
            }
        } actions: { EmptyView() }
            .accessibilityElement(children: .contain).accessibilityIdentifier("message.pending.\(item.id)")
    }

    private func requestPeople() {
        model.people.request(Set(messages.map(\.account)).union([model.session?.account ?? ""]), model: model)
    }

    private func hide(_ message: MessageItem, hidden: Bool) async {
        let epoch = model.epoch
        do {
            try await model.engine?.hideMessage(channel: channel.id, sequence: message.sequence, hidden: hidden)
            if epoch == model.epoch, !Task.isCancelled { await load() }
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func refreshMessages() async {
        guard !model.concealed, !Task.isCancelled else { return }
        refreshRequested = true
        guard !refreshing else { return }
        refreshing = true
        defer { refreshing = false }
        while refreshRequested, !model.concealed, !Task.isCancelled {
            refreshRequested = false
            await refreshMessagePass()
        }
    }

    private func refreshMessagePass() async {
        let epoch = model.epoch
        refreshFailed = false
        // The core commits verified pages as it receives them. Read those
        // pages while the remaining history or attachment metadata is pending.
        // A slow later request must not leave the whole conversation blank.
        let local = Task {
            while !Task.isCancelled {
                await load()
                do { try await Task.sleep(for: .seconds(1)) } catch { break }
            }
        }
        defer { local.cancel() }
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
                if case MobileError.Failure(let code) = error { MessageTiming.failure(.refresh, code: code) }
                model.reportBackground(error)
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
        return searchQuery.isEmpty ? try await engine.messages(channel: channel.id, before: before, limit: 100)
            : try await engine.searchMessagesPage(channel: channel.id, query: searchQuery, before: before)
    }

    private func load() async {
        guard !loadingEarlier || query != loadedQuery else { return }
        let started = ContinuousClock.now
        let epoch = model.epoch, query = query, request = UUID(), sendRevision = model.messageSending.revision
        messageRequest = request
        if query != loadedQuery { oldestLoaded = nil }
        let earlier = query == loadedQuery
            ? oldestLoaded ?? (searchQuery.isEmpty && !nearBottom ? messages.first?.sequence : nil) : nil
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
            // This runs every second during a sync; an unchanged page redraws nothing.
            if messages != fresh { messages = fresh; requestPeople() }
            if hasEarlier != page.hasMore { hasEarlier = page.hasMore }
            if nextBefore != page.nextBefore { nextBefore = page.nextBefore }
            if firstUnread == nil && channel.unread > 0 {
                firstUnread = fresh.first { $0.sequence > channel.readThrough && $0.account != model.session?.account }?.sequence
            }
            if loadedQuery != query { loadedQuery = query }
            if !messagesKnown { messagesKnown = true }
            if loadFailed { loadFailed = false }
            MessageTiming.record(.localPage, since: started)
            guard let engine = model.engine else {
                #if DEBUG
                if model.uiTesting, ProcessInfo.processInfo.arguments.contains("--ui-channel-saved-attachment") {
                    receivedFiles[1] = model.fixtureFiles.first { $0.id == "fixture-local" && $0.canOpen }
                }
                #endif
                model.messageSending.loaded(channel: channel.id, items: fresh, since: sendRevision)
                return
            }
            let pending = try await engine.outbox(channel: channel.id)
            guard epoch == model.epoch, self.query == query, messageRequest == request, !Task.isCancelled else { return }
            if outbox != pending.items { outbox = pending.items }
            model.messageSending.loaded(channel: channel.id, items: fresh, since: sendRevision)
            // Each attachment is looked up once until the library changes.
            for message in fresh where message.attachmentName != nil && !message.attachmentDeleted && !message.unreadable && !checkedAttachments.contains(message.sequence) {
                guard let value = try? await engine.savedAttachment(channel: channel.id, sequence: message.sequence), epoch == model.epoch, !Task.isCancelled else { continue }
                checkedAttachments.insert(message.sequence)
                if let file = try? model.decode(value) as LibraryFile? { receivedFiles[message.sequence] = file }
            }
        } catch {
            if epoch == model.epoch, self.query == query, messageRequest == request, !Task.isCancelled {
                loadFailed = true; model.reportBackground(error)
            }
        }
    }

    private func loadEarlier() async {
        guard hasEarlier, let before = nextBefore, before > 1, !loadingEarlier else { return }
        let epoch = model.epoch, request = UUID(), first = messages.first, query = query
        messageRequest = request
        loadingEarlier = true; defer { loadingEarlier = false }
        do {
            let page: MessagePage
            #if DEBUG
            if model.uiTesting { page = model.conversationFixture(before: before, query: searchQuery) }
            else { page = try await readEarlier(before: before) }
            #else
            page = try await readEarlier(before: before)
            #endif
            guard epoch == model.epoch, query == self.query, messageRequest == request, !Task.isCancelled else { return }
            preserveMessage = first?.id
            let rows = Dictionary((page.items + messages).map { ($0.sequence, $0) }, uniquingKeysWith: { first, _ in first })
            messages = rows.values.sorted { $0.sequence < $1.sequence }
            oldestLoaded = messages.first?.sequence; hasEarlier = page.hasMore; nextBefore = page.nextBefore
            requestPeople()
        } catch { if epoch == model.epoch, messageRequest == request, !Task.isCancelled { model.report(error) } }
    }

    private func readEarlier(before: UInt64) async throws -> MessagePage {
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        if !searchQuery.isEmpty { return try await engine.searchMessagesPage(channel: channel.id, query: searchQuery, before: before) }
        return try await coreOperation { try await engine.earlierMessages(channel: channel.id, before: before, limit: 100, operation: $0) }
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
        guard receiving == nil else { return }
        let epoch = model.epoch; receiving = message.sequence; defer { receiving = nil }
        do {
            let result = try await model.transfer.run(message.attachmentName ?? String(localized: "Files")) { try await engine.saveAttachment(channel: channel.id, sequence: message.sequence, operation: $0) }
            guard epoch == model.epoch, !Task.isCancelled else { return }
            struct Imported: Decodable { let file: LibraryFile }
            let saved: Imported = try model.decode(result)
            receivedFiles[message.sequence] = saved.file; openingFile = saved.file
            model.notice = String(localized: "Verified attachment saved to Files.")
        } catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
    }
}

/// Long-pressing a message opens its menu, so selecting text happens on its own page.
private struct MessageTextView: View {
    let text: String
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ScrollView {
            Text(text).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading).padding(16)
        }.privateScreen().navigationTitle("Select text").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
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
    @State private var leaving = false
    @State private var scope: UInt64 = 0
    private var key: String { "message-edit:" + channel + ":" + message.id }
    var body: some View {
        Form { SettingsField("Message") { TextEditor(text: $text).frame(minHeight: 180) } }.disabled(working)
            .privateScreen().navigationTitle("Edit message").onAppear { scope = model.viewGeneration; text = model.formDraft(key, as: String.self) ?? message.text }
            .interactiveDismissDisabled(working || text != message.text)
            .onDisappear { if text != message.text { model.keepFormDraft(key, value: text, generation: scope) } }
            .confirmationDialog("Leave this form?", isPresented: $leaving) { Button("Keep draft and leave") { dismiss() } }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { if text == message.text { dismiss() } else { leaving = true } }.disabled(working) }
                ToolbarItem(placement: .confirmationAction) { Button("Save") { Task { await save() } }.disabled(working || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || text.utf8.count > 16 * 1024) }
            }
    }
    private func save() async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            let id = try await engine.queueMessageAction(channel: channel, action: "edit", sequence: message.sequence, digest: message.digest, text: text)
            guard epoch == model.epoch else { return }
            model.deliverMessage(channel: channel, item: OutgoingMessage(id: id, text: text, attachmentName: nil, attempted: false, error: nil))
            await changed(); model.settingsDrafts.removeValue(forKey: key); text = message.text; dismiss()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
