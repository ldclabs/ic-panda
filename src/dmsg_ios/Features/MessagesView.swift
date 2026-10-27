import SwiftUI
@preconcurrency import DMsgCore

struct MessagesView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var selected: String?
    @State private var mode = 0
    @State private var creating = false
    @State private var letters: [InboxLetter] = []
    @State private var archived = false
    @State private var columns: NavigationSplitViewVisibility = .all
    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            List(selection: $selected) {
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
                    Toggle("Archived", isOn: $archived)
                    let visible = letters.filter { $0.archived == archived && !model.blockedAccounts.contains($0.sender) }
                    ForEach(visible) { letter in
                        NavigationLink(value: letter.id) { InboxRow(letter: letter) }.compactObjectRow()
                    }
                    if visible.isEmpty { ContentUnavailableView("No letters yet", image: AppGlyph.inbox.assetName) }
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
                    if mode == 1 { await loadLetters(refresh: false); await loadLetters(refresh: true) }
                }
                .refreshable {
                    if mode == 1 { await loadLetters(refresh: true) } else { await model.synchronize() }
                }
        } detail: {
            if mode == 1, let letter = letters.first(where: { $0.id == selected }) {
                InboxDetail(letter: letter) { await loadLetters(refresh: false) }
            } else if mode == 0, let channel = model.channels.first(where: { $0.id == selected }) { ConversationView(channel: channel).id(channel.id) }
            else { ContentUnavailableView("Select a conversation", image: AppGlyph.channels.assetName) }
        }
        .navigationSplitViewStyle(.balanced)
        .onAppear { if selected == nil { columns = .all } }
        .onChange(of: selected) { _, value in if value != nil { columns = .automatic } }
        .sheet(isPresented: $creating) { NavigationStack { ConversationSetupView() } }
    }

    private func loadLetters(refresh: Bool) async {
        let epoch = model.epoch
        do {
            guard let snapshot = try await model.engine?.inbox(refresh: refresh),
                  epoch == model.epoch, !Task.isCancelled else { return }
            let incoming: [InboxLetter] = try model.decode(snapshot)
            letters = incoming.sorted { $0.createdAt > $1.createdAt }
        } catch { if epoch == model.epoch { model.report(error) } }
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
    @State private var sending = false
    @State private var outbox: [OutgoingMessage] = []
    @State private var ownerEpoch: UInt64 = 0
    @State private var choosingAttachment = false
    @State private var inviting = false
    @State private var sharingSecret = false
    @State private var secretPackage: SecretPackageImportView.ChannelSource?
    @State private var hasEarlier = false
    @State private var preserveMessage: String?
    @State private var nearBottom = true
    @State private var loadedQuery = ""
    @State private var messageRequest: UUID?
    @State private var persistTask: Task<Void, Never>?
    @State private var actionTask: Task<Void, Never>?
    @State private var loadingEarlier = false
    @State private var query = ""
    @State private var replying: MessageItem?
    @State private var editingMessage: MessageEdit?
    @State private var retracting: MessageItem?
    @State private var reporting: ReportTarget?
    struct MessageEdit: Identifiable { let message: MessageItem; var id: String { message.id } }
    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if hasEarlier && query.isEmpty { Button("Load earlier messages") { actionTask = Task { await loadEarlier() } }.disabled(loadingEarlier) }
                    ForEach(messages.filter { !model.blockedAccounts.contains($0.account) }, id: \.id) { message in
                        messageBubble(message)                            .id(message.id).contextMenu {
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
                    ForEach(outbox, id: \.id) { pending in
                        VStack(alignment: .leading, spacing: 8) {
                            NoticeBar(text: String(localized: "Waiting for confirmation: ") + pending.text)
                            if let name = pending.attachmentName { Label(name, systemImage: "doc") }
                            Button(pending.attempted ? "Reconcile original send" : "Send queued message") { actionTask = Task { await deliver(pending.id) } }
                                .disabled(sending)
                        }
                    }
                }.padding(16)
            }.defaultScrollAnchor(.bottom)
                .onScrollGeometryChange(for: Bool.self) { geometry in
                    geometry.contentOffset.y + geometry.containerSize.height >= geometry.contentSize.height - 100
                } action: { _, value in nearBottom = value }
                .onChange(of: messages.count) { _, _ in
                    if let preserveMessage { proxy.scrollTo(preserveMessage, anchor: .top); self.preserveMessage = nil }
                    else if nearBottom, let id = messages.last?.id { proxy.scrollTo(id, anchor: .bottom) }
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
                        .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || sending)
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
            .refreshable {
                do { if let engine = model.engine { _ = try await coreOperation { try await engine.refreshChannel(channel: channel.id, operation: $0) } }; await load() }
                catch { model.report(error) }
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

    private func load() async {
        guard let engine = model.engine else { return }
        guard !loadingEarlier || query != loadedQuery else { return }
        let epoch = model.epoch, query = query, request = UUID()
        messageRequest = request
        let earlier = query.isEmpty && loadedQuery.isEmpty ? messages.first?.sequence : nil
        do {
            var page = query.isEmpty ? try await engine.messages(channel: channel.id, before: nil, limit: 100) : try await engine.searchMessages(channel: channel.id, query: query)
            var fresh = page.items
            // Refresh the already visible history too, preserving its extent.
            // Each local batch yields to lock/cancellation; no full-history read.
            if let earlier {
                while page.hasMore, let first = fresh.first, first.sequence > earlier {
                    guard epoch == model.epoch, !Task.isCancelled else { return }
                    page = try await engine.messages(channel: channel.id, before: first.sequence, limit: 100)
                    guard !page.items.isEmpty else { break }
                    fresh = page.items + fresh
                }
            }
            guard epoch == model.epoch, self.query == query, messageRequest == request, !Task.isCancelled else { return }
            messages = fresh; hasEarlier = page.hasMore; loadedQuery = query
            if query.isEmpty, let last = fresh.last { try await engine.markRead(channel: channel.id, through: last.sequence) }
            let pending = try await engine.outbox(channel: channel.id)
            guard epoch == model.epoch, self.query == query, messageRequest == request, !Task.isCancelled else { return }; outbox = pending.items
        } catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
    }

    private func loadEarlier() async {
        guard let first = messages.first, !loadingEarlier, query.isEmpty else { return }; let epoch = model.epoch, request = UUID()
        messageRequest = request
        loadingEarlier = true; defer { loadingEarlier = false }
        do {
            guard let page = try await model.engine?.messages(channel: channel.id, before: first.sequence, limit: 100), epoch == model.epoch, messageRequest == request, !Task.isCancelled else { return }
            preserveMessage = first.id; messages = page.items + messages; hasEarlier = page.hasMore
        } catch { model.report(error) }
    }

    private func send() async {
        model.activity()
        guard let engine = model.engine, !sending else { return }
        let text = draft, reply = replying
        let persistence = persistTask; persistence?.cancel()
        sending = true; defer { sending = false }
        let epoch = model.epoch
        do {
            await persistence?.value
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let id: String
            if let reply { id = try await engine.queueMessageAction(channel: channel.id, action: "reply", sequence: reply.sequence, digest: reply.digest, text: text) }
            else { id = try await engine.queueMessage(channel: channel.id, text: text) }
            guard epoch == model.epoch, !Task.isCancelled else { return }
            model.clearQueuedDraft(channel: channel.id, sent: text); replying = nil; await load()
            _ = try await coreOperation { try await engine.sendMessage(channel: channel.id, messageId: id, operation: $0) }
            if epoch == model.epoch { await load() }
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func deliver(_ id: String) async {
        guard let engine = model.engine, !sending else { return }
        sending = true; defer { sending = false }
        do { _ = try await coreOperation { try await engine.sendMessage(channel: channel.id, messageId: id, operation: $0) }; await load() }
        catch { model.report(error); await load() }
    }

    private func attach(_ file: LibraryFile) async {
        guard let engine = model.engine else { return }
        let epoch = model.epoch, text = draft
        let persistence = persistTask; persistence?.cancel()
        do {
            await persistence?.value
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let id = try await engine.queueAttachment(channel: channel.id, file: file.id, text: text)
            guard epoch == model.epoch, !Task.isCancelled else { return }; model.clearQueuedDraft(channel: channel.id, sent: text); await load(); await deliver(id)
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func withdraw(_ message: MessageItem) async {
        guard let engine = model.engine else { return }; let epoch = model.epoch
        retracting = nil
        do {
            let id = try await engine.queueMessageAction(channel: channel.id, action: "retract", sequence: message.sequence, digest: message.digest, text: "")
            guard epoch == model.epoch else { return }; await deliver(id)
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
            _ = try await coreOperation { try await engine.sendMessage(channel: channel, messageId: id, operation: $0) }
            guard epoch == model.epoch else { return }; await changed(); dismiss()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
