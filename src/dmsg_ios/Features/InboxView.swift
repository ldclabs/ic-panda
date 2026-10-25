import SwiftUI

struct InboxLetter: Decodable, Identifiable {
    let orderId: String
    let sender: String
    let state: String
    let text: String
    let createdAt: UInt64
    let read: Bool
    let archived: Bool
    let abortReason: String?
    var id: String { orderId }

}

struct InboxRow: View {
    let letter: InboxLetter
    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            ObjectIcon(name: letter.read ? "envelope.open" : "envelope.badge")
            VStack(alignment: .leading, spacing: 4) {
                Text(letter.sender).font(.subheadline.weight(letter.read ? .regular : .semibold))
                    .lineLimit(1).environment(\.layoutDirection, .leftToRight)
                Text(letter.state == "aborted" ? String(localized: "This letter was withdrawn.") : letter.text)
                    .font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                Text(Date(timeIntervalSince1970: Double(letter.createdAt) / 1000), style: .date)
                    .font(.caption).foregroundStyle(.secondary)
            }
        }.padding(.vertical, 6)
    }
}

struct InboxDetail: View {
    let letter: InboxLetter
    let changed: () async -> Void
    @Environment(SessionModel.self) private var model
    @State private var working = false

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
                } else { Text(letter.text).textSelection(.enabled) }
            }
            Section {
                Button(letter.archived ? "Restore" : "Archive", systemImage: "archivebox") {
                    Task { await mark(read: letter.read, archived: !letter.archived) }
                }.disabled(working)
                if !letter.read {
                    Button("Mark as read", systemImage: "envelope.open") {
                        Task { await mark(read: true, archived: letter.archived) }
                    }.disabled(working)
                }
            }
        }.privateScreen().navigationTitle("Inbox").navigationBarTitleDisplayMode(.inline)
    }

    private func mark(read: Bool, archived: Bool) async {
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch
        working = true; model.activity(); defer { working = false }
        do {
            _ = try await engine.markLetter(order: letter.id, read: read, archived: archived)
            guard epoch == model.epoch else { return }
            await changed()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
