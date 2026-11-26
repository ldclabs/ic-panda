import SwiftUI
@preconcurrency import DMsgCore

struct ConversationView: View {
    let channel: ChannelItem
    @State private var historyAnchor: UInt64?

    init(channel: ChannelItem, at sequence: UInt64? = nil) {
        self.channel = channel
        _historyAnchor = State(initialValue: sequence)
    }
    @Environment(SessionModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var messages: [MessageItem] = []
    @State private var queuedAt: ContinuousClock.Instant?
    @State private var outbox: [OutgoingMessage] = []
    @State private var receivedFiles: [UInt64: LibraryFile] = [:]
    @State private var checkedAttachments: Set<UInt64> = []
    @State private var openingFile: LibraryFile?
    @State private var previewLaunch: FullscreenPreviewLaunch?
    @State private var receiving: UInt64?
    @State private var firstUnread: UInt64?
    @State private var lastSeen: UInt64 = 0
    @State private var inviting = false
    @State private var showingSettings = false
    @State private var secretPackage: SecretPackageImportView.ChannelSource?
    @State private var hasEarlier = false
    @State private var nextBefore: UInt64?
    @State private var hasAfter = false
    @State private var nextAfter: UInt64?
    @State private var loadingNewer = false
    @State private var highlightedSequence: UInt64?
    @State private var highlightSpans: [HighlightSpan] = []
    @State private var scrolledAnchor: UInt64?
    @State private var oldestLoaded: UInt64?
    @State private var preserveMessage: String?
    @State private var nearBottom = true
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
    @State private var hasFileOnly = false
    @State private var searchHits: [RichMessageSearchHit] = []
    @State private var searchNextBefore: UInt64?
    @State private var searchLoading = false
    @State private var searchIndex = -1
    @State private var searchExpanded = true
    @FocusState private var searchFocused: Bool
    @State private var editingMessage: MessageEdit?
    @State private var selectingText: MessageEdit?
    @State private var retracting: MessageItem?
    /// The message a reply jump landed on, lit for a moment.
    @State private var pointed: String?
    @State private var replyJump: Task<Void, Never>?
    private var currentChannel: ChannelItem { model.channels.first { $0.id == channel.id } ?? channel }
    private var searchQuery: String { query.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var activeSearchKey: String {
        guard searching, !searchQuery.isEmpty || hasFileOnly else { return "" }
        return "\(channel.id):\(model.epoch):\(model.contentRevision):\(hasFileOnly):\(searchQuery)"
    }
    private static let bottom = "conversation.bottom"
    struct MessageEdit: Identifiable { let message: MessageItem; var id: String { message.id } }

    var body: some View {
        let own = model.session?.account
        let merged = model.messageSending.messages(channel: channel.id, stored: messages)
        let shown = merged.filter { !model.blockedAccounts.contains($0.account) }
        // A verified local copy can arrive before the original send's ACK.
        let received = Set(merged.filter { $0.account == own }.map(\.id))
        let pending = hasAfter ? [] : model.messageSending.items(channel: channel.id, stored: outbox, received: received)
        let rows = ConversationTimeline.rows(shown)
        let references = Dictionary(merged.map { ($0.sequence, $0) }, uniquingKeysWith: { _, next in next })
        let last = shown.last
        let readWatermark = merged.last?.sequence
        let readScope = "\(channel.id):\(model.epoch):\(scenePhase == .active):\(model.unlocked):\(model.concealed):\(messagesKnown):\(!hasAfter):\(nearBottom):\(readWatermark ?? 0)"
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    // Saved messages stay on screen without comment while newer ones arrive.
                    if shown.isEmpty && (refreshing || !messagesKnown && !loadFailed) {
                        ProgressView("Syncing…").frame(maxWidth: .infinity).accessibilityIdentifier("conversation.loading")
                    }
                    if loadFailed || refreshFailed {
                        NoticeBar(text: String(localized: "Showing saved content. The latest sync did not finish."))
                        Button("Retry") { actionTask = Task { await refreshMessages() } }
                            .disabled(refreshing).accessibilityIdentifier("conversation.retry")
                    } else if messagesKnown && shown.isEmpty && pending.isEmpty && !hasEarlier && !refreshing {
                        ContentUnavailableView {
                            Label { Text("No messages") } icon: { AppGlyph.channels.image }
                        }.accessibilityIdentifier("conversation.empty")
                    }
                    if hasEarlier {
                        LoadMoreRow(title: "Load earlier messages", working: loadingEarlier, trigger: shown.count) { await loadEarlier() }
                            .accessibilityIdentifier("conversation.earlier")
                    }
                    ForEach(rows) { row in
                        VStack(alignment: .leading, spacing: 8) {
                            if row.startsDay {
                                Text(ConversationTimeline.dayTitle(ConversationTimeline.date(row.message.createdAtMs)))
                                    .font(.footnote).foregroundStyle(.secondary).frame(maxWidth: .infinity).padding(.top, 8)
                            }
                            if row.message.sequence == firstUnread && !hasAfter {
                                Label("Unread messages", systemImage: "circle.fill").font(.footnote).foregroundStyle(Theme.forest).frame(maxWidth: .infinity)
                            }
                            bubble(row, own: row.message.account == own, references: references, shown: shown) { sequence in
                                jumpToReply(sequence, proxy: proxy)
                            }
                        }.padding(.top, row.firstOfGroup ? 10 : 0)
                            .background {
                                if pointed == row.id { RoundedRectangle(cornerRadius: Theme.radius).fill(Theme.fill).padding(-4).accessibilityHidden(true) }
                            }
                            .id(row.id)
                            // A message that arrives while the conversation is open rises in; see `load()`.
                            .transition(reduceMotion ? .opacity : .opacity.combined(with: .offset(y: 8)))
                    }
                    if hasAfter {
                        HStack(spacing: 12) {
                            Button("Load newer messages") { actionTask = Task { await loadNewer() } }
                                .buttonStyle(.bordered).disabled(loadingNewer)
                                .accessibilityIdentifier("conversation.newer")
                            Button("Latest messages") { followLatest() }
                                .buttonStyle(.borderless)
                        }.frame(maxWidth: .infinity).padding(.vertical, 8)
                    }
                    // Pending and confirmed rows need distinct IDs so
                    // LazyVStack replaces the sending UI after an ACK.
                    ForEach(pending, id: \.id) { item in
                        // Once confirmed, the stored message takes its place as the queued one fades.
                        pendingBubble(item).id("pending:\(item.id)").padding(.top, 8)
                            .transition(.asymmetric(insertion: reduceMotion ? .opacity : .move(edge: .bottom).combined(with: .opacity), removal: .opacity))
                    }
                    // The end of the conversation being on screen is what counts as read and as following along.
                    Color.clear.frame(height: 1).id(Self.bottom)
                        .reminderVisible(.channel(channel.id), watermark: !hasAfter ? readWatermark : nil,
                                         ready: messagesKnown && !hasAfter && nearBottom && readWatermark != nil)
                        .onScrollVisibilityChange(threshold: 0.01) { nearBottom = $0 }
                }.padding(16).motion(Motion.move, reduced: Motion.fade, value: pending.map(\.id))
            }.defaultScrollAnchor(.bottom)
                .defaultScrollAnchor(shown.isEmpty && pending.isEmpty ? .center : .bottom, for: .alignment)
                .scrollDismissesKeyboard(.interactively)
                .task(id: readScope) {
                    guard scenePhase == .active, model.unlocked, !model.concealed,
                          messagesKnown, !hasAfter, nearBottom, let seq = readWatermark else { return }
                    // Opening at the first unread message scrolls the end away
                    // after this starts; that change cancels the read.
                    do { try await Task.sleep(for: .milliseconds(300)) } catch { return }
                    if seq > currentChannel.readThrough {
                        await model.channelRead(channel.id, through: seq)
                    }
                }
                .onDisappear { Task { await model.flushChannelCursor(channel.id) } }
                .onChange(of: shown.count) { previousCount, _ in
                    if let preserveMessage { proxy.scrollTo(preserveMessage, anchor: .top); self.preserveMessage = nil }
                    else if let targetSeq = highlightedSequence, scrolledAnchor != targetSeq,
                            let target = shown.first(where: { $0.sequence == targetSeq }) {
                        scrolledAnchor = targetSeq
                        proxy.scrollTo(target.id, anchor: .center)
                    }
                    // The default anchor positions the first page while the composer lays out.
                    else if previousCount > 0 && nearBottom && !hasAfter { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                }
                .onChange(of: highlightedSequence) { _, targetSeq in
                    guard let targetSeq, scrolledAnchor != targetSeq,
                          let target = shown.first(where: { $0.sequence == targetSeq }) else { return }
                    scrolledAnchor = targetSeq
                    withAnimation(reduceMotion ? nil : .default) { proxy.scrollTo(target.id, anchor: .center) }
                }
                .onChange(of: pending.count) { old, count in
                    if count > old {
                        proxy.scrollTo(Self.bottom, anchor: .bottom)
                        if let queuedAt { MessageTiming.record(.queueVisible, since: queuedAt); self.queuedAt = nil }
                    }
                }
                .onChange(of: last?.sequence) { _, sequence in if nearBottom && !hasAfter, let sequence { lastSeen = sequence } }
                .onChange(of: nearBottom) { _, value in if value && !hasAfter, let sequence = last?.sequence { lastSeen = sequence } }
                // Opening with unread messages starts at the first of them, once.
                .onChange(of: firstUnread) { _, sequence in
                    guard let sequence, !hasAfter, let id = shown.first(where: { $0.sequence == sequence })?.id else { return }
                    lastSeen = sequence - 1
                    proxy.scrollTo(id, anchor: .top)
                }
                .overlay(alignment: .bottomTrailing) {
                    let following = hasAfter || !nearBottom
                    ZStack {
                        if following {
                            let unseen = hasAfter ? 0 : shown.filter { $0.sequence > lastSeen && $0.account != own }.count
                            Button {
                                followLatest()
                                withAnimation { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                            } label: {
                                HStack(spacing: 4) {
                                    Image(systemName: "arrow.down").accessibilityHidden(true)
                                    if unseen > 0 { Text(unseen.formatted()).font(.subheadline.monospacedDigit()).countMotion(unseen).accessibilityHidden(true) }
                                }.frame(minWidth: 24, minHeight: 24)
                            }.buttonStyle(.borderedProminent).padding(12)
                                .accessibilityLabel("Latest messages").accessibilityValue(unseen > 0 ? unseen.formatted() : "")
                                .accessibilityIdentifier("conversation.latest")
                                .popping(scale: 0.8)
                        }
                    }.motion(Motion.settle, reduced: Motion.fade, value: following)
                }
                .onChange(of: model.messageSending.revision) { _, _ in
                    if let confirmation = model.messageSending.lastConfirmation, confirmation.channel == channel.id {
                        MessageTiming.record(.ackVisible, since: confirmation.at)
                    }
                }
        }.privateScreen(.plain).navigationTitle(channel.name).navigationBarTitleDisplayMode(.inline)
            .toolbar(sizeClass == .compact ? .hidden : .automatic, for: .tabBar)
            .safeAreaInset(edge: .top, spacing: 0) { if searching { searchBar } }
            .edgeBar(.bottom, fallback: .bar) {
                ConversationComposer(channel: currentChannel, restriction: channelRestriction(currentChannel.status), receiving: receiving != nil,
                                     willQueue: { query = ""; searching = false; followLatest(); queuedAt = .now }, failed: { await load() })
            }
            // The reply and attachments rise above the composer, and the conversation makes room.
            .motion(Motion.move, reduced: Motion.fade,
                    value: "\(model.replyDrafts[channel.id]?.id ?? ""):\(model.attachmentDrafts[channel.id]?.count ?? 0)")
            .toolbar {
                ToolbarItem(placement: .principal) {
                    Button { showingSettings = true } label: {
                        HStack(spacing: 8) {
                            Avatar(currentChannel.name, seed: currentChannel.id, kind: .conversation, size: 28)
                            Text(currentChannel.name).font(.headline).lineLimit(1)
                        }
                    }
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
            .fullScreenCover(item: $previewLaunch) { launch in FullscreenFilePreviewView(launch: launch) }
            .sheet(isPresented: $inviting) { NavigationStack { ChannelInvitationView(channel: channel.id) } }
            .sheet(item: $secretPackage) { source in NavigationStack { SecretPackageImportView(source: source) } }
            .onChange(of: model.filesRevision) { _, _ in receivedFiles = [:]; checkedAttachments = [] }
            .task(id: "\(channel.id):\(model.contentRevision):\(model.filesRevision):\(model.epoch):\(historyAnchor ?? 0)") {
                model.activeChannel = channel.id
                if !Task.isCancelled { await load() }
            }
            .task(id: activeSearchKey) {
                await runInChannelSearch(append: false)
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
                cancelReplyJump()
                refreshRequested = false
                actionTask?.cancel()
                if model.activeChannel == channel.id { model.activeChannel = nil }
            }
    }

    private var searchBar: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
                TextField("Search messages", text: Binding(get: { query }, set: { query = String($0.prefix(200)); searchExpanded = true }))
                    .focused($searchFocused).submitLabel(.search).autocorrectionDisabled()
                    .onSubmit {
                        if !searchHits.isEmpty { stepSearch(1) }
                    }
                    .accessibilityIdentifier("conversation.search")
                Button {
                    hasFileOnly.toggle()
                    searchExpanded = true
                } label: {
                    Image(systemName: hasFileOnly ? "paperclip.circle.fill" : "paperclip")
                        .foregroundStyle(hasFileOnly ? Theme.forest : Color.secondary)
                        .frame(minWidth: 36, minHeight: 44)
                }.buttonStyle(.plain).accessibilityLabel("Attachments")
                if !searchHits.isEmpty {
                    let displayIndex = searchIndex >= 0 ? "\(searchIndex + 1)/\(searchHits.count)" : "\(searchHits.count)"
                    Text(verbatim: displayIndex).font(.footnote.monospacedDigit()).foregroundStyle(.secondary)
                    Button { stepSearch(-1) } label: {
                        Image(systemName: "chevron.up").frame(minWidth: 32, minHeight: 44)
                    }.buttonStyle(.plain).accessibilityLabel("Previous match")
                    Button { stepSearch(1) } label: {
                        Image(systemName: "chevron.down").frame(minWidth: 32, minHeight: 44)
                    }.buttonStyle(.plain).accessibilityLabel("Next match")
                }
                Button("Cancel") { closeSearch() }.frame(minHeight: 44)
            }.padding(.horizontal, 16)
            if !searchQuery.isEmpty || hasFileOnly {
                if searchLoading && searchHits.isEmpty {
                    ProgressView().frame(maxWidth: .infinity).padding(.vertical, 8)
                } else if searchHits.isEmpty {
                    Text("No matching results").font(.footnote).foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.bottom, 8)
                } else if searchExpanded {
                    if searchHits.count <= 3 && searchNextBefore == nil {
                        VStack(alignment: .leading, spacing: 0) { searchHitRows }
                    } else {
                        ScrollView {
                            LazyVStack(alignment: .leading, spacing: 0) { searchHitRows }
                        }.frame(height: 200)
                    }
                }
            }
            Divider()
        }.background(.bar)
    }

    @ViewBuilder private var searchHitRows: some View {
        ForEach(Array(searchHits.enumerated()), id: \.element.id) { idx, hit in
            Button {
                jumpToHit(index: idx)
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    Text(highlightedText(hit.preview.text, spans: hit.preview.spans))
                        .font(.subheadline).foregroundStyle(.primary).lineLimit(2)
                    HStack(spacing: 6) {
                        if let sender = hit.senderName, !sender.isEmpty { Text(sender).lineLimit(1) }
                        Spacer(minLength: 4)
                        if hit.message.timestampMs > 0 {
                            Text(ConversationTimeline.date(hit.message.timestampMs), style: .date)
                        }
                    }.font(.footnote).foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 16).padding(.vertical, 8)
                .background(idx == searchIndex ? Theme.fill : Color.clear)
            }.buttonStyle(.plain)
            Divider().padding(.leading, 16)
        }
        if searchNextBefore != nil {
            Button("Load more") { actionTask = Task { await runInChannelSearch(append: true) } }
                .font(.footnote).disabled(searchLoading)
                .frame(maxWidth: .infinity).padding(.vertical, 8)
        }
    }

    private func openSearch() {
        cancelReplyJump()
        searching = true; searchExpanded = true; searchFocused = true
    }

    private func closeSearch() {
        query = ""; searching = false; hasFileOnly = false
        searchHits = []; searchNextBefore = nil; searchIndex = -1; searchExpanded = true
        highlightedSequence = nil; highlightSpans = []
    }

    private func stepSearch(_ delta: Int) {
        guard !searchHits.isEmpty else { return }
        let next: Int
        if searchIndex < 0 { next = delta > 0 ? 0 : searchHits.count - 1 }
        else { next = (searchIndex + delta + searchHits.count) % searchHits.count }
        jumpToHit(index: next)
    }

    private func jumpToHit(index: Int) {
        guard searchHits.indices.contains(index) else { return }
        let hit = searchHits[index]
        searchIndex = index
        searchExpanded = false
        searchFocused = false
        actionTask = Task { await jumpTo(sequence: hit.sequence, spans: hit.snippetSpans ?? []) }
    }

    private func runInChannelSearch(append: Bool) async {
        let term = searchQuery
        let filesOnly = hasFileOnly
        guard searching, !term.isEmpty || filesOnly else {
            searchHits = []; searchNextBefore = nil; searchIndex = -1; searchLoading = false
            return
        }
        let cursor = append ? searchNextBefore : nil
        if append && cursor == nil { return }
        let epoch = model.epoch
        searchLoading = true
        defer { if epoch == model.epoch { searchLoading = false } }
        if !append {
            do { try await Task.sleep(for: .milliseconds(150)) } catch { return }
        }
        guard epoch == model.epoch, !Task.isCancelled else { return }
        #if DEBUG
        if model.uiTesting {
            let page = model.conversationFixture(channel: channel.id, before: cursor, query: term)
            let hits = page.items.reversed().compactMap { item -> RichMessageSearchHit? in
                guard !item.hidden && !item.retracted && !item.unreadable && !model.blockedAccounts.contains(item.account) else { return nil }
                if filesOnly && (item.attachmentDeleted || item.attachmentName == nil) { return nil }
                if !term.isEmpty && !item.text.localizedCaseInsensitiveContains(term)
                    && !(!item.attachmentDeleted && item.attachmentName?.localizedCaseInsensitiveContains(term) == true) { return nil }
                let file = (!item.attachmentDeleted ? item.attachmentName : nil).map { MessageHitFile(name: $0) }
                return RichMessageSearchHit(channel: channel.id,
                                            message: MessageHitBody(seq: item.sequence, messageId: item.id, account: item.account,
                                                                    text: item.text, createdAt: item.createdAtMs, storedAt: item.createdAtMs,
                                                                    file: file, fileDeleted: item.attachmentDeleted))
            }
            searchHits = append ? searchHits + hits : hits
            searchNextBefore = page.hasMore ? page.nextBefore : nil
            if !append { searchIndex = -1 }
            return
        }
        #endif
        guard let engine = model.engine else { return }
        do {
            let page: RichChannelMessagePage = try await model.decodeInBackground(
                try await engine.searchChannelMessages(channel: channel.id, query: term, before: cursor, limit: 40, hasFile: filesOnly, sender: nil)
            )
            guard epoch == model.epoch, !Task.isCancelled, searchQuery == term, hasFileOnly == filesOnly else { return }
            let filtered = page.hits.filter { !model.blockedAccounts.contains($0.message.account) }
            searchHits = append ? searchHits + filtered : filtered
            searchNextBefore = page.hasMore ? page.nextBefore : nil
            if !append { searchIndex = -1 }
        } catch {
            if epoch == model.epoch, !Task.isCancelled, !append { searchHits = []; searchNextBefore = nil }
        }
    }

    private func jumpTo(sequence: UInt64, spans: [HighlightSpan]) async {
        let epoch = model.epoch, request = UUID()
        messageRequest = request
        scrolledAnchor = nil
        #if DEBUG
        if model.uiTesting {
            highlightedSequence = sequence
            highlightSpans = spans
            historyAnchor = sequence
            return
        }
        #endif
        guard let engine = model.engine else { return }
        do {
            let window = try await engine.messagesAround(channel: channel.id, seq: sequence, before: 30, after: 30)
            guard epoch == model.epoch, messageRequest == request, !Task.isCancelled else { return }
            guard window.items.contains(where: { $0.sequence == sequence }) else { return }
            messages = window.items
            hasEarlier = window.hasBefore
            nextBefore = window.nextBefore
            hasAfter = window.hasAfter
            nextAfter = window.nextAfter
            oldestLoaded = window.items.first?.sequence
            // A window that reaches the newest message continues as the live timeline.
            historyAnchor = window.hasAfter ? window.anchor : nil
            highlightedSequence = window.anchor
            highlightSpans = spans
            messagesKnown = true
            loadFailed = false
        } catch {
            if epoch == model.epoch, !Task.isCancelled { model.report(error) }
        }
    }

    private func followLatest() {
        cancelReplyJump()
        let wasAnchored = historyAnchor != nil || hasAfter
        messageRequest = nil
        highlightedSequence = nil; highlightSpans = []; scrolledAnchor = nil
        if wasAnchored {
            messages = []; messagesKnown = false; loadFailed = false
            oldestLoaded = nil; nextBefore = nil; hasEarlier = false; preserveMessage = nil
            hasAfter = false; nextAfter = nil
            nearBottom = true
            historyAnchor = nil
            actionTask = Task { await load() }
        }
    }

    private func channelRestriction(_ status: String) -> String {
        switch status {
        case "archived": String(localized: "This conversation is archived.")
        case "rotation_required": String(localized: "Publishing is paused while conversation keys are updated.")
        case "active": String(localized: "You have read-only access to this conversation.")
        default: String(localized: "Sync this conversation before sending.")
        }
    }

    private func cancelReplyJump() {
        replyJump?.cancel()
        pointed = nil
    }

    private func jumpToReply(_ sequence: UInt64, proxy: ScrollViewProxy) {
        let previous = replyJump
        cancelReplyJump()
        replyJump = Task {
            // Let a cancelled history read release the pagination slot before the next jump.
            await previous?.value
            guard !Task.isCancelled, !model.concealed else { return }
            await reveal(sequence, proxy: proxy)
        }
    }

    private func reveal(_ sequence: UInt64, proxy: ScrollViewProxy) async {
        let epoch = model.epoch
        if !messages.contains(where: { $0.sequence == sequence }) {
            await jumpTo(sequence: sequence, spans: [])
        }
        while epoch == model.epoch, !Task.isCancelled, !model.concealed,
              hasEarlier, let before = nextBefore, sequence < before,
              !messages.contains(where: { $0.sequence == sequence }) {
            await loadEarlier()
            if nextBefore == before { break }
        }
        guard !Task.isCancelled, epoch == model.epoch, !model.concealed else { return }
        if let message = messages.first(where: { $0.sequence == sequence }) {
            preserveMessage = nil
            scrolledAnchor = sequence
            proxy.scrollTo(message.id, anchor: .center)
            // The original lights where it landed, then fades, so the eye finds it.
            pointed = message.id
            try? await Task.sleep(for: .seconds(0.6))
            if !Task.isCancelled { withAnimation(Motion.point) { pointed = nil } }
        } else { model.notice = String(localized: "The original message is not available on this device.") }
    }

    private func bubble(_ row: ConversationRow, own: Bool, references: [UInt64: MessageItem], shown: [MessageItem], reveal: @escaping (UInt64) -> Void) -> some View {
        let message = row.message
        let isAnchor = highlightedSequence == message.sequence
        return ConversationMessageRow(own: own, account: message.account, name: model.personName(message.account), avatar: model.people.avatar(message.account),
                                      firstOfGroup: row.firstOfGroup, lastOfGroup: row.lastOfGroup) {
            VStack(alignment: .leading, spacing: 6) {
                if !own && row.firstOfGroup { Text(model.personName(message.account)).font(.footnote.weight(.semibold)).foregroundStyle(Theme.forest).lineLimit(1) }
                if message.hidden {
                    Text("Hidden message").foregroundStyle(.secondary)
                    Button("Show") { actionTask = Task { await hide(message, hidden: false) } }
                } else {
                    if let reference = message.replySequence { replyPreview(reference, original: references[reference], own: own, reveal: reveal) }
                    if message.retracted { Text("Message withdrawn").italic().foregroundStyle(.secondary) }
                    else if message.unreadable { Text("This message could not be verified.").foregroundStyle(.secondary) }
                    else if !message.text.isEmpty {
                        if isAnchor && !highlightSpans.isEmpty {
                            Text(bubbleHighlightedText(message.text, spans: highlightSpans, own: own))
                        } else {
                            Text(message.text)
                        }
                    }
                    if let name = message.attachmentName { attachment(message, name: name, own: own, shown: shown) }
                    if message.edited || row.lastOfGroup {
                        HStack(spacing: 4) {
                            if message.edited { Text("Edited") }
                            if row.lastOfGroup {
                                Text(ConversationTimeline.date(message.createdAtMs), style: .time)
                                if own { Image(systemName: "checkmark").accessibilityLabel("Sent") }
                            }
                        }.font(.footnote).foregroundStyle(.secondary)
                    }
                }
            }.accessibilityIdentifier("message.content.\(message.sequence)")
        } actions: {
            messageActions(message, own: own)
        }
            .padding(isAnchor ? 4 : 0)
            .background(isAnchor ? Theme.fill : Color.clear, in: RoundedRectangle(cornerRadius: Theme.radiusLarge, style: .continuous))
            .accessibilityIdentifier("message.row.\(message.sequence)")
            .loadMessagePerson(row.firstOfGroup ? message.account : nil, model: model)
    }

    private func replyPreview(_ reference: UInt64, original: MessageItem?, own: Bool, reveal: @escaping (UInt64) -> Void) -> some View {
        Button { reveal(reference) } label: {
            VStack(alignment: .leading, spacing: 3) {
                if let original {
                    Text(model.personName(original.account)).font(.footnote.bold())
                        .loadMessagePerson(original.account, model: model)
                    Text(original.hidden ? String(localized: "Hidden message") : original.retracted ? String(localized: "Message withdrawn") : original.unreadable ? String(localized: "This message could not be verified.") : original.text.isEmpty ? original.attachmentName ?? "" : original.text).font(.footnote).lineLimit(2)
                } else { Text("Reply to message \(reference)").font(.footnote) }
            }.padding(8).frame(maxWidth: .infinity, alignment: .leading).background(.bubbleInset(own: own), in: RoundedRectangle(cornerRadius: Theme.radiusSmall))
        }.buttonStyle(.plain).accessibilityLabel("Open original message")
    }

    @ViewBuilder private func attachment(_ message: MessageItem, name: String, own: Bool, shown: [MessageItem]) -> some View {
        let secret = message.attachmentMime == "application/vnd.dmsg.secret-package"
        let saved = receivedFiles[message.sequence]
        Button {
            if secret { secretPackage = .init(channel: channel.id, sequence: message.sequence) }
            else { openAttachmentPreview(for: message, in: shown) }
        } label: {
            VStack(alignment: .leading, spacing: 4) {
                if let saved, ThumbnailStore.supports(saved) {
                    Color.clear.frame(minWidth: 0, maxWidth: 220).frame(height: 150)
                        .overlay { LibraryThumbnail(file: saved) }
                        .clipShape(RoundedRectangle(cornerRadius: Theme.radiusSmall))
                }
                Label(name, systemImage: secret ? "lock.doc" : fileSymbol(message.attachmentMime ?? ""))
                if let size = message.attachmentSize { Text(fileSize(size)).font(.footnote).foregroundStyle(.secondary) }
                if saved != nil { Text("On this device").font(.footnote) }
                if receiving == message.sequence { ProgressView() }
                if secret { Text("Preview and choose what to import").font(.footnote) }
            }.font(.subheadline).lineLimit(nil).fixedSize(horizontal: false, vertical: true).padding(10)
        }.disabled(message.unreadable || message.attachmentDeleted || receiving != nil)
            .background(.bubbleInset(own: own), in: RoundedRectangle(cornerRadius: Theme.radius))
    }

    private func openAttachmentPreview(for message: MessageItem, in shown: [MessageItem]) {
        guard let initial = FullscreenPreviewItem.fromMessage(channel: channel.id, message: message) else { return }
        var items = shown.compactMap { FullscreenPreviewItem.fromMessage(channel: channel.id, message: $0) }
        if !items.contains(where: { $0.id == initial.id }) {
            items.append(initial)
        }
        previewLaunch = FullscreenPreviewLaunch(items: items, initialID: initial.id, showFilter: true, channelID: channel.id)
    }

    @ViewBuilder private func messageActions(_ message: MessageItem, own: Bool) -> some View {
        if !message.hidden && !message.unreadable && !message.retracted {
            if currentChannel.canPublish { Button("Reply") { model.replyDrafts[channel.id] = message } }
            if message.attachmentName != nil && !message.attachmentDeleted && message.attachmentMime != "application/vnd.dmsg.secret-package" {
                Button("Save to Files", systemImage: "folder.badge.plus") {
                    actionTask = Task { await receiveAttachment(message) }
                }
                .disabled(receiving != nil)
            }
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
        return ConversationMessageRow(own: true, account: "", name: "", avatar: nil) {
            VStack(alignment: .leading, spacing: 6) {
                if !item.text.isEmpty { Text(item.text) }
                if let name = item.attachmentName { Label(name, systemImage: "doc").font(.subheadline) }
                HStack(spacing: 6) {
                    if active { ProgressView().controlSize(.small); Text("Sending") }
                    else if queued { Text("Waiting to submit") }
                    else if unconfirmed { Image(systemName: "clock").accessibilityHidden(true); Text("Waiting for confirmation") }
                    else if let error = item.error { Image(systemName: "exclamationmark.circle").accessibilityHidden(true); Text(MessageFailure.text(error)) }
                    else { Text("Waiting to submit") }
                }.font(.footnote).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                if !queued {
                    Button(LocalizedStringKey(item.attempted ? "Retry" : "Send message")) {
                        model.deliverMessage(channel: channel.id, item: item)
                    }.font(.subheadline.weight(.medium)).accessibilityIdentifier("message.retry.\(item.id)")
                }
            }
        } actions: { EmptyView() }
            .accessibilityElement(children: .contain).accessibilityIdentifier("message.pending.\(item.id)")
    }

    private func hide(_ message: MessageItem, hidden: Bool) async {
        let epoch = model.epoch
        do {
            try await model.setMessageHidden(channel: channel.id, sequence: message.sequence, hidden: hidden)
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
        if model.uiTesting { return model.conversationFixture(channel: channel.id, before: before, query: "") }
        #endif
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        return try await engine.messages(channel: channel.id, before: before, limit: 100)
    }

    private func load() async {
        // Keep an anchored historical window intact during periodic refreshes until user returns to latest.
        if hasAfter && messagesKnown { return }
        guard !loadingEarlier || !messagesKnown else { return }
        let started = ContinuousClock.now
        let epoch = model.epoch, request = UUID(), sendRevision = model.messageSending.revision
        messageRequest = request
        let earlier = oldestLoaded ?? (!nearBottom ? messages.first?.sequence : nil)
        do {
            var fresh: [MessageItem]
            var moreBefore: Bool
            var cursorBefore: UInt64?
            if let anchor = historyAnchor, !model.uiTesting, let engine = model.engine {
                let window = try await engine.messagesAround(channel: channel.id, seq: anchor, before: 30, after: 30)
                guard epoch == model.epoch, messageRequest == request, !Task.isCancelled else { return }
                fresh = window.items
                moreBefore = window.hasBefore
                cursorBefore = window.nextBefore
                hasAfter = window.hasAfter
                nextAfter = window.nextAfter
                highlightedSequence = window.anchor
                // Refreshes then read the live timeline down to the window instead of resetting to it.
                if !window.hasAfter {
                    oldestLoaded = window.items.first?.sequence
                    historyAnchor = nil
                }
            } else {
                var page = try await readMessages(before: historyAnchor.map { $0 == .max ? $0 : $0 + 1 })
                fresh = page.items
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
                moreBefore = page.hasMore
                cursorBefore = page.nextBefore
                hasAfter = historyAnchor != nil && model.uiTesting
                nextAfter = nil
            }
            guard epoch == model.epoch, messageRequest == request, !Task.isCancelled else { return }
            // This runs every second during a sync; an unchanged page redraws nothing.
            if messages != fresh {
                // A message that arrives at the end while the conversation is open rises in;
                // a first page, earlier history and anchored windows show at once.
                let arrived = !hasAfter && historyAnchor == nil && !reduceMotion && messages.last.map { last in
                    fresh.contains { $0.id == last.id } && (fresh.last?.sequence ?? 0) > last.sequence } == true
                if arrived { withAnimation(Motion.move) { messages = fresh } } else { messages = fresh }
            }
            if hasEarlier != moreBefore { hasEarlier = moreBefore }
            if nextBefore != cursorBefore { nextBefore = cursorBefore }
            let current = currentChannel
            if historyAnchor == nil && highlightedSequence == nil && firstUnread == nil && current.unread > 0 {
                firstUnread = fresh.first { $0.sequence > current.readThrough && $0.account != model.session?.account }?.sequence
            }
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
            guard epoch == model.epoch, messageRequest == request, !Task.isCancelled else { return }
            if outbox != pending.items { outbox = pending.items }
            model.messageSending.loaded(channel: channel.id, items: fresh, since: sendRevision)
            // Each attachment is looked up once until the library changes.
            for message in fresh where message.attachmentName != nil && !message.attachmentDeleted && !message.unreadable && !checkedAttachments.contains(message.sequence) {
                guard let value = try? await engine.savedAttachment(channel: channel.id, sequence: message.sequence), epoch == model.epoch, !Task.isCancelled else { continue }
                checkedAttachments.insert(message.sequence)
                if let file = try? model.decode(value) as LibraryFile? { receivedFiles[message.sequence] = file }
            }
        } catch {
            if epoch == model.epoch, messageRequest == request, !Task.isCancelled {
                loadFailed = true; model.reportBackground(error)
            }
        }
    }

    private func loadNewer() async {
        guard hasAfter, let after = nextAfter, !loadingNewer, let engine = model.engine else { return }
        let epoch = model.epoch, request = UUID()
        messageRequest = request
        loadingNewer = true; defer { loadingNewer = false }
        do {
            let page = try await engine.messagesAfter(channel: channel.id, after: after, limit: 100)
            guard epoch == model.epoch, messageRequest == request, !Task.isCancelled else { return }
            let merged = Dictionary((messages + page.items).map { ($0.sequence, $0) }, uniquingKeysWith: { _, next in next })
            messages = merged.values.sorted { $0.sequence < $1.sequence }
            hasAfter = page.hasMore
            nextAfter = page.nextBefore
            if !hasAfter { historyAnchor = nil }
        } catch {
            if epoch == model.epoch, messageRequest == request, !Task.isCancelled { model.report(error) }
        }
    }

    private func loadEarlier() async {
        guard hasEarlier, let before = nextBefore, before > 1, !loadingEarlier else { return }
        let epoch = model.epoch, request = UUID(), first = messages.first
        messageRequest = request
        loadingEarlier = true; defer { loadingEarlier = false }
        do {
            let page: MessagePage
            #if DEBUG
            if model.uiTesting { page = model.conversationFixture(channel: channel.id, before: before, query: "") }
            else { page = try await readEarlier(before: before) }
            #else
            page = try await readEarlier(before: before)
            #endif
            guard epoch == model.epoch, messageRequest == request, !Task.isCancelled else { return }
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
        }.privateScreen(.plain).navigationTitle("Select text").navigationBarTitleDisplayMode(.inline)
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
    private var tooLong: Bool { text.utf8.count > ComposerQueue.maxTextBytes }
    var body: some View {
        Form {
            SettingsField("Message", counter: utf8ByteCounter(text, limit: Int64(ComposerQueue.maxTextBytes)), invalid: tooLong) {
                TextEditor(text: $text).frame(minHeight: 180)
            }
            if tooLong { Text("This message is too long.").font(.footnote).foregroundStyle(.red) }
        }.disabled(working)
            .privateScreen().navigationTitle("Edit message").onAppear { scope = model.viewGeneration; text = model.formDraft(key, as: String.self) ?? message.text }
            .interactiveDismissDisabled(working || text != message.text)
            .onDisappear { if text != message.text { model.keepFormDraft(key, value: text, generation: scope) } }
            .confirmationDialog("Leave this form?", isPresented: $leaving) { Button("Keep draft and leave") { dismiss() } }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { if text == message.text { dismiss() } else { leaving = true } }.disabled(working) }
                ToolbarItem(placement: .confirmationAction) { Button("Save") { Task { await save() } }.disabled(working || text == message.text || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || tooLong) }
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
