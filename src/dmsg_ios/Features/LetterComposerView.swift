import SwiftUI

struct LetterComposerView: View {
    @Environment(SessionModel.self) private var model
    @State private var recipient = ""
    @State private var text = ""
    @State private var invitation = ""
    @State private var id = NativeApple.requestID()
    @State private var prepared = false
    @State private var working = false
    @State private var result: DisplayValue?
    @State private var pending: [Pending] = []
    struct Pending: Decodable, Identifiable { let id: String; let recipient: String }
    var body: some View {
        Form {
            Section("New free letter") {
                TextField("Recipient account ID", text: $recipient).textInputAutocapitalization(.never).autocorrectionDisabled()
                TextField("Contact invitation, if provided", text: $invitation).textInputAutocapitalization(.never).autocorrectionDisabled()
                TextField("Message", text: $text, axis: .vertical).lineLimit(5...12)
            }.disabled(prepared || working)
            NoticeBar(text: String(localized: "Only free contact is supported here. This action cannot pay or approve a recipient's fee."))
            Button(prepared ? "Retry original letter" : "Send free letter") { Task { await send() } }
                .disabled(working || result != nil || recipient.count != 20 || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            if let result { CanonicalDetails(value: result) }
            if !pending.isEmpty {
                Section("Pending letters") {
                    ForEach(pending) { letter in
                        VStack(alignment: .leading, spacing: 6) {
                            FingerprintRow(title: "Recipient", value: letter.recipient)
                            Button("Retry original letter") { Task {
                                do { if let engine = model.engine { _ = try await coreOperation { try await engine.sendFreeLetter(id: letter.id, operation: $0) } }; await load() }
                                catch { model.report(error) }
                            } }.disabled(working)
                        }
                    }
                }
            }
        }.privateScreen().navigationTitle("Write a letter").task { await load() }
    }
    private func send() async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }; model.activity()
        do {
            if !prepared { try await engine.queueFreeLetter(id: id, recipient: recipient, text: text, invitation: invitation.isEmpty ? nil : invitation); prepared = true }
            result = try model.decode(try await coreOperation { try await engine.sendFreeLetter(id: id, operation: $0) })
            await load()
        } catch { model.report(error) }
    }
    private func load() async { do { if let value = try await model.engine?.pendingLetters() { pending = try model.decode(value) } } catch { model.report(error) } }
}
