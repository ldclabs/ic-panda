import SwiftUI
@preconcurrency import DMsgCore

struct MessagePerson: Decodable, Equatable { let name: String?; let avatarUrl: String? }

/// A person is round and a conversation is a rounded square, so people never look like
/// objects. Without a picture the first letter sits on one of three tones chosen from
/// `seed`, by the same rule as Android's Avatar, so an account keeps its tone on both.
struct Avatar: View {
    enum Kind { case person, conversation }
    private static let tones = [(Color("AvatarForest"), Color("AvatarForestInk")), (Color("AvatarSage"), Color("AvatarSageInk")),
                                (Color("AvatarAmber"), Color("AvatarAmberInk"))]
    let label: String
    let seed: String
    var image: UIImage?
    var kind: Kind
    @ScaledMetric private var size: CGFloat

    init(_ label: String, seed: String, image: UIImage? = nil, kind: Kind = .person, size: CGFloat = 40) {
        self.label = label; self.seed = seed; self.image = image; self.kind = kind
        _size = ScaledMetric(wrappedValue: size, relativeTo: .body)
    }

    var body: some View {
        let tone = Self.tone(seed)
        Group {
            if let image { Image(uiImage: image).resizable().scaledToFill() }
            else {
                Text(label.trimmingCharacters(in: .whitespacesAndNewlines).first.map { String($0).uppercased() } ?? "?")
                    .font(size >= 40 ? .headline : .footnote.weight(.semibold)).foregroundStyle(tone.1)
            }
        }.frame(width: size, height: size).background(tone.0)
            // A picture that arrives after the initial fades in over it; one already loaded shows at once.
            .motion(Motion.fade, reduced: Motion.fade, value: image != nil)
            .clipShape(kind == .person ? AnyShape(Circle()) : AnyShape(RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)))
            .accessibilityHidden(true)
    }

    /// Java's String.hashCode over UTF-16, as `Math.floorMod(seed.hashCode(), 3)` on Android.
    static func tone(_ seed: String) -> (Color, Color) {
        var hash: Int32 = 0
        for unit in seed.utf16 { hash = hash &* 31 &+ Int32(unit) }
        return tones[Int((hash % 3 + 3) % 3)]
    }
}

extension View {
    /// Lazy rows request their displayed identity when they appear or the workspace resumes.
    func loadMessagePerson(_ account: String?, model: SessionModel) -> some View {
        task(id: "\(account ?? ""):\(model.profileRevision):\(model.concealed)") {
            if let account, !model.concealed { model.people.request([account], model: model) }
        }
    }
}

struct MessageIdentity: View {
    let account: String
    var showID = false
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var typeSize
    private var name: String? { model.people.people[account]?.name ?? model.accountIdentities[account]?.name }
    var body: some View {
        let layout = typeSize.isAccessibilitySize && showID
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 10))
            : AnyLayout(HStackLayout(spacing: 10))
        layout {
            Avatar(name ?? account, seed: account, image: model.people.avatar(account), size: 36)
            VStack(alignment: .leading, spacing: 3) {
                Text(name ?? account).font(.subheadline.weight(.medium))
                    .lineLimit(showID ? nil : 1)
                if showID && name != nil {
                    Text(account).font(.footnote.monospaced()).foregroundStyle(.secondary).environment(\.layoutDirection, .leftToRight)
                }
            }.frame(maxWidth: .infinity, alignment: .leading)
            if showID {
                if typeSize.isAccessibilitySize {
                    Button { model.clipboard.copy(account); model.notice = String(localized: "Copied") } label: { Label("Copy account ID", systemImage: "doc.on.doc") }
                        .buttonStyle(.borderless).frame(minHeight: 44)
                } else { IDCopyButton(value: account, label: "Copy account ID") }
            }
        }.loadMessagePerson(account, model: model)
    }
}

struct MessageAccountInput: View {
    let title: LocalizedStringKey
    @Binding var account: String
    var identifier = "message.recipient"
    var required = false
    @FocusState private var focused: Bool
    @State private var leftField = false
    private var invalid: Bool {
        if account.isEmpty { return required && !focused }
        return !validMessageAccount(account) && (account.utf8.count >= 20 || leftField && !focused)
    }

    var body: some View {
        SettingsField(title, invalid: invalid) {
            TextField(title, text: Binding(get: { account }, set: { account = $0.trimmingCharacters(in: .whitespacesAndNewlines) }))
                .textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight)
                .accessibilityIdentifier(identifier).focused($focused).onChange(of: focused) { wasFocused, nowFocused in
                    if wasFocused && !nowFocused { leftField = true }
                }
        }
        if invalid { Text("Enter a valid account ID.").font(.footnote).foregroundStyle(.red) }
    }
}

struct MessageRecipientField: View {
    let title: LocalizedStringKey
    @Binding var account: String
    @State private var choosing = false

    var body: some View {
        MessageAccountInput(title: title, account: $account)
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
    @State private var query = ""
    @State private var loading = true
    @State private var failed = false
    private var matches: [String] { accounts.filter { query.isEmpty || $0.localizedCaseInsensitiveContains(query) || model.people.people[$0]?.name?.localizedCaseInsensitiveContains(query) == true } }
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
            model.people.request(values, model: model)
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
