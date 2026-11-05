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
    @State private var selected: String?
    @State private var mode = 0
    @State private var creating = false
    @State private var writing: LetterDraft?
    @State private var inbox = InboxLoading()
    @State private var folder = LetterFolder.inbox
    @State private var unreadOnly = false
    @State private var query = ""
    @State private var limit = 50
    @State private var names: [String: MessagePerson] = [:]
    @State private var marking: Set<String> = []
    private var channels: [ChannelItem] { model.channels.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) || $0.preview.localizedCaseInsensitiveContains(query) } }
    private func matches(_ account: String, _ text: String) -> Bool { query.isEmpty || account.localizedCaseInsensitiveContains(query) || text.localizedCaseInsensitiveContains(query) || names[account]?.name?.localizedCaseInsensitiveContains(query) == true }
    private var received: [InboxLetter] { inbox.letters.filter { $0.archived == (folder == .archived) && (!unreadOnly || $0.read == false) && !model.blockedAccounts.contains($0.sender) && matches($0.sender, $0.text) } }
    private var sent: [SentLetter] { model.letters.history.filter { $0.pending == (folder == .pending) && matches($0.recipient, $0.text ?? "") } }
    private var drafts: [LetterDraft] { model.letters.drafts.values.filter { !$0.empty && matches($0.recipient, $0.text) }.sorted { $0.updatedAt > $1.updatedAt } }
    private var count: Int { switch folder { case .inbox, .archived: received.count; case .sent, .pending: sent.count; case .drafts: drafts.count } }

    var body: some View {
        WorkspaceNavigation(selection: $selected, sidebarInitiallyVisible: true) { listSelection in
            List(selection: listSelection) {
                Picker("Messages", selection: $mode) { Text("Conversations").tag(0); Text("Letters").tag(1) }
                    .pickerStyle(.segmented).listRowBackground(Color.clear).accessibilityIdentifier("messages.mode")
                if mode == 0 { channelRows }
                else { letterRows }
            }.listStyle(.plain).privateScreen().navigationTitle("Messages")
                .searchable(text: $query, prompt: "Search messages")
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { ApprovalButton() }
                    ToolbarItem(placement: .primaryAction) {
                        NavigationLink { MessageSettingsView() } label: { Label("Message settings", systemImage: "slider.horizontal.3") }.accessibilityIdentifier("messages.settings")
                    }
                    ToolbarItem(placement: .primaryAction) {
                        if mode == 0 { Button("New conversation", systemImage: "plus") { creating = true } }
                        else { Button("Write a letter", systemImage: "square.and.pencil") { writing = LetterDraft() }.accessibilityIdentifier("letters.compose") }
                    }
                }
                .task(id: "channels:\(mode):\(model.epoch):\(model.concealed)") {
                    guard !model.concealed else { return }
                    await model.refresh()
                    guard mode == 0 else { return }
                    while !Task.isCancelled {
                        await model.checkChannels()
                        do { try await Task.sleep(for: .seconds(model.pollSeconds(60))) } catch { break }
                    }
                }
                .task(id: "\(mode):\(model.contentRevision):\(model.epoch)") { if mode == 1 { await loadLetters(refresh: false) } }
                .task(id: "inbox:\(mode):\(model.epoch):\(model.concealed)") {
                    guard mode == 1, !model.concealed else { return }
                    await loadLetters(refresh: false)
                    while !Task.isCancelled {
                        await loadLetters(refresh: true)
                        do { try await Task.sleep(for: .seconds(model.hintsConnected ? 120 : 30)) } catch { break }
                    }
                }
                .onChange(of: model.inboxHints) { _, _ in if mode == 1 { Task { await loadLetters(refresh: true) } } }
                .refreshable { if mode == 1 { await loadLetters(refresh: true) } else { await model.synchronize() } }
        } detail: { id in
            if mode == 1, let id, id.hasPrefix("received:"), let letter = inbox.letters.first(where: { "received:" + $0.id == id }) {
                InboxDetail(letter: letter) { await loadLetters(refresh: false) }.id(letter.id)
            } else if mode == 1, let letter = model.letters.history.first(where: { "sent:" + $0.id == id }) { SentLetterDetail(source: letter).id(letter.id) }
            else if mode == 0, let channel = model.channels.first(where: { $0.id == id }) { ConversationView(channel: channel).id(channel.id) }
            else { ContentUnavailableView("Select a conversation", image: AppGlyph.channels.assetName) }
        }
        .onChange(of: selected) { _, value in if value != nil { model.activity() } }
        .onChange(of: mode) { _, _ in selected = nil; query = ""; limit = 50 }
        .onChange(of: folder) { _, _ in selected = nil; limit = 50; unreadOnly = false }
        .onChange(of: query) { _, _ in limit = 50 }
        .sheet(isPresented: $creating) { NavigationStack { ConversationSetupView { selected = $0 } } }
        .sheet(item: $writing) { draft in NavigationStack { LetterComposerView(draft: draft) } }
    }

    @ViewBuilder private var channelRows: some View {
        ForEach(channels, id: \.id) { channel in
            NavigationLink(value: channel.id) {
                HStack(spacing: 10) {
                    ObjectIcon(glyph: .channels)
                    VStack(alignment: .leading, spacing: 3) {
                        ViewThatFits(in: .horizontal) {
                            HStack { Text(channel.name).font(.body.weight(.medium)); Spacer(); channelTime(channel) }
                            VStack(alignment: .leading) { Text(channel.name).font(.body.weight(.medium)); channelTime(channel) }
                        }
                        Text(channel.draft.isEmpty ? channel.preview : String(localized: "Draft: ") + channel.draft)
                            .font(.caption).foregroundStyle(.secondary).lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 2)
                        HStack {
                            if channel.muted { Label("Mute notifications", systemImage: "bell.slash").labelStyle(.iconOnly) }
                            if channel.pendingSends > 0 { Label("Waiting to submit", systemImage: "clock") }
                            if channel.status == "archived" { Text("Archived") }
                        }.font(.caption2).foregroundStyle(.secondary)
                    }.frame(maxWidth: .infinity, alignment: .leading)
                    if channel.unread > 0 { Text(channel.unread.formatted()).font(.caption.bold()).padding(6).background(Theme.forest.opacity(0.1), in: Circle()).accessibilityLabel("Unread messages").accessibilityValue(channel.unread.formatted()) }
                    else if channel.news { Circle().fill(channel.muted ? Color.secondary : Theme.forest).frame(width: 8, height: 8).accessibilityElement().accessibilityLabel("Unread messages") }
                }
            }.compactObjectRow().accessibilityIdentifier("channel.row.\(channel.id)")
        }
        if channels.isEmpty && model.contentSync?.initialized == true {
            ContentUnavailableView(query.isEmpty ? "No conversations yet" : "No matching results", image: AppGlyph.channels.assetName)
        }
    }

    @ViewBuilder private func channelTime(_ channel: ChannelItem) -> some View {
        if channel.lastAt > 0 { Text(Date(timeIntervalSince1970: Double(channel.lastAt) / 1000), style: .relative).font(.caption2).foregroundStyle(.secondary) }
    }

    @ViewBuilder private var letterRows: some View {
        Picker("Folder", selection: $folder) { ForEach(LetterFolder.allCases) { Text($0.title).tag($0) } }.pickerStyle(.menu).accessibilityIdentifier("letters.folder")
        if folder == .inbox { Toggle("Unread only", isOn: $unreadOnly) }
        if folder == .inbox || folder == .archived {
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
            if inbox.refreshing || !inbox.known && inbox.failure == nil { ProgressView("Syncing…").accessibilityIdentifier("inbox.loading") }
        } else {
            SettingsLoadStatus(loading: model.letters.loading, failed: model.letters.failed) { Task { await model.letters.load(model) } }
            if folder == .drafts {
                ForEach(Array(drafts.prefix(limit))) { draft in
                    Button { writing = draft } label: { LetterSummary(account: draft.recipient, text: draft.text, date: draft.updatedAt, status: String(localized: "Drafts")) }
                        .compactObjectRow().accessibilityIdentifier("letter.draft.\(draft.id)")
                }
            } else {
                ForEach(Array(sent.prefix(limit))) { letter in
                    NavigationLink(value: "sent:" + letter.id) { LetterSummary(account: letter.recipient, text: letter.text ?? "", date: letter.createdAt, status: letterStateTitle(letter.state)) }
                        .tag("sent:" + letter.id)
                        .compactObjectRow().accessibilityIdentifier("letter.sent.\(letter.id)")
                }
            }
            if !model.letters.loading && !model.letters.failed && count == 0 { ContentUnavailableView("No letters yet", image: AppGlyph.inbox.assetName) }
            Text("Drafts and sent letters are saved on this device.").font(.footnote).foregroundStyle(.secondary)
        }
        if count > limit { Button("Load more") { limit += 50 } }
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
        guard let engine = model.engine else { return }
        let epoch = model.epoch
        let accounts = Array(Set(inbox.letters.map(\.sender) + model.letters.history.map(\.recipient))).filter(validMessageAccount)
        for start in stride(from: 0, to: accounts.count, by: 100) {
            let batch = Array(accounts[start..<min(start + 100, accounts.count)])
            if let data = try? await coreOperation({ try await engine.channelNames(accounts: batch, refresh: false, operation: $0) }),
               let next = try? model.decode(data) as [String: MessagePerson], epoch == model.epoch, !Task.isCancelled { names.merge(next) { _, value in value } }
        }
    }

    private func readLetters(refresh: Bool) async throws -> InboxSnapshot {
        let epoch = model.epoch
        #if DEBUG
        if model.uiTesting { return try await model.inboxFixture(refresh: refresh) }
        #endif
        guard let engine = model.engine else { throw CancellationError() }
        do {
            let incoming: [InboxLetter] = try model.decode(try await engine.inbox(refresh: refresh))
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
