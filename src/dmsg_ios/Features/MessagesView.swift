import SwiftUI
@preconcurrency import DMsgCore

struct MessagesView: View {
    @Environment(SessionModel.self) private var model
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
                            HStack(spacing: 12) {
                                ObjectIcon(name: "number")
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(channel.name).font(.body.weight(.medium))
                                    Text(channel.draft.isEmpty ? channel.preview : String(localized: "Draft: ") + channel.draft)
                                        .font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                                }.frame(maxWidth: .infinity, alignment: .leading)
                                if channel.unread > 0 { Text(channel.unread.formatted()).font(.caption.bold()).padding(6).background(Theme.forest.opacity(0.1), in: Circle()) }
                            }.padding(.vertical, 6)
                        }.accessibilityIdentifier("channel.row.\(channel.id)")
                    }
                    if model.channels.isEmpty { ContentUnavailableView("No conversations yet", systemImage: "bubble.left.and.bubble.right", description: Text("Your authorized channels appear after synchronization.")) }
                } else {
                    Toggle("Archived", isOn: $archived)
                    let visible = letters.filter { $0.archived == archived }
                    ForEach(visible) { letter in
                        NavigationLink(value: letter.id) { InboxRow(letter: letter) }
                    }
                    if visible.isEmpty { ContentUnavailableView("No letters yet", systemImage: "envelope") }
                }
            }.listStyle(.plain).privateScreen().navigationTitle("Messages")
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { ApprovalButton() }
                    ToolbarItem(placement: .primaryAction) {
                        if mode == 0 { Button("New conversation", systemImage: "plus") { creating = true } }
                    }
                }
                .task { await model.refresh() }
                .task(id: mode) {
                    if mode == 1 { await loadLetters(refresh: false); await loadLetters(refresh: true) }
                }
                .refreshable {
                    if mode == 1 { await loadLetters(refresh: true) } else { await model.synchronize() }
                }
        } detail: {
            if mode == 1, let letter = letters.first(where: { $0.id == selected }) {
                InboxDetail(letter: letter) { await loadLetters(refresh: false) }
            } else if mode == 0, let channel = model.channels.first(where: { $0.id == selected }) { ConversationView(channel: channel) }
            else { ContentUnavailableView("Select a conversation", systemImage: "bubble.left.and.bubble.right") }
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
        nonmutating set { model.drafts[channel.id] = newValue }
    }
    @State private var sending = false
    @State private var outbox: [OutgoingMessage] = []
    @State private var ownerEpoch: UInt64 = 0
    @State private var choosingAttachment = false
    @State private var inviting = false
    @State private var hasEarlier = false
    @State private var preserveMessage: String?
    @State private var persistTask: Task<Void, Never>?
    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if hasEarlier { Button("Load earlier messages") { Task { await loadEarlier() } } }
                    ForEach(messages, id: \.id) { message in
                        HStack {
                            if message.account == model.session?.account { Spacer(minLength: 36) }
                            VStack(alignment: .leading, spacing: 8) {
                                Text(message.account).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                                Text(message.unreadable ? String(localized: "This message could not be verified.") : message.text).textSelection(.enabled)
                                if let name = message.attachmentName {
                                    Button { Task { await receiveAttachment(message) } } label: { Label(name, systemImage: "doc.badge.arrow.down").font(.subheadline).padding(10) }
                                        .background(Theme.forest.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
                                }
                                Text(Date(timeIntervalSince1970: Double(message.createdAtMs) / 1000), style: .time).font(.caption2).foregroundStyle(.secondary)
                            }.padding(12).background(message.account == model.session?.account ? Theme.forest.opacity(0.12) : Theme.surface,
                                                      in: RoundedRectangle(cornerRadius: 16))
                            if message.account != model.session?.account { Spacer(minLength: 36) }
                        }.id(message.id)
                    }
                    ForEach(outbox, id: \.id) { pending in
                        VStack(alignment: .leading, spacing: 8) {
                            NoticeBar(text: String(localized: "Waiting for confirmation: ") + pending.text)
                            if let name = pending.attachmentName { Label(name, systemImage: "doc") }
                            Button(pending.attempted ? "Reconcile original send" : "Send queued message") { Task { await deliver(pending.id) } }
                                .disabled(sending)
                        }
                    }
                }.padding(16)
            }.defaultScrollAnchor(.bottom)
                .onChange(of: messages.count) { _, _ in
                    if let preserveMessage { proxy.scrollTo(preserveMessage, anchor: .top); self.preserveMessage = nil }
                    else if let id = messages.last?.id { proxy.scrollTo(id, anchor: .bottom) }
                }
        }.background(Theme.paper).navigationTitle(channel.name).navigationBarTitleDisplayMode(.inline)
            .safeAreaInset(edge: .bottom) {
                HStack(alignment: .bottom, spacing: 10) {
                    Button("Attach file", systemImage: "paperclip") { choosingAttachment = true }
                        .labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44)
                    TextField("Write a message", text: Binding(get: { draft }, set: { draft = $0 }), axis: .vertical).lineLimit(1...5)
                        .padding(12).background(Theme.surface, in: RoundedRectangle(cornerRadius: 16))
                        .accessibilityIdentifier("message.draft")
                    Button { Task { await send() } } label: { Image(systemName: "arrow.up").font(.headline).frame(width: 44, height: 44) }
                        .buttonStyle(.borderedProminent).buttonBorderShape(.circle).accessibilityLabel("Send message")
                        .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || sending)
                }.padding(12).background(.bar)
            }
            .toolbar { Button("Invite member", systemImage: "person.badge.plus") { inviting = true } }
            .sheet(isPresented: $choosingAttachment) {
                NavigationStack { AttachmentPicker(folder: "root") { file in
                    choosingAttachment = false
                    Task { await attach(file) }
                } }
            }
            .sheet(isPresented: $inviting) { NavigationStack { ChannelInvitationView(channel: channel.id) } }
            .task(id: channel.id) {
                ownerEpoch = model.epoch
                if model.drafts[channel.id] == nil { draft = channel.draft }
                await load()
            }
            .onChange(of: draft) { _, value in
                model.activity(); persistTask?.cancel()
                let epoch = model.epoch, channelID = channel.id
                persistTask = Task {
                    guard !Task.isCancelled, epoch == model.epoch else { return }
                    do { try await model.engine?.saveDraft(channel: channelID, text: value) }
                    catch { if epoch == model.epoch { model.report(error) } }
                }
            }
            .onDisappear {
                persistTask?.cancel()
                guard ownerEpoch == model.epoch else { return }
                let value = draft, channelID = channel.id
                let engine = model.engine, epoch = ownerEpoch
                Task { if epoch == model.epoch { try? await engine?.saveDraft(channel: channelID, text: value) } }
            }
            .refreshable {
                do { if let engine = model.engine { _ = try await coreOperation { try await engine.refreshChannel(channel: channel.id, operation: $0) } }; await load() }
                catch { model.report(error) }
            }
    }

    private func load() async {
        let epoch = model.epoch
        do {
            let page = try await model.engine?.messages(channel: channel.id, before: nil, limit: 100)
            guard epoch == model.epoch, !Task.isCancelled else { return }; messages = page?.items ?? []; hasEarlier = page?.hasMore ?? false
            if let last = messages.last { try await model.engine?.markRead(channel: channel.id, through: last.sequence) }
            let pending = try await model.engine?.outbox(channel: channel.id)
            guard epoch == model.epoch, !Task.isCancelled else { return }; outbox = pending?.items ?? []
        } catch { model.report(error) }
    }

    private func loadEarlier() async {
        guard let first = messages.first else { return }; let epoch = model.epoch
        do {
            guard let page = try await model.engine?.messages(channel: channel.id, before: first.sequence, limit: 100), epoch == model.epoch, !Task.isCancelled else { return }
            preserveMessage = first.id; messages = page.items + messages; hasEarlier = page.hasMore
        } catch { model.report(error) }
    }

    private func send() async {
        model.activity()
        guard let engine = model.engine else { return }
        sending = true; defer { sending = false }
        let epoch = model.epoch
        do {
            try await engine.saveDraft(channel: channel.id, text: draft)
            let id = try await engine.queueMessage(channel: channel.id, text: draft)
            guard epoch == model.epoch else { return }
            draft = ""; await load()
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
        let epoch = model.epoch
        do {
            let id = try await engine.queueAttachment(channel: channel.id, file: file.id, text: draft)
            guard epoch == model.epoch else { return }; draft = ""; await load(); await deliver(id)
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func receiveAttachment(_ message: MessageItem) async {
        guard let engine = model.engine else { return }
        do {
            _ = try await coreOperation { try await engine.saveAttachment(channel: channel.id, sequence: message.sequence, operation: $0) }
            model.notice = String(localized: "Verified attachment saved to Files.")
        } catch { model.report(error) }
    }
}
