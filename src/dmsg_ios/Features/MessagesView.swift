import SwiftUI
@preconcurrency import DMsgCore

enum LetterFolder: String, CaseIterable, Identifiable {
    case inbox, sent, pending, drafts, archived
    var id: String { rawValue }
    var title: String {
        switch self {
        case .inbox: String(localized: "Inbox")
        case .sent: String(localized: "Sent letters")
        case .pending: String(localized: "Pending letters")
        case .drafts: String(localized: "Drafts")
        case .archived: String(localized: "Archived")
        }
    }
}

struct MessagesView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.horizontalSizeClass) private var horizontalSize
    @State private var creating = false
    @State private var writing: LetterDraft?
    @State private var inbox = InboxLoading()
    @State private var folder = LetterFolder.inbox
    @State private var unreadOnly = false
    @State private var query = ""
    @State private var searchScope: WorkspaceSearchScope = .current
    @State private var globalSearch = GlobalSearchStore()
    @State private var hasFileOnly = false
    @State private var channelHits: [ChannelDirectoryHit] = []
    @State private var richGroups: [RichChannelSearchGroup] = []
    @State private var letterHits: [LetterSearchHit] = []
    @State private var completedLetterSearch = ""
    @State private var searched: ChannelSearchKey?
    @State private var searchingChannels = false
    @State private var searchingLetters = false
    @State private var letterSearchFailed = false
    @State private var searchFailed = false
    @State private var searchRetry = 0
    @State private var paging: Set<String> = []
    private var trimmedQuery: String { query.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var searchingAll: Bool { searchScope == .all && !trimmedQuery.isEmpty }
    private var searchKey: ChannelSearchKey {
        ChannelSearchKey(query: mode == 0 && !searchingAll ? trimmedQuery : "",
                         hasFile: mode == 0 && !searchingAll && hasFileOnly,
                         epoch: model.epoch, revision: model.contentRevision, concealed: model.concealed || !model.unlocked,
                         channels: model.channels.map { ChannelSearchSource(id: $0.id, lastAt: $0.lastAt) },
                         messagesRevision: model.messageSearchRevision, blocked: model.blockedAccounts, retry: searchRetry)
    }
    private var letterSearchKey: String {
        guard mode == 1, !searchingAll, !trimmedQuery.isEmpty, !model.concealed, model.unlocked else { return "" }
        return "\(model.epoch):\(model.contentRevision):\(folder.rawValue):\(trimmedQuery):\(searchRetry)"
    }
    @State private var limit = 50
    @State private var marking: Set<String> = []
    private var mode: Int { model.route.messagesMode }
    private var channels: [ChannelItem] {
        let term = searchKey.query
        if term.isEmpty { return model.channels }
        if searched == searchKey && !channelHits.isEmpty {
            let order = Dictionary(uniqueKeysWithValues: channelHits.enumerated().map { ($1.id, $0) })
            return model.channels.filter { order[$0.id] != nil }.sorted { (order[$0.id] ?? Int.max) < (order[$1.id] ?? Int.max) }
        }
        return model.channels.filter { $0.name.localizedCaseInsensitiveContains(term) }
    }
    private var channelHitByID: [String: ChannelDirectoryHit] {
        Dictionary(uniqueKeysWithValues: channelHits.map { ($0.id, $0) })
    }

    var body: some View {
        @Bindable var model = model
        WorkspaceNavigation(selection: $model.route.message, sidebarInitiallyVisible: true) { listSelection in
            // Large text scrolls the bar away on phones. A selectable list would swallow taps on its menu.
            let barInList = dynamicTypeSize.isAccessibilitySize && listSelection == nil
            List(selection: listSelection) {
                if searchingAll {
                    GlobalSearchResultsContent(store: globalSearch, query: query)
                } else {
                if barInList {
                    bars.buttonStyle(.borderless)
                        .listRowInsets(EdgeInsets()).listRowBackground(Color.clear).listRowSeparator(.hidden)
                }
                Group {
                    if mode == 0 { channelRows }
                    else { letterRows }
                }.plainListRows(selectable: listSelection != nil)
                }
            }.listStyle(.plain)
                .globalSearch(store: globalSearch, query: query, active: searchingAll)
                // New, removed, reordered and newly read conversations animate, except with Reduce Motion.
                .motion(Motion.move, value: channels.map { "\($0.id):\($0.unread)" })
                .refreshable { if mode == 1 { await loadLetters(refresh: true) } else { await model.synchronize() } }
                .scopeBar { if !barInList && !searchingAll { bars } }
                .privateScreen(.plain).navigationTitle("Messages")
                .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: mode == 0 ? "Search messages" : "Search letters")
                .searchScopes($searchScope) {
                    Text("Current").tag(WorkspaceSearchScope.current)
                    Text("All").tag(WorkspaceSearchScope.all)
                }
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { ActivityButton() }
                    ToolbarItemGroup(placement: .primaryAction) {
                        if mode == 0 { Button("New conversation", systemImage: "plus") { creating = true } }
                        else { Button("Write a letter", systemImage: "square.and.pencil") { writing = LetterDraft() }.accessibilityIdentifier("letters.compose") }
                        NavigationLink { MessageSettingsView() } label: { Label("Message settings", systemImage: "ellipsis.circle") }.accessibilityIdentifier("messages.settings")
                    }
                }
                .task(id: searchKey) { await searchAll(searchKey) }
                .task(id: letterSearchKey) { await searchLetters() }
                .task(id: "\(mode):\(model.epoch):\(model.concealed)") {
                    guard !model.concealed else { return }
                    if mode == 0 {
                        await model.refresh()
                        while !Task.isCancelled {
                            await model.checkChannels()
                            do { try await Task.sleep(for: .seconds(model.pollSeconds(60))) } catch { break }
                        }
                    } else {
                        await loadLetters(refresh: false)
                        while !Task.isCancelled {
                            await loadLetters(refresh: true)
                            do { try await Task.sleep(for: .seconds(model.pollSeconds(30))) } catch { break }
                        }
                    }
                }
                .onChange(of: model.contentRevision) { _, _ in if mode == 1 { Task { await loadLetters(refresh: false) } } }
                .onChange(of: model.inboxHints) { _, _ in if mode == 1 { Task { await loadLetters(refresh: true) } } }
        } detail: { id in
            if mode == 1, let id, id.hasPrefix("received:"), let letter = inbox.letters.first(where: { "received:" + $0.id == id }) {
                InboxDetail(letter: letter) { await loadLetters(refresh: false) }.id(letter.id)
            } else if mode == 1, let letter = model.letters.history.first(where: { "sent:" + $0.id == id }) { SentLetterDetail(source: letter).id(letter.id) }
            else if mode == 0, let id, let target = ChannelSearchTarget(route: id), let channel = model.channels.first(where: { $0.id == target.channel }) {
                ConversationView(channel: channel, at: target.sequence).id(id)
            } else if mode == 0, let channel = model.channels.first(where: { $0.id == id }) { ConversationView(channel: channel).id(channel.id) }
            else if id != nil && (mode == 0 && model.channels.isEmpty || mode == 1 && !inbox.known) { ProgressView("Loading…") }
            else if mode == 1 { ContentUnavailableView("Select a letter", image: AppGlyph.inbox.assetName) }
            else { ContentUnavailableView("Select a conversation", image: AppGlyph.channels.assetName) }
        }
        .onChange(of: model.route.message) { _, value in if value != nil { model.activity() } }
        .onChange(of: mode) { _, _ in query = ""; searchScope = .current; hasFileOnly = false; limit = 50 }
        .onChange(of: model.route.letterDraft.flatMap { model.letters.drafts[$0] }, initial: true) { _, draft in
            if let draft { writing = draft; model.route.letterDraft = nil }
        }
        .onChange(of: folder) { _, _ in model.route.message = nil; limit = 50; unreadOnly = false }
        .onChange(of: query) { _, value in
            limit = 50
            if value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { searchScope = .current; hasFileOnly = false }
        }
        .sheet(isPresented: $creating) { NavigationStack { ConversationSetupView { model.route.message = $0 } } }
        .sheet(item: $writing) { draft in NavigationStack { LetterComposerView(draft: draft) } }
    }

    @ViewBuilder private var bars: some View {
        @Bindable var model = model
        VStack(spacing: 0) {
            BrowseScopes(title: "Messages", selection: $model.route.messagesMode, options: [
                .init(value: 0, title: "Conversations", icon: AppGlyph.channels.image, identifier: "messages.mode.conversations", badge: model.unreadConversations),
                .init(value: 1, title: "Letters", icon: AppGlyph.inbox.image, identifier: "messages.mode.letters", badge: model.unreadLetters),
            ]).font(.subheadline).padding(.bottom, 8)
            if mode == 0 && (!trimmedQuery.isEmpty || hasFileOnly) {
                HStack(spacing: 6) {
                    BrowseTab(selected: hasFileOnly) { hasFileOnly.toggle() } label: {
                        Image(systemName: "paperclip").accessibilityHidden(true)
                        Text("Attachments")
                    }.accessibilityIdentifier("messages.filter.attachments")
                    Spacer(minLength: 0)
                }.font(.subheadline).padding(.horizontal, 16).padding(.bottom, 8)
            }
            if mode == 1 { letterFilters }
        }
    }

    private var letterFilters: some View {
        HStack(spacing: 6) {
            Menu {
                Picker("Folder", selection: $folder) { ForEach(LetterFolder.allCases) { Text($0.title).tag($0) } }
            } label: {
                HStack(spacing: 5) { Text(folder.title); Image(systemName: "chevron.down").font(.footnote) }.browseTabShape(selected: true)
            }.accessibilityLabel("Folder").accessibilityValue(folder.title).accessibilityIdentifier("letters.folder")
            if folder == .inbox {
                BrowseTab(selected: unreadOnly) { unreadOnly.toggle() } label: { Text("Unread only") }
                    .accessibilityIdentifier("letters.unread")
            }
            Spacer(minLength: 0)
        }.font(.subheadline).padding(.horizontal, 16).padding(.bottom, 8)
    }

    @ViewBuilder private var channelRows: some View {
        let hitLookup = channelHitByID
        ForEach(channels, id: \.id) { channel in
            NavigationLink(value: channel.id) {
                if searchKey.query.isEmpty {
                    ChannelRow(channel: channel)
                } else {
                    let hit = hitLookup[channel.id]
                    HStack(spacing: 10) {
                        Avatar(channel.name, seed: channel.id, kind: .conversation)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(highlightedText(channel.name, spans: hit?.nameSpans)).font(.body.weight(.medium)).lineLimit(1)
                            if hit?.matchField == "member", let member = hit?.matchValue, !member.isEmpty {
                                Text(String(localized: "Member") + ": " + member).rowSubtitle().lineLimit(1)
                            }
                        }.frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
                .compactObjectRow().accessibilityIdentifier("channel.row.\(channel.id)")
                .swipeActions {
                    Button(LocalizedStringKey(channel.muted ? "Unmute notifications" : "Mute notifications"), systemImage: channel.muted ? "bell" : "bell.slash") {
                        Task { await setMuted(channel, muted: !channel.muted) }
                    }.tint(Theme.forest)
                    if channel.unread > 0 || channel.news {
                        Button("Mark as read", systemImage: "checkmark") { Task { await markRead(channel) } }
                    }
                }
        }
        if !searchKey.query.isEmpty || searchKey.hasFile {
            Text("Searches messages saved on this device.").font(.footnote).foregroundStyle(.secondary)
            if searchKey.tooLong { Text("Use up to 200 characters to search.") }
            else {
                if searched == searchKey {
                    ForEach(richGroups) { group in
                        let title = model.channels.first(where: { $0.id == group.channelId })?.name ?? group.channelName
                        Section(title) {
                            ForEach(group.hits) { hit in
                                let route = ChannelSearchTarget.route(channel: group.channelId, sequence: hit.sequence)
                                NavigationLink(value: route) {
                                    VStack(alignment: .leading, spacing: 4) {
                                        if !hit.text.isEmpty {
                                            Text(highlightedText(hit.text, spans: hit.snippetSpans)).lineLimit(2)
                                        }
                                        if let name = hit.attachmentName {
                                            Label { Text(highlightedText(name, spans: hit.fileSpans)).lineLimit(2) } icon: { Image(systemName: "paperclip") }
                                        }
                                        HStack(spacing: 6) {
                                            if let sender = hit.senderName, !sender.isEmpty { Text(sender).lineLimit(1) }
                                            Spacer(minLength: 4)
                                            if hit.message.timestampMs > 0 {
                                                Text(ConversationTimeline.date(hit.message.timestampMs), format: .dateTime)
                                            }
                                        }.font(.footnote).foregroundStyle(.secondary)
                                    }
                                }.tag(route).accessibilityIdentifier("messages.hit." + group.channelId + ".\(hit.sequence)")
                            }
                            if let before = group.nextBefore {
                                Button("Load earlier messages") { Task { await moreHits(group.channelId, before: before, for: searchKey) } }
                                    .disabled(paging.contains(group.channelId))
                            }
                        }
                    }
                    if searchFailed { SettingsLoadStatus(loading: false, failed: true) { searchRetry += 1 } }
                    if searchingChannels { ProgressView("Searching…") }
                    else if !searchFailed && channels.isEmpty && richGroups.isEmpty { ContentUnavailableView.search(text: query) }
                } else { ProgressView("Searching…") }
            }
        } else if channels.isEmpty {
            if model.contentSync?.initialized == true {
                ContentUnavailableView("No conversations yet", image: AppGlyph.channels.assetName, description: Text("Start a conversation to exchange messages and files with members."))
                Button("New conversation") { creating = true }
            } else {
                ContentUnavailableView(ContentSyncStatusView.title(model.contentSync), systemImage: "arrow.triangle.2.circlepath")
            }
        }
    }

    private func searchAll(_ key: ChannelSearchKey) async {
        channelHits = []; richGroups = []; paging = []; searchFailed = false; searched = key
        searchingChannels = (!key.query.isEmpty || key.hasFile) && !key.concealed && !key.tooLong
        guard searchingChannels else { return }
        defer { if searchKey == key && !Task.isCancelled { searchingChannels = false } }
        do { try await Task.sleep(for: .milliseconds(160)) } catch { return }
        guard await model.loadBlockedAccounts() else {
            if searchKey == key && !Task.isCancelled { searchFailed = true }
            return
        }
        #if DEBUG
        if model.uiTesting {
            var groups: [RichChannelSearchGroup] = []
            for source in key.channels {
                guard searchKey == key && !Task.isCancelled else { return }
                let page = model.conversationFixture(channel: source.id, before: nil, query: key.query)
                let hits = page.items.reversed().compactMap { item -> RichMessageSearchHit? in
                    guard !item.hidden && !item.retracted && !item.unreadable && !key.blocked.contains(item.account) else { return nil }
                    if key.hasFile && (item.attachmentDeleted || item.attachmentName == nil) { return nil }
                    if !key.query.isEmpty && !item.text.localizedCaseInsensitiveContains(key.query)
                        && !(!item.attachmentDeleted && item.attachmentName?.localizedCaseInsensitiveContains(key.query) == true) { return nil }
                    let file = (!item.attachmentDeleted ? item.attachmentName : nil).map { MessageHitFile(name: $0) }
                    return RichMessageSearchHit(channel: source.id,
                                                message: MessageHitBody(seq: item.sequence, messageId: item.id, account: item.account,
                                                                        text: item.text, createdAt: item.createdAtMs, storedAt: item.createdAtMs,
                                                                        file: file, fileDeleted: item.attachmentDeleted))
                }
                if !hits.isEmpty {
                    let name = model.channels.first(where: { $0.id == source.id })?.name ?? source.id
                    groups.append(RichChannelSearchGroup(channelId: source.id, channelName: name, hits: hits, nextBefore: page.hasMore ? page.nextBefore : nil))
                }
            }
            if searchKey == key && !Task.isCancelled { richGroups = groups }
            return
        }
        #endif
        guard let engine = model.engine else { searchFailed = true; return }
        do {
            async let listTask: [ChannelDirectoryHit] = key.query.isEmpty ? [] : model.decodeInBackground(try await engine.searchChannels(query: key.query))
            async let messagesTask: [RichChannelSearchGroup] = model.decodeInBackground(
                try await engine.searchMessagesAll(query: key.query, limitPerChannel: 5, maxChannels: 20, hasFile: key.hasFile)
            )
            let (foundChannels, rawGroups) = try await (listTask, messagesTask)
            guard searchKey == key && !Task.isCancelled else { return }
            channelHits = foundChannels
            richGroups = rawGroups.compactMap { group in
                let filtered = group.hits.filter { !key.blocked.contains($0.message.account) }
                guard !filtered.isEmpty else { return nil }
                return RichChannelSearchGroup(channelId: group.channelId, channelName: group.channelName,
                                              hits: filtered, nextBefore: group.nextBefore, topScore: group.topScore)
            }
        } catch {
            if searchKey == key && !Task.isCancelled { searchFailed = true }
        }
    }

    private func moreHits(_ channel: String, before: UInt64, for key: ChannelSearchKey) async {
        guard searched == key, paging.insert(channel).inserted else { return }
        defer { if searched == key { paging.remove(channel) } }
        guard let engine = model.engine else { return }
        do {
            let page: RichChannelMessagePage = try await model.decodeInBackground(
                try await engine.searchChannelMessages(channel: channel, query: key.query, before: before, limit: 20, hasFile: key.hasFile, sender: nil)
            )
            guard searchKey == key, let index = richGroups.firstIndex(where: { $0.channelId == channel && $0.nextBefore == before }) else { return }
            let filtered = page.hits.filter { !key.blocked.contains($0.message.account) }
            richGroups[index].hits += filtered
            richGroups[index].nextBefore = page.hasMore ? page.nextBefore : nil
        } catch { if searchKey == key && !Task.isCancelled { searchFailed = true } }
    }

    private func searchLetters() async {
        let key = letterSearchKey
        let term = trimmedQuery
        guard !key.isEmpty, !term.isEmpty else {
            letterHits = []; searchingLetters = false
            return
        }
        #if DEBUG
        if model.uiTesting { letterHits = []; searchingLetters = false; return }
        #endif
        guard let engine = model.engine else { return }
        searchingLetters = true
        letterSearchFailed = false
        defer { if letterSearchKey == key { searchingLetters = false } }
        do {
            try await Task.sleep(for: .milliseconds(160))
            guard letterSearchKey == key, !Task.isCancelled else { return }
            let boxKind: String = {
                switch folder {
                case .inbox: "inbox"
                case .archived: "archive"
                case .sent: "sent"
                case .pending: "pending"
                case .drafts: "draft"
                }
            }()
            let hits: [LetterSearchHit] = try await model.decodeInBackground(try await engine.searchInbox(query: term, boxKind: boxKind))
            guard letterSearchKey == key, !Task.isCancelled else { return }
            letterHits = hits
            completedLetterSearch = key
        } catch {
            if letterSearchKey == key, !Task.isCancelled { letterSearchFailed = true; completedLetterSearch = key }
        }
    }

    /// Renaming a conversation stays in its settings; muting only keeps the name it has.
    private func setMuted(_ channel: ChannelItem, muted: Bool) async {
        guard let engine = model.engine else { return }
        let epoch = model.epoch
        do {
            _ = try await engine.channelSettings(channel: channel.id, name: channel.name, muted: muted, hidden: false)
            if epoch == model.epoch {
                await model.loadChannels()
                await model.notifications.refresh(model, receive: true)
            }
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func markRead(_ channel: ChannelItem) async {
        guard let engine = model.engine else { return }
        let epoch = model.epoch
        do {
            let page = try await engine.messages(channel: channel.id, before: nil, limit: 1)
            guard epoch == model.epoch, let last = page.items.last else { return }
            await model.channelRead(channel.id, through: last.sequence)
            if epoch == model.epoch { await model.loadChannels() }
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private var emptyLetterFolder: some View {
        ContentUnavailableView(query.isEmpty ? "No letters yet" : "No matching results", image: AppGlyph.inbox.assetName)
    }

    private func matches(_ account: String, _ text: String) -> Bool {
        query.isEmpty || account.localizedCaseInsensitiveContains(query) || text.localizedCaseInsensitiveContains(query) || model.people.people[account]?.name?.localizedCaseInsensitiveContains(query) == true
    }

    @ViewBuilder private var letterRows: some View {
        if !trimmedQuery.isEmpty && !model.uiTesting {
            let hits = letterHits.filter { !model.blockedAccounts.contains($0.account) && (folder != .inbox || !unreadOnly || !$0.read) }
            if completedLetterSearch != letterSearchKey || searchingLetters {
                ProgressView("Searching…").frame(maxWidth: .infinity)
            } else if letterSearchFailed {
                NoticeBar(text: String(localized: "The operation could not be completed. Please try again."))
                Button("Retry") { searchRetry += 1 }
            } else if hits.isEmpty {
                emptyLetterFolder
            } else {
                ForEach(hits.prefix(limit)) { hit in
                    if hit.boxKind == "draft", let draft = model.letters.drafts[hit.id] {
                        Button { writing = draft } label: {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(highlightedText(hit.displayName ?? hit.account, spans: hit.nameSpans)).font(.body.weight(.medium)).lineLimit(1)
                                Text(highlightedText(draft.text, spans: hit.snippetSpans)).rowSubtitle().lineLimit(2)
                            }
                        }.compactObjectRow().accessibilityIdentifier("letter.draft.\(hit.id)")
                    } else {
                        let routeValue = (hit.boxKind == "sent" || hit.boxKind == "pending") ? "sent:" + hit.id : "received:" + hit.id
                        NavigationLink(value: routeValue) {
                            VStack(alignment: .leading, spacing: 4) {
                                HStack(spacing: 6) {
                                    Text(highlightedText(hit.displayName ?? hit.account, spans: hit.nameSpans)).font(.body.weight(.medium)).lineLimit(1)
                                    Spacer(minLength: 4)
                                    if hit.createdAt > 0 {
                                        Text(Date(timeIntervalSince1970: Double(hit.createdAt) / 1000), style: .date).font(.footnote).foregroundStyle(.secondary)
                                    }
                                }
                                if let spans = hit.snippetSpans, !spans.isEmpty {
                                    Text(highlightedText("", spans: spans)).rowSubtitle().lineLimit(2)
                                }
                            }
                        }.tag(routeValue).compactObjectRow()
                    }
                }
                if hits.count > limit { LoadMoreRow(trigger: limit) { limit += 50 } }
            }
        } else if folder == .inbox || folder == .archived {
            let received = inbox.letters.filter { $0.archived == (folder == .archived) && (!unreadOnly || $0.read == false) && !model.blockedAccounts.contains($0.sender) && matches($0.sender, $0.text) }
            ForEach(Array(received.prefix(limit))) { letter in
                NavigationLink(value: "received:" + letter.id) { InboxRow(letter: letter, selected: horizontalSize == .regular && model.route.message == "received:" + letter.id) }.compactObjectRow().accessibilityIdentifier("inbox.row.\(letter.id)")
                    .tag("received:" + letter.id)
                    .swipeActions {
                        Button(LocalizedStringKey(letter.archived ? "Restore" : "Archive")) { Task { await mark(letter, read: letter.read, archived: !letter.archived) } }.tint(Theme.forest)
                        Button(LocalizedStringKey(letter.read ? "Mark as unread" : "Mark as read")) { Task { await mark(letter, read: !letter.read, archived: letter.archived) } }
                    }
                    .disabled(marking.contains(letter.id))
            }
            if let failure = inbox.failure {
                NoticeBar(text: failure).accessibilityIdentifier("inbox.failure")
                Button("Retry") { Task { await loadLetters(refresh: true) } }.disabled(inbox.refreshing).accessibilityIdentifier("inbox.retry")
            } else if received.isEmpty && inbox.known && !inbox.refreshing {
                ContentUnavailableView(query.isEmpty && !unreadOnly ? (folder == .archived ? "No archived letters" : "No letters yet") : "No matching results", image: AppGlyph.inbox.assetName).accessibilityIdentifier("inbox.empty")
                if folder == .inbox && query.isEmpty && !unreadOnly {
                    Button("Write a letter") { writing = LetterDraft() }
                    InboxPolicyHint()
                }
            }
            if inbox.refreshing && received.isEmpty || !inbox.known && inbox.failure == nil { PlaceholderRows(identifier: "inbox.loading") }
            if received.count > limit { LoadMoreRow(trigger: limit) { limit += 50 } }
        } else {
            SettingsLoadStatus(loading: model.letters.loading, failed: model.letters.failed) { Task { await model.letters.load(model) } }
            if folder == .drafts {
                let drafts = model.letters.drafts.values.filter { !$0.empty && matches($0.recipient, $0.text) }.sorted { $0.updatedAt > $1.updatedAt }
                ForEach(Array(drafts.prefix(limit))) { draft in
                    Button { writing = draft } label: { LetterSummary(account: draft.recipient, text: draft.text, date: draft.updatedAt, status: String(localized: "Drafts")) }
                        .compactObjectRow().accessibilityIdentifier("letter.draft.\(draft.id)")
                }
                if !model.letters.loading && !model.letters.failed && drafts.isEmpty { emptyLetterFolder }
                if drafts.count > limit { LoadMoreRow(trigger: limit) { limit += 50 } }
            } else {
                let sent = model.letters.history.filter { $0.pending == (folder == .pending) && matches($0.recipient, $0.text ?? "") }
                ForEach(Array(sent.prefix(limit))) { letter in
                    NavigationLink(value: "sent:" + letter.id) { LetterSummary(account: letter.recipient, text: letter.text ?? "", date: letter.createdAt, status: letterStateTitle(letter.state)) }
                        .tag("sent:" + letter.id)
                        .compactObjectRow().accessibilityIdentifier("letter.sent.\(letter.id)")
                }
                if !model.letters.loading && !model.letters.failed && sent.isEmpty { emptyLetterFolder }
                if sent.count > limit { LoadMoreRow(trigger: limit) { limit += 50 } }
            }
            Text("Drafts and sent letters are saved on this device.").font(.footnote).foregroundStyle(.secondary)
        }
    }

    private func mark(_ letter: InboxLetter, read: Bool, archived: Bool) async {
        guard !marking.contains(letter.id) else { return }
        marking.insert(letter.id); defer { marking.remove(letter.id) }
        let epoch = model.epoch
        do {
            #if DEBUG
            if model.uiTesting { model.markInboxFixture(order: letter.id, read: read, archived: archived) }
            else { _ = try await model.engine?.markLetter(order: letter.id, read: read, archived: archived) }
            #else
            _ = try await model.engine?.markLetter(order: letter.id, read: read, archived: archived)
            #endif
            if epoch == model.epoch {
                await loadLetters(refresh: false)
                await model.notifications.refresh(model)
            }
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func loadLetters(refresh: Bool) async {
        guard model.unlocked, !model.concealed else { return }
        if refresh { await inbox.refresh(read: readLetters) } else { await inbox.cached(read: readLetters) }
        model.updateUnreadLetters(inbox.letters)
        await model.letters.load(model)
        model.people.request(inbox.letters.map(\.sender) + model.letters.history.map(\.recipient), model: model)
    }

    private func readLetters(refresh: Bool) async throws -> InboxSnapshot {
        let epoch = model.epoch
        #if DEBUG
        if model.uiTesting { return try await model.inboxFixture(refresh: refresh) }
        #endif
        guard let engine = model.engine else { throw CancellationError() }
        do {
            let incoming: [InboxLetter] = try await model.decodeInBackground(try await engine.inbox(refresh: refresh))
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

/// A conversation in the list: bold while it has unread messages. Its time does
/// not tick, so the list stays still.
private struct ChannelRow: View {
    let channel: ChannelItem
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    private var weight: Font.Weight { channel.unread > 0 ? .semibold : .medium }
    /// A draft leads with its label in the accent colour, as Messages marks drafts.
    private var preview: Text {
        let text = model.drafts[channel.id] ?? channel.draft
        let draft = !text.isEmpty ? text : model.attachmentDrafts[channel.id]?.first?.name
        guard let draft else { return Text(channel.preview) }
        return Text("Draft: ").foregroundStyle(Theme.forest) + Text(draft)
    }

    var body: some View {
        HStack(spacing: 10) {
            Avatar(channel.name, seed: channel.id, kind: .conversation)
            VStack(alignment: .leading, spacing: 3) {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 6) { title; Spacer(minLength: 8); time }
                    VStack(alignment: .leading) { title; time }
                }
                HStack(alignment: .top, spacing: 6) {
                    preview
                        .font(.subheadline).foregroundStyle(channel.unread > 0 ? .primary : .secondary)
                        .lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 2)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if channel.unread > 0 {
                        Text(channel.unread.formatted()).font(.caption2.bold().monospacedDigit()).foregroundStyle(Theme.paper).countMotion(channel.unread)
                            .padding(.horizontal, 6).frame(minWidth: 20, minHeight: 20)
                            .background(channel.muted ? Color.secondary : Theme.forest, in: Capsule())
                            .popping()
                            .accessibilityLabel("Unread messages").accessibilityValue(channel.unread.formatted())
                    } else if channel.news {
                        Circle().fill(channel.muted ? Color.secondary : Theme.forest).frame(width: 8, height: 8)
                            .accessibilityElement().accessibilityLabel("Unread messages")
                    }
                }
                if channel.pendingSends > 0 || channel.status == "archived" {
                    HStack {
                        if channel.pendingSends > 0 { Label("Waiting to submit", systemImage: "clock") }
                        if channel.status == "archived" { Text("Archived") }
                    }.font(.footnote).foregroundStyle(.secondary)
                }
            }.frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var title: some View {
        HStack(spacing: 4) {
            Text(channel.name).font(.body.weight(weight)).lineLimit(1)
            if channel.muted { Image(systemName: "bell.slash").font(.caption2).foregroundStyle(.secondary).accessibilityLabel("Mute notifications") }
        }
    }

    @ViewBuilder private var time: some View {
        if channel.lastAt > 0 {
            Text(listDateText(ConversationTimeline.date(channel.lastAt))).rowSubtitle().fixedSize()
        }
    }
}

struct LetterSummary: View {
    let account: String
    let text: String
    let date: UInt64?
    let status: String
    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            if !account.isEmpty { MessageIdentity(account: account) }
            Text(text).rowSubtitle().lineLimit(2)
            HStack { Text(status); Spacer(); if let date, date > 0 { Text(Date(timeIntervalSince1970: Double(date) / 1000), style: .date) } }.font(.footnote).foregroundStyle(.secondary)
        }
    }
}

private struct InboxPolicyHint: View {
    @Environment(SessionModel.self) private var model
    @State private var mode: String?
    private var title: String {
        switch mode {
        case "closed": String(localized: "Closed")
        case "contacts": String(localized: "Selected accounts")
        case "free": String(localized: "Anyone, free of charge")
        default: String(localized: "Current policy requires payment")
        }
    }
    var body: some View {
        if mode != nil { LabeledContent("Who may contact you", value: title).font(.footnote) }
        NavigationLink("Inbox settings") { InboxSettingsView() }
            .task {
                let epoch = model.epoch
                if model.uiTesting { mode = "closed"; return }
                struct Policy: Decodable { let mode: String }
                do {
                    guard let engine = model.engine else { return }
                    let policy: Policy? = try model.decode(try await engine.inboxPolicy())
                    if epoch == model.epoch, !Task.isCancelled { mode = policy?.mode ?? "closed" }
                } catch { /* Unknown policy stays unknown; settings offers retry. */ }
            }
    }
}

private struct ChannelSearchKey: Equatable {
    let query: String
    let hasFile: Bool
    let epoch: UInt64
    let revision: UInt64
    let concealed: Bool
    let channels: [ChannelSearchSource]
    let messagesRevision: UInt64
    let blocked: Set<String>
    let retry: Int
    var tooLong: Bool { query.lowercased().unicodeScalars.count > 200 }
}

private struct ChannelSearchSource: Equatable {
    let id: String
    let lastAt: UInt64
}

private struct ChannelSearchTarget {
    let channel: String
    let sequence: UInt64

    static func route(channel: String, sequence: UInt64) -> String { "hit:\(channel):\(sequence)" }

    init?(route: String) {
        let fields = route.split(separator: ":", maxSplits: 2)
        guard fields.count == 3, fields[0] == "hit", let value = UInt64(fields[2]) else { return nil }
        channel = String(fields[1]); sequence = value
    }
}
