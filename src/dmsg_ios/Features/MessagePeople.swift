import SwiftUI
@preconcurrency import DMsgCore

struct MessagePerson: Decodable { let name: String?; let avatarUrl: String? }

struct MessageIdentity: View {
    let account: String
    var showID = false
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var person: MessagePerson?
    @State private var avatar: UIImage?
    var body: some View {
        let layout = typeSize.isAccessibilitySize && showID
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 10))
            : AnyLayout(HStackLayout(spacing: 10))
        layout {
            Group {
                if let avatar { Image(uiImage: avatar).resizable().scaledToFill() }
                else { Image(systemName: "person.crop.square").resizable().scaledToFit().padding(7) }
            }.frame(width: 36, height: 36).background(Theme.forest.opacity(0.08))
                .clipShape(RoundedRectangle(cornerRadius: 9)).foregroundStyle(Theme.forest).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text(person?.name ?? model.accountIdentities[account]?.name ?? account).font(.subheadline.weight(.medium))
                    .lineLimit(showID ? nil : 1)
                if showID && (person?.name != nil || model.accountIdentities[account]?.name != nil) {
                    Text(account).font(.caption.monospaced()).foregroundStyle(.secondary).environment(\.layoutDirection, .leftToRight)
                }
            }.frame(maxWidth: .infinity, alignment: .leading)
            if showID {
                if typeSize.isAccessibilitySize {
                    Button { model.clipboard.copy(account); model.notice = String(localized: "Copied") } label: { Label("Copy account ID", systemImage: "doc.on.doc") }
                        .buttonStyle(.borderless).frame(minHeight: 44)
                } else { AccountIDCopyButton(account: account) }
            }
        }.task(id: "\(account):\(model.epoch)") {
            person = nil; avatar = nil
            guard validMessageAccount(account), let engine = model.engine, !model.concealed else { return }
            let epoch = model.epoch
            for refresh in [false, true] {
                do {
                    let values: [String: MessagePerson] = try model.decode(try await coreOperation { try await engine.channelNames(accounts: [account], refresh: refresh, operation: $0) })
                    guard epoch == model.epoch, !Task.isCancelled else { return }
                    person = values[account]
                    if person?.avatarUrl != nil {
                        let bytes = try? await engine.channelAvatar(account: account, refresh: refresh)
                        let image = await AvatarImage.decode(bytes)
                        guard epoch == model.epoch, !Task.isCancelled else { return }
                        avatar = image
                    }
                } catch { /* Keep the cached identity while offline. */ }
            }
        }
    }
}

struct MessageRecipientField: View {
    let title: LocalizedStringKey
    @Binding var account: String
    @State private var choosing = false
    var body: some View {
        SettingsField(title, invalid: !account.isEmpty && !validMessageAccount(account)) {
            TextField(title, text: $account).textInputAutocapitalization(.never).autocorrectionDisabled()
                .environment(\.layoutDirection, .leftToRight).accessibilityIdentifier("message.recipient")
        }
        if !account.isEmpty && !validMessageAccount(account) { Text("Enter a valid account ID.").font(.caption).foregroundStyle(.red) }
        if validMessageAccount(account) { MessageIdentity(account: account, showID: true).id(account) }
        Button("Choose an account", systemImage: "person.crop.circle.badge.plus") { choosing = true }
        .sheet(isPresented: $choosing) { NavigationStack { MessagingAccountsPicker { account = $0 } } }
    }
}

struct MessagingAccountsPicker: View {
    let choose: (String) -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var accounts: [String] = []
    @State private var names: [String: MessagePerson] = [:]
    @State private var query = ""
    @State private var loading = true
    @State private var failed = false
    private var matches: [String] { accounts.filter { query.isEmpty || $0.localizedCaseInsensitiveContains(query) || names[$0]?.name?.localizedCaseInsensitiveContains(query) == true } }
    var body: some View {
        List {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            ForEach(matches, id: \.self) { account in
                Button { choose(account); dismiss() } label: { MessageIdentity(account: account) }
            }
            if !loading && !failed && matches.isEmpty { ContentUnavailableView("No matching accounts", systemImage: "person.crop.circle.badge.questionmark") }
        }.privateScreen().navigationTitle("Choose an account").searchable(text: $query, prompt: "Search")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }.task { await load() }
    }
    private func load() async {
        let epoch = model.epoch; loading = true; failed = false
        defer { if epoch == model.epoch { loading = false } }
        #if DEBUG
        if model.uiTesting { accounts = ["040g2082040g2082040g"]; return }
        #endif
        guard let engine = model.engine else { return }
        do {
            let values: [String] = try model.decode(try await engine.messagingAccounts())
            guard epoch == model.epoch, !Task.isCancelled else { return }; accounts = values
            for start in stride(from: 0, to: values.count, by: 100) {
                let batch = Array(values[start..<min(start + 100, values.count)])
                let cached: [String: MessagePerson] = try model.decode(try await coreOperation { try await engine.channelNames(accounts: batch, refresh: false, operation: $0) })
                guard epoch == model.epoch, !Task.isCancelled else { return }; names.merge(cached) { _, next in next }
            }
        } catch { if epoch == model.epoch, !Task.isCancelled { failed = true } }
    }
}

extension ChannelItem {
    // Presentation fixtures keep their small initializer; real metadata always
    // comes from the core's verified ledger and local send queue.
    init(id: String, name: String, unread: UInt64, draft: String, preview: String) {
        self.init(id: id, name: name, unread: unread, draft: draft, preview: preview,
                  lastAt: 0, muted: false, status: "active", canPublish: true, canInvite: true, pendingSends: 0, readThrough: 0, news: false)
    }
}
