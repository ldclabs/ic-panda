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
            ObjectIcon(name: letter.read ? "envelope.open" : "envelope.badge")
            VStack(alignment: .leading, spacing: 2) {
                header {
                    Text(letter.sender).font(.subheadline.weight(letter.read ? .regular : .semibold))
                        .lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 1).environment(\.layoutDirection, .leftToRight)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text(Date(timeIntervalSince1970: Double(letter.createdAt) / 1000), style: .date)
                        .font(.caption2).foregroundStyle(.secondary).fixedSize(horizontal: true, vertical: false)
                }
                Text(letter.state == "aborted" ? String(localized: "This letter was withdrawn.") : letter.text)
                    .font(.caption).foregroundStyle(.secondary).lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 2)
            }
        }
    }
}

struct InboxDetail: View {
    let source: InboxLetter
    @State private var letter: InboxLetter
    let changed: () async -> Void
    @Environment(SessionModel.self) private var model
    @State private var working = false
    @State private var reporting = false

    init(letter: InboxLetter, changed: @escaping () async -> Void) {
        source = letter
        _letter = State(initialValue: letter)
        self.changed = changed
    }

    var body: some View {
        List {
            Section("Sender") {
                Text(letter.sender).font(.subheadline.monospaced()).textSelection(.enabled)
                    .environment(\.layoutDirection, .leftToRight)
                Text(Date(timeIntervalSince1970: Double(letter.createdAt) / 1000), format: .dateTime)
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                if letter.state == "aborted" {
                    NoticeBar(text: String(localized: "This letter was withdrawn."))
                    if let reason = letter.abortReason { Text(reason).font(.caption).foregroundStyle(.secondary) }
                } else { Text(letter.text).textSelection(.enabled).reminderVisible(.inbox(letter.id), watermark: 1) }
            }
            Section {
                Button("Report content") { reporting = true }
                Button("Block this account", role: .destructive) { Task { await model.blockAccount(letter.sender, blocked: true) } }
                Button(LocalizedStringKey(letter.archived ? "Restore" : "Archive"), systemImage: "archivebox") {
                    Task { await mark(read: letter.read, archived: !letter.archived) }
                }.disabled(working).accessibilityIdentifier("inbox.archive")
                if !letter.read {
                    Button("Mark as read", systemImage: "envelope.open") {
                        Task { await mark(read: true, archived: letter.archived) }
                    }.disabled(working).accessibilityIdentifier("inbox.read")
                }
            }
        }.privateScreen().navigationTitle("Inbox").navigationBarTitleDisplayMode(.inline)
            .onChange(of: source) { _, value in letter = value }
            .sheet(isPresented: $reporting) { NavigationStack { ReportContentView(target: ReportTarget(kind: "inbox", account: letter.sender, resource: letter.id)) } }
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
