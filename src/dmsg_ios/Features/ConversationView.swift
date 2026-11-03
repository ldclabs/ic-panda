import SwiftUI
@preconcurrency import DMsgCore

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
    @State private var attachment: LibraryFile?
    @State private var receivedFiles: [UInt64: LibraryFile] = [:]
    @State private var openingFile: LibraryFile?
    @State private var receiving: UInt64?
    @State private var firstUnread: UInt64?
    private var currentChannel: ChannelItem { model.channels.first { $0.id == channel.id } ?? channel }
    private var canPublish: Bool { currentChannel.canPublish }
    private var searchQuery: String { query.trimmingCharacters(in: .whitespacesAndNewlines) }
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
    @State private var participants: [String: Participant] = [:]
    @State private var avatars: [String: UIImage] = [:]
    @State private var visibleParticipants: Set<String> = []
    private struct Participant: Decodable { let name: String?; let avatarUrl: String? }
    private var participantIDs: [String] { Array(visibleParticipants.union([model.session?.account ?? ""])).filter { !$0.isEmpty }.sorted() }
    struct MessageEdit: Identifiable { let message: MessageItem; var id: String { message.id } }
    private var shownMessages: [MessageItem] {
        model.messageSending.messages(channel: channel.id, stored: messages)
            .filter { !model.blockedAccounts.contains($0.account) && (searchQuery.isEmpty || $0.text.localizedCaseInsensitiveContains(searchQuery) || $0.attachmentName?.localizedCaseInsensitiveContains(searchQuery) == true) }
    }
    private var shownOutbox: [OutgoingMessage] {
        // A verified local copy can arrive before the original send's ACK.
        let received = Set(model.messageSending.messages(channel: channel.id, stored: messages).filter { $0.account == model.session?.account }.map(\.id))
        return model.messageSending.items(channel: channel.id, stored: outbox, received: received)
            .filter { searchQuery.isEmpty || $0.text.localizedCaseInsensitiveContains(searchQuery) || $0.attachmentName?.localizedCaseInsensitiveContains(searchQuery) == true }
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
                        ContentUnavailableView(searchQuery.isEmpty ? "No messages" : "No matching results", image: AppGlyph.channels.assetName)
                            .accessibilityIdentifier("conversation.empty")
                    }
                    if !searchQuery.isEmpty {
                        Text("Searches messages saved on this device.").font(.caption).foregroundStyle(.secondary)
                        LabeledContent("Messages", value: (shownMessages.count + shownOutbox.count).formatted() + (hasEarlier ? "+" : "")).font(.caption).foregroundStyle(.secondary)
                    }
                    if hasEarlier {
                        Button(LocalizedStringKey(searchQuery.isEmpty ? "Load earlier messages" : "Load more")) { actionTask = Task { await loadEarlier() } }
                            .disabled(loadingEarlier).accessibilityIdentifier("conversation.earlier")
                    }
                    ForEach(shownMessages, id: \.id) { message in
                        if dayStarts.contains(message.id) { Text(Date(timeIntervalSince1970: Double(message.createdAtMs) / 1000), style: .date).font(.caption).foregroundStyle(.secondary).frame(maxWidth: .infinity) }
                        if message.sequence == firstUnread && searchQuery.isEmpty { Label("Unread messages", systemImage: "circle.fill").font(.caption).foregroundStyle(Theme.forest).frame(maxWidth: .infinity) }
                        messageBubble(message) { sequence in actionTask = Task { await reveal(sequence, proxy: proxy) } }
                            .onAppear { visibleParticipants.insert(message.account) }
                            .reminderVisible(.channel(channel.id), watermark: message.sequence,
                                ready: query.isEmpty && !message.unreadable && message.id == shownMessages.last?.id) {
                                try? await model.engine?.markRead(channel: channel.id, through: message.sequence)
                            }
                            .id(message.id).accessibilityIdentifier("message.row.\(message.sequence)").contextMenu {
                            if !message.hidden && !message.unreadable && !message.retracted {
                                if canPublish { Button("Reply") { replying = message } }
                                if !message.text.isEmpty { Button("Copy") { UIPasteboard.general.string = message.text } }
                                if message.account == model.session?.account && canPublish {
                                    if !message.text.isEmpty { Button("Edit message") { editingMessage = MessageEdit(message: message) } }
                                    Button("Withdraw message", role: .destructive) { retracting = message }
                                }
                            }
                            Button(LocalizedStringKey(message.hidden ? "Show" : "Hide on this device")) { actionTask = Task { await hide(message, hidden: !message.hidden) } }
                        }
                    }
                    // Pending and confirmed rows need distinct IDs so
                    // LazyVStack replaces the sending UI after an ACK.
                    ForEach(shownOutbox, id: \.id) { pending in
                        ConversationMessageRow(own: true, name: participantName(model.session?.account ?? ""), avatar: participantAvatar(model.session?.account ?? "")) {
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
                .overlay(alignment: .bottomTrailing) {
                    if !nearBottom && searchQuery.isEmpty {
                        Button("Latest messages", systemImage: "arrow.down") { if let id = shownMessages.last?.id { proxy.scrollTo(id, anchor: .bottom) } }
                            .buttonStyle(.borderedProminent).padding(12).accessibilityIdentifier("conversation.latest")
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
                if !canPublish { NoticeBar(text: channelRestriction(currentChannel.status)).padding(.horizontal) }
                if let replying {
                    HStack(alignment: .top) {
                        VStack(alignment: .leading) { Text(participantName(replying.account)).font(.caption.bold()); Text(replying.text.isEmpty ? replying.attachmentName ?? "" : replying.text).font(.caption).lineLimit(2) }
                        Spacer(); Button("Cancel reply", systemImage: "xmark.circle") { self.replying = nil }.labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44)
                    }.padding(.horizontal)
                }
                if let attachment {
                    HStack { VStack(alignment: .leading) { Label(attachment.name, systemImage: "doc").lineLimit(2); Text(binarySize(attachment.size)).font(.caption).foregroundStyle(.secondary) }; Spacer(); Button("Remove", systemImage: "xmark.circle") { self.attachment = nil }.labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44) }.padding(.horizontal).accessibilityIdentifier("message.attachment.draft")
                }
                if receiving != nil { TransferStatusView().padding(.horizontal) }
                HStack(alignment: .bottom, spacing: 10) {
                    Menu("Attach", systemImage: "paperclip") {
                        Button("Attach file", systemImage: "doc") { choosingAttachment = true }
                        Button("Share secret snapshot", systemImage: "lock.doc") { sharingSecret = true }
                    }.labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44).disabled(!canPublish || queuing)
                    TextField("Write a message", text: Binding(get: { draft }, set: { draft = $0 }), axis: .vertical).lineLimit(1...5)
                        .padding(12).background(Theme.surface, in: RoundedRectangle(cornerRadius: 16))
                        .accessibilityIdentifier("message.draft").disabled(!canPublish)
                    Button { actionTask = Task { await send() } } label: { Image(systemName: "arrow.up").font(.headline).frame(width: 44, height: 44) }
                        .buttonStyle(.borderedProminent).buttonBorderShape(.circle).accessibilityLabel("Send message")
                        .accessibilityIdentifier("message.send")
                        .disabled(!canPublish || (draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && attachment == nil) || draft.utf8.count > 16 * 1024 || queuing)
                }.padding(12).background(.bar)
                if draft.utf8.count > 16 * 1024 { Text("This message is too long.").font(.caption).foregroundStyle(.red) }
                }
            }
            .toolbar {
                if currentChannel.canInvite { Button("Invite member", systemImage: "person.badge.plus") { inviting = true } }
                NavigationLink { ChannelSettingsView(channel: channel.id) } label: { Label("Conversation settings", systemImage: "gearshape") }
            }
            .searchable(text: Binding(get: { query }, set: { query = String($0.prefix(200)) }), prompt: "Search messages")
            .sheet(item: $editingMessage) { target in NavigationStack { MessageEditView(channel: channel.id, message: target.message) { await load() } } }
            .confirmationDialog("Withdraw this message?", isPresented: Binding(get: { retracting != nil }, set: { if !$0 { retracting = nil } })) {
                Button("Withdraw", role: .destructive) { if let message = retracting { actionTask = Task { await withdraw(message) } } }
            }
            .sheet(isPresented: $choosingAttachment) {
                NavigationStack { AttachmentPicker(folder: "root") { file in
                    choosingAttachment = false
                    attachment = file
                } }
            }
            .sheet(item: $openingFile) { file in NavigationStack { FileDetailDestination(id: file.id, version: file.current).toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { openingFile = nil } } } } }
            .sheet(isPresented: $inviting) { NavigationStack { ChannelInvitationView(channel: channel.id) } }
            .sheet(isPresented: $sharingSecret) { NavigationStack { SecretSnapshotItemPicker(channel: channel.id) } }
            .sheet(item: $secretPackage) { source in NavigationStack { SecretPackageImportView(source: source) } }
            .task(id: "participants:\(model.epoch):\(model.profileRevision):\(participantIDs.joined(separator: ","))") { await loadParticipants() }
            .onChange(of: model.filesRevision) { _, _ in receivedFiles = [:] }
            .task(id: "\(channel.id):\(model.contentRevision):\(model.filesRevision):\(model.epoch):\(query)") {
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

    private var dayStarts: Set<String> {
        var result: Set<String> = [], previous: Date?
        for message in shownMessages {
            let day = Calendar.current.startOfDay(for: Date(timeIntervalSince1970: Double(message.createdAtMs) / 1000))
            if day != previous { result.insert(message.id) }; previous = day
        }
        return result
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
        query = ""; await load()
        while !Task.isCancelled && !messages.contains(where: { $0.sequence == sequence }) && hasEarlier {
            let before = nextBefore; await loadEarlier(); if nextBefore == before { break }
        }
        if let message = messages.first(where: { $0.sequence == sequence }) { proxy.scrollTo(message.id, anchor: .center) }
        else if !Task.isCancelled { model.notice = String(localized: "The original message is not available on this device.") }
    }

    private func messageBubble(_ message: MessageItem, reveal: @escaping (UInt64) -> Void) -> some View {
        ConversationMessageRow(own: message.account == model.session?.account, name: participantName(message.account), avatar: participantAvatar(message.account)) {
            VStack(alignment: .leading, spacing: 8) {
                Text(participantName(message.account)).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                if message.hidden {
                    Text("Hidden message").foregroundStyle(.secondary)
                    Button("Show") { actionTask = Task { await hide(message, hidden: false) } }
                } else {
                    Text(message.retracted ? String(localized: "Message withdrawn") : message.unreadable ? String(localized: "This message could not be verified.") : message.text).textSelection(.enabled)
                    if let reference = message.replySequence {
                        Button { reveal(reference) } label: {
                            VStack(alignment: .leading, spacing: 3) {
                                if let original = messages.first(where: { $0.sequence == reference }) {
                                    Text(participantName(original.account)).font(.caption.bold())
                                    Text(original.hidden ? String(localized: "Hidden message") : original.retracted ? String(localized: "Message withdrawn") : original.unreadable ? String(localized: "This message could not be verified.") : original.text.isEmpty ? original.attachmentName ?? "" : original.text).font(.caption).lineLimit(2)
                                } else { Text("Reply to message \(reference)").font(.caption) }
                            }.padding(8).frame(maxWidth: .infinity, alignment: .leading).background(Theme.forest.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
                        }.buttonStyle(.plain).accessibilityLabel("Open original message")
                    }
                    if message.edited { Text("Edited").font(.caption).foregroundStyle(.secondary) }
                    if let name = message.attachmentName {
                        Button {
                            if message.attachmentMime == "application/vnd.dmsg.secret-package" {
                                secretPackage = .init(channel: channel.id, sequence: message.sequence)
                            } else if let saved = receivedFiles[message.sequence] { openingFile = saved }
                            else { actionTask = Task { await receiveAttachment(message) } }
                        } label: {
                            VStack(alignment: .leading, spacing: 4) {
                                Label(name, systemImage: message.attachmentMime == "application/vnd.dmsg.secret-package" ? "lock.doc" : "doc.badge.arrow.down")
                                if let size = message.attachmentSize { Text(binarySize(size)).font(.caption).foregroundStyle(.secondary) }
                                if receivedFiles[message.sequence] != nil { Text("On this device").font(.caption) }
                                if receiving == message.sequence { ProgressView() }
                                if message.attachmentMime == "application/vnd.dmsg.secret-package" { Text("Preview and choose what to import").font(.caption) }
                            }.font(.subheadline).padding(10)
                        }.disabled(message.unreadable || message.attachmentDeleted || receiving != nil)
                            .background(Theme.forest.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
                    }
                    HStack {
                        Text(Date(timeIntervalSince1970: Double(message.createdAtMs) / 1000), style: .time)
                        if message.account == model.session?.account { Label("Accepted by service", systemImage: "checkmark") }
                    }.font(.caption2).foregroundStyle(.secondary)
                }
            }.accessibilityIdentifier("message.content.\(message.sequence)")
        }
    }

    private func participantName(_ account: String) -> String {
        participants[account]?.name ?? String(account.prefix(8))
    }

    private func participantAvatar(_ account: String) -> UIImage? {
        guard let url = participants[account]?.avatarUrl else { return nil }
        return avatars[url]
    }

    private func loadParticipants() async {
        guard let engine = model.engine, !model.concealed else { return }
        do { try await Task.sleep(for: .milliseconds(80)) } catch { return }
        let epoch = model.epoch, accounts = participantIDs
        for start in stride(from: 0, to: accounts.count, by: 100) {
            let batch = Array(accounts[start..<min(start + 100, accounts.count)])
            for refresh in [false, true] {
                guard epoch == model.epoch, !Task.isCancelled else { return }
                do {
                    let snapshot = try await coreOperation { try await engine.channelNames(accounts: batch, refresh: refresh, operation: $0) }
                    let names: [String: Participant] = try model.decode(snapshot)
                    guard epoch == model.epoch, !Task.isCancelled else { return }
                    participants.merge(names) { _, next in next }
                    var loadedURLs: Set<String> = []
                    for (account, profile) in names {
                        guard let url = profile.avatarUrl, loadedURLs.insert(url).inserted else { continue }
                        let bytes = try? await engine.channelAvatar(account: account, refresh: refresh)
                        let image = await AvatarImage.decode(bytes)
                        guard epoch == model.epoch, !Task.isCancelled else { return }
                        if refresh || image != nil { avatars[url] = image }
                    }
                } catch { /* Names and initials remain available when a public image is offline. */ }
            }
        }
    }

    private func hide(_ message: MessageItem, hidden: Bool) async {
        let epoch = model.epoch
        do {
            try await model.engine?.hideMessage(channel: channel.id, sequence: message.sequence, hidden: hidden)
            if epoch == model.epoch, !Task.isCancelled { await load() }
        } catch { if epoch == model.epoch { model.report(error) } }
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
            messages = fresh; hasEarlier = page.hasMore; nextBefore = page.nextBefore
            if firstUnread == nil && channel.unread > 0 {
                firstUnread = fresh.first { $0.sequence > channel.readThrough && $0.account != model.session?.account }?.sequence
            }
            loadedQuery = query; messagesKnown = true; loadFailed = false
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
            guard epoch == model.epoch, self.query == query, messageRequest == request, !Task.isCancelled else { return }; outbox = pending.items
            model.messageSending.loaded(channel: channel.id, items: fresh, since: sendRevision)
            for message in fresh where message.attachmentName != nil && !message.attachmentDeleted && !message.unreadable && receivedFiles[message.sequence] == nil {
                if let value = try? await engine.savedAttachment(channel: channel.id, sequence: message.sequence),
                   let file = try? model.decode(value) as LibraryFile?, epoch == model.epoch, !Task.isCancelled {
                    receivedFiles[message.sequence] = file
                }
            }
        } catch {
            if epoch == model.epoch, self.query == query, messageRequest == request, !Task.isCancelled {
                loadFailed = true; model.report(error)
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
        } catch { if epoch == model.epoch, messageRequest == request, !Task.isCancelled { model.report(error) } }
    }

    private func readEarlier(before: UInt64) async throws -> MessagePage {
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        if !searchQuery.isEmpty { return try await engine.searchMessagesPage(channel: channel.id, query: searchQuery, before: before) }
        return try await coreOperation { try await engine.earlierMessages(channel: channel.id, before: before, limit: 100, operation: $0) }
    }

    private func send() async {
        model.activity()
        guard !queuing, canPublish else { return }
        queuedAt = .now
        let text = draft, reply = replying, file = attachment
        let persistence = persistTask; persistence?.cancel()
        queuing = true; defer { queuing = false }
        let epoch = model.epoch
        do {
            await persistence?.value
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let id: String
            #if DEBUG
            if model.uiTesting { id = UUID().uuidString }
            else { id = try await queueText(text, reply: reply, file: file) }
            #else
            id = try await queueText(text, reply: reply, file: file)
            #endif
            guard epoch == model.epoch, !Task.isCancelled else { return }
            model.clearQueuedDraft(channel: channel.id, sent: text); replying = nil; attachment = nil
            model.deliverMessage(channel: channel.id, item: OutgoingMessage(id: id, text: text, attachmentName: file?.name, attempted: false, error: nil))
        } catch { if epoch == model.epoch, !Task.isCancelled { await load(); model.report(error) } }
    }

    private func queueText(_ text: String, reply: MessageItem?, file: LibraryFile?) async throws -> String {
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        if let file { return try await engine.queueAttachmentReply(channel: channel.id, file: file.id, text: text, sequence: reply?.sequence, digest: reply?.digest) }
        if let reply { return try await engine.queueMessageAction(channel: channel.id, action: "reply", sequence: reply.sequence, digest: reply.digest, text: text) }
        return try await engine.queueMessage(channel: channel.id, text: text)
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
