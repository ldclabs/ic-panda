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
    @State private var creating = false
    @State private var writing: LetterDraft?
    @State private var inbox = InboxLoading()
    @State private var folder = LetterFolder.inbox
    @State private var unreadOnly = false
    @State private var query = ""
    @State private var limit = 50
    @State private var marking: Set<String> = []
    private var mode: Int { model.route.messagesMode }
    private var channels: [ChannelItem] { model.channels.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) || $0.preview.localizedCaseInsensitiveContains(query) } }

    var body: some View {
        @Bindable var model = model
        WorkspaceNavigation(selection: $model.route.message, sidebarInitiallyVisible: true) { listSelection in
            // Large text scrolls the bar away on phones. A selectable list would swallow taps on its menu.
            let barInList = dynamicTypeSize.isAccessibilitySize && listSelection == nil
            VStack(spacing: 0) {
                if !barInList { bars }
                List(selection: listSelection) {
                    if barInList {
                        bars.buttonStyle(.borderless)
                            .listRowInsets(EdgeInsets()).listRowBackground(Color.clear).listRowSeparator(.hidden)
                    }
                    Group {
                        if mode == 0 { channelRows }
                        else { letterRows }
                    }.plainListRows(selectable: listSelection != nil)
                }.listStyle(.plain)
                    .refreshable { if mode == 1 { await loadLetters(refresh: true) } else { await model.synchronize() } }
            }.privateScreen().navigationTitle("Messages")
                .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search messages")
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { ActivityButton() }
                    ToolbarItemGroup(placement: .primaryAction) {
                        if mode == 0 { Button("New conversation", systemImage: "plus") { creating = true } }
                        else { Button("Write a letter", systemImage: "square.and.pencil") { writing = LetterDraft() }.accessibilityIdentifier("letters.compose") }
                        NavigationLink { MessageSettingsView() } label: { Label("Message settings", systemImage: "ellipsis.circle") }.accessibilityIdentifier("messages.settings")
                    }
                }
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
            else if mode == 0, let channel = model.channels.first(where: { $0.id == id }) { ConversationView(channel: channel).id(channel.id) }
            else if id != nil && (mode == 0 && model.channels.isEmpty || mode == 1 && !inbox.known) { ProgressView("Loading…") }
            else { ContentUnavailableView("Select a conversation", image: AppGlyph.channels.assetName) }
        }
        .onChange(of: model.route.message) { _, value in if value != nil { model.activity() } }
        .onChange(of: mode) { _, _ in model.route.message = nil; query = ""; limit = 50 }
        .onChange(of: folder) { _, _ in model.route.message = nil; limit = 50; unreadOnly = false }
        .onChange(of: query) { _, _ in limit = 50 }
        .sheet(isPresented: $creating) { NavigationStack { ConversationSetupView { model.route.message = $0 } } }
        .sheet(item: $writing) { draft in NavigationStack { LetterComposerView(draft: draft) } }
    }

    @ViewBuilder private var bars: some View {
        @Bindable var model = model
        BrowseScopes(title: "Messages", selection: $model.route.messagesMode, options: [
            .init(value: 0, title: "Conversations", icon: AppGlyph.channels.image, identifier: "messages.mode.conversations"),
            .init(value: 1, title: "Letters", icon: AppGlyph.inbox.image, identifier: "messages.mode.letters"),
        ]).font(.subheadline).padding(.bottom, 8).background(Theme.paper)
        if mode == 1 { letterFilters }
    }

    private var letterFilters: some View {
        HStack(spacing: 6) {
            Menu {
                Picker("Folder", selection: $folder) { ForEach(LetterFolder.allCases) { Text($0.title).tag($0) } }
            } label: {
                HStack(spacing: 5) { Text(folder.title); Image(systemName: "chevron.down").font(.caption) }.browseTabShape(selected: true)
            }.accessibilityLabel("Folder").accessibilityValue(folder.title).accessibilityIdentifier("letters.folder")
            if folder == .inbox {
                BrowseTab(selected: unreadOnly) { unreadOnly.toggle() } label: { Text("Unread only") }
                    .accessibilityIdentifier("letters.unread")
            }
            Spacer(minLength: 0)
        }.font(.subheadline).padding(.horizontal, 16).padding(.bottom, 8).background(Theme.paper)
    }

    @ViewBuilder private var channelRows: some View {
        ForEach(channels, id: \.id) { channel in
            NavigationLink(value: channel.id) { ChannelRow(channel: channel) }
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
        if channels.isEmpty && model.contentSync?.initialized == true {
            ContentUnavailableView(query.isEmpty ? "No conversations yet" : "No matching results", image: AppGlyph.channels.assetName)
        }
    }

    /// Renaming a conversation stays in its settings; muting only keeps the name it has.
    private func setMuted(_ channel: ChannelItem, muted: Bool) async {
        guard let engine = model.engine else { return }
        let epoch = model.epoch
        do {
            _ = try await engine.channelSettings(channel: channel.id, name: channel.name, muted: muted, hidden: false)
            if epoch == model.epoch { await model.loadChannels() }
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

    private func matches(_ account: String, _ text: String) -> Bool {
        query.isEmpty || account.localizedCaseInsensitiveContains(query) || text.localizedCaseInsensitiveContains(query) || model.people.people[account]?.name?.localizedCaseInsensitiveContains(query) == true
    }

    @ViewBuilder private var letterRows: some View {
        if folder == .inbox || folder == .archived {
            let received = inbox.letters.filter { $0.archived == (folder == .archived) && (!unreadOnly || $0.read == false) && !model.blockedAccounts.contains($0.sender) && matches($0.sender, $0.text) }
            ForEach(Array(received.prefix(limit))) { letter in
                NavigationLink(value: "received:" + letter.id) { InboxRow(letter: letter) }.compactObjectRow().accessibilityIdentifier("inbox.row.\(letter.id)")
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
                InboxPolicyHint()
            }
            if inbox.refreshing && received.isEmpty || !inbox.known && inbox.failure == nil { ProgressView("Syncing…").accessibilityIdentifier("inbox.loading") }
            if received.count > limit { LoadMoreRow(trigger: limit) { limit += 50 } }
        } else {
            SettingsLoadStatus(loading: model.letters.loading, failed: model.letters.failed) { Task { await model.letters.load(model) } }
            if folder == .drafts {
                let drafts = model.letters.drafts.values.filter { !$0.empty && matches($0.recipient, $0.text) }.sorted { $0.updatedAt > $1.updatedAt }
                ForEach(Array(drafts.prefix(limit))) { draft in
                    Button { writing = draft } label: { LetterSummary(account: draft.recipient, text: draft.text, date: draft.updatedAt, status: String(localized: "Drafts")) }
                        .compactObjectRow().accessibilityIdentifier("letter.draft.\(draft.id)")
                }
                if !model.letters.loading && !model.letters.failed && drafts.isEmpty { ContentUnavailableView("No letters yet", image: AppGlyph.inbox.assetName) }
                if drafts.count > limit { LoadMoreRow(trigger: limit) { limit += 50 } }
            } else {
                let sent = model.letters.history.filter { $0.pending == (folder == .pending) && matches($0.recipient, $0.text ?? "") }
                ForEach(Array(sent.prefix(limit))) { letter in
                    NavigationLink(value: "sent:" + letter.id) { LetterSummary(account: letter.recipient, text: letter.text ?? "", date: letter.createdAt, status: letterStateTitle(letter.state)) }
                        .tag("sent:" + letter.id)
                        .compactObjectRow().accessibilityIdentifier("letter.sent.\(letter.id)")
                }
                if !model.letters.loading && !model.letters.failed && sent.isEmpty { ContentUnavailableView("No letters yet", image: AppGlyph.inbox.assetName) }
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
            if epoch == model.epoch { await loadLetters(refresh: false) }
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func loadLetters(refresh: Bool) async {
        guard model.unlocked, !model.concealed else { return }
        if refresh { await inbox.refresh(read: readLetters) } else { await inbox.cached(read: readLetters) }
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
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    private var weight: Font.Weight { channel.unread > 0 ? .semibold : .medium }

    var body: some View {
        HStack(spacing: 10) {
            Text(String(channel.name.prefix(1)).uppercased()).font(.body.weight(.semibold)).foregroundStyle(Theme.forest)
                .frame(width: 36, height: 36).background(Theme.forest.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 6) { title; Spacer(minLength: 8); time }
                    VStack(alignment: .leading) { title; time }
                }
                HStack(alignment: .top, spacing: 6) {
                    Text(channel.draft.isEmpty ? channel.preview : String(localized: "Draft: ") + channel.draft)
                        .font(.caption).foregroundStyle(channel.unread > 0 ? .primary : .secondary)
                        .lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 2)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if channel.unread > 0 {
                        Text(channel.unread.formatted()).font(.caption2.bold().monospacedDigit()).foregroundStyle(Theme.paper)
                            .padding(.horizontal, 6).frame(minWidth: 20, minHeight: 20)
                            .background(channel.muted ? Color.secondary : Theme.forest, in: Capsule())
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
                    }.font(.caption2).foregroundStyle(.secondary)
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
            Text(listDateText(ConversationTimeline.date(channel.lastAt))).font(.caption2).foregroundStyle(.secondary).fixedSize()
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
            Text(text).font(.caption).foregroundStyle(.secondary).lineLimit(2)
            HStack { Text(status); Spacer(); if let date, date > 0 { Text(Date(timeIntervalSince1970: Double(date) / 1000), style: .date) } }.font(.caption2).foregroundStyle(.secondary)
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
