import SwiftUI

struct InboxLetter: Decodable, Identifiable, Equatable {
    let orderId: String
    let sender: String
    let state: String
    let text: String
    let createdAt: UInt64
    var read: Bool
    var archived: Bool
    let abortReason: String?
    var id: String { orderId }

}

struct InboxRow: View {
    let letter: InboxLetter
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        let header = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 2))
            : AnyLayout(HStackLayout(alignment: .firstTextBaseline, spacing: 8))
        HStack(alignment: .top, spacing: 10) {
            Circle().fill(letter.read ? Color.clear : Theme.forest).frame(width: 6, height: 6).padding(.top, 14).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                header {
                    MessageIdentity(account: letter.sender)
                    Text(Date(timeIntervalSince1970: Double(letter.createdAt) / 1000), style: .date)
                        .font(.caption2).foregroundStyle(.secondary).fixedSize(horizontal: true, vertical: false)
                }
                Text(letter.state == "aborted" ? String(localized: "Not delivered") : letter.text)
                    .font(.caption).foregroundStyle(.secondary).lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 2)
            }
        }.accessibilityElement(children: .combine)
            .accessibilityValue(letter.read ? "" : String(localized: "Unread messages"))
    }
}

struct InboxDetail: View {
    let source: InboxLetter
    @State private var letter: InboxLetter
    let changed: () async -> Void
    @Environment(SessionModel.self) private var model
    @State private var working = false
    @State private var reporting = false
    @State private var composing = false
    @State private var inviting = false
    @State private var reopening = false
    @State private var blocking = false

    init(letter: InboxLetter, changed: @escaping () async -> Void) {
        source = letter
        _letter = State(initialValue: letter)
        self.changed = changed
    }

    var body: some View {
        List {
            Section("Sender") {
                MessageIdentity(account: letter.sender, showID: true)
                Text(Date(timeIntervalSince1970: Double(letter.createdAt) / 1000), format: .dateTime)
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                if letter.state == "aborted" {
                    NoticeBar(text: String(localized: "Not delivered"))
                    if let reason = letter.abortReason { DisclosureGroup("Details") { Text(reason).font(.caption).foregroundStyle(.secondary) } }
                } else { Text(letter.text).textSelection(.enabled).reminderVisible(.inbox(letter.id), watermark: 1) }
            }
            Section {
                SettingsAction("Reply", systemImage: "arrowshape.turn.up.left", prominent: true) { composing = true }.accessibilityIdentifier("inbox.reply")
                Button("Invite to a conversation", systemImage: "person.badge.plus") { inviting = true }
            }.disabled(working || model.blockedAccounts.contains(letter.sender))
            Section {
                Button(LocalizedStringKey(letter.archived ? "Restore" : "Archive"), systemImage: "archivebox") {
                    Task { await mark(read: letter.read, archived: !letter.archived) }
                }.disabled(working).accessibilityIdentifier("inbox.archive")
                    Button(LocalizedStringKey(letter.read ? "Mark as unread" : "Mark as read"), systemImage: "envelope.open") {
                        Task { await mark(read: !letter.read, archived: letter.archived) }
                    }.disabled(working).accessibilityIdentifier("inbox.read")
            }
            Section {
                Button("Allow another letter") { reopening = true }.disabled(working)
                Text("Archiving only organizes this letter. Allow another letter separately to reopen contact.").font(.footnote).foregroundStyle(.secondary)
                Button("Report content") { reporting = true }
                Button(LocalizedStringKey(model.blockedAccounts.contains(letter.sender) ? "Unblock" : "Block this account"), role: .destructive) { blocking = true }.disabled(working)
            }
        }.accessibilityIdentifier("inbox.detail").privateScreen().navigationTitle("Inbox").navigationBarTitleDisplayMode(.inline)
            .onChange(of: source) { _, value in letter = value }
            .sheet(isPresented: $reporting) { NavigationStack { ReportContentView(target: ReportTarget(kind: "inbox", account: letter.sender, resource: letter.id)) } }
            .sheet(isPresented: $composing) { NavigationStack { LetterComposerView(recipient: letter.sender) } }
            .sheet(isPresented: $inviting) { NavigationStack { InviteLetterSenderView(account: letter.sender) } }
            .confirmationDialog("Allow this account to send another letter?", isPresented: $reopening, titleVisibility: .visible) {
                Button("Allow another letter") { Task {
                    let epoch = model.epoch; working = true; defer { working = false }
                    do {
                        if !model.uiTesting { _ = try await model.engine?.reopenLetterContact(order: letter.id, read: letter.read, archived: letter.archived) }
                        if epoch == model.epoch { model.notice = String(localized: "Saved"); await changed() }
                    } catch { if epoch == model.epoch { model.report(error) } }
                } }
            } message: { Text("Your receiving rules still apply. This does not add a contact or a channel member.") }
            .confirmationDialog("Apply these receiving rules?", isPresented: $blocking, titleVisibility: .visible) {
                Button(LocalizedStringKey(model.blockedAccounts.contains(letter.sender) ? "Unblock" : "Block this account"), role: .destructive) { Task {
                    working = true; defer { working = false }
                    if await model.blockAccount(letter.sender, blocked: !model.blockedAccounts.contains(letter.sender)) { await changed() }
                } }
            }
    }

    private func mark(read: Bool, archived: Bool) async {
        guard !working else { return }
        let epoch = model.epoch
        working = true; model.activity(); defer { working = false }
        do {
            #if DEBUG
            if model.uiTesting { model.markInboxFixture(order: letter.id, read: read, archived: archived) }
            else {
                guard let engine = model.engine else { return }
                _ = try await engine.markLetter(order: letter.id, read: read, archived: archived)
            }
            #else
            guard let engine = model.engine else { return }
            _ = try await engine.markLetter(order: letter.id, read: read, archived: archived)
            #endif
            guard epoch == model.epoch else { return }
            letter.read = read; letter.archived = archived
            await changed()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
