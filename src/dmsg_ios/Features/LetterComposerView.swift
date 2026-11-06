import SwiftUI
@preconcurrency import DMsgCore

struct LetterComposerView: View {
    let initial: LetterDraft
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var discarding = false
    @State private var leaving = false
    @State private var showsInvitation = false

    init(recipient: String = "", draft: LetterDraft? = nil) { initial = draft ?? LetterDraft(recipient: recipient) }
    private var draft: LetterDraft { model.letters.drafts[initial.id] ?? initial }
    private var working: Bool { model.letters.working.contains(initial.id) }
    private var queued: SentLetter? { model.letters.history.first { $0.id == initial.id } }
    private func binding(_ key: WritableKeyPath<LetterDraft, String>) -> Binding<String> {
        Binding(get: { draft[keyPath: key] }, set: { value in var next = draft; next[keyPath: key] = value; model.letters.update(next, model: model) })
    }

    var body: some View {
        Group {
            if let queued { SentLetterContent(source: queued) }
            else {
                Form {
                    Section {
                        MessageRecipientField(title: "Recipient account ID", account: binding(\.recipient))
                        SettingsField("Message", counter: "\(draft.text.utf8.count) / 6000", invalid: draft.text.utf8.count > 6000) {
                            TextField("Message", text: binding(\.text), axis: .vertical).lineLimit(5...12).accessibilityIdentifier("letter.text")
                        }
                        if draft.text.utf8.count > 6000 { Text("This letter is too long.").font(.caption).foregroundStyle(.red) }
                        DisclosureGroup("Contact invitation, if provided", isExpanded: Binding(get: { showsInvitation || draft.invitation != nil }, set: { showsInvitation = $0 })) {
                            TextField("Contact invitation, if provided", text: Binding(get: { draft.invitation ?? "" }, set: { value in
                                var next = draft; next.invitation = value.isEmpty ? nil : value; model.letters.update(next, model: model)
                            })).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight)
                            if !draft.invitationValid { Text("The invitation is invalid.").font(.caption).foregroundStyle(.red) }
                        }
                    } footer: {
                        Text("Only free contact is supported here. This action cannot pay or approve a recipient's fee.")
                    }.disabled(working)
                    if let error = model.letters.failures[initial.id] { NoticeBar(text: error) }
                    if working { ProgressView("Sending") }
                    if !draft.empty {
                        Text(model.letters.dirty.contains(initial.id) ? "Saving draft…" : "Draft saved").font(.caption).foregroundStyle(.secondary)
                        SettingsAction("Discard draft", role: .destructive) { discarding = true }.disabled(working)
                    }
                }
            }
        }.privateScreen().navigationTitle("Write a letter").navigationBarTitleDisplayMode(.inline)
            .navigationBarBackButtonHidden(true)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Done") { Task {
                    leaving = true; await model.letters.flush(model); leaving = false
                    if !model.letters.dirty.contains(initial.id) { dismiss() }
                } }.disabled(leaving) }
                if queued == nil {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Send") { model.letters.send(draft, id: draft.id, model: model) }
                            .disabled(!draft.valid || working).accessibilityIdentifier("letter.send")
                    }
                }
            }
            .interactiveDismissDisabled(model.letters.dirty.contains(initial.id))
            .task { if model.letters.drafts[initial.id] == nil && !initial.empty && queued == nil { model.letters.update(initial, model: model) } }
            .confirmationDialog("Discard this draft?", isPresented: $discarding, titleVisibility: .visible) {
                Button("Discard draft", role: .destructive) { Task { if await model.letters.discard(initial.id, model: model) { dismiss() } } }
            }
    }
}

struct SentLetterDetail: View {
    let source: SentLetter
    var body: some View { SentLetterContent(source: source).privateScreen().navigationTitle("Sent letters").navigationBarTitleDisplayMode(.inline) }
}

struct SentLetterContent: View {
    let source: SentLetter
    @Environment(SessionModel.self) private var model
    @State private var editing: LetterDraft?
    private var letter: SentLetter { model.letters.history.first { $0.id == source.id } ?? source }
    private var working: Bool { model.letters.working.contains(letter.id) }
    var body: some View {
        List {
            Section("Recipient") {
                MessageIdentity(account: letter.recipient, showID: true)
                if let date = letter.createdAt { Text(Date(timeIntervalSince1970: Double(date) / 1000), format: .dateTime).font(.caption).foregroundStyle(.secondary) }
            }
            Section {
                if let text = letter.text { Text(text).textSelection(.enabled) }
                else { Text("The original text is not available on this device.").foregroundStyle(.secondary) }
            }
            Section("Status") {
                Label(letterStateTitle(letter.state), systemImage: letter.state == "visible" ? "checkmark.circle" : "clock")
                if letter.state == "visible" { Text("Service acceptance does not mean the recipient has read your letter.").font(.footnote).foregroundStyle(.secondary) }
                if let error = model.letters.failures[letter.id] { NoticeBar(text: error) }
                else if let error = letter.error { NoticeBar(text: InboxLoading.message(DMsgCore.MobileError.Failure(code: error))) }
                if working { ProgressView() }
                if letter.pending {
                    SettingsAction("Retry original letter", prominent: true) { model.letters.send(nil, id: letter.id, model: model) }.disabled(working)
                    if letter.editable { SettingsAction("Edit draft") { Task { editing = await model.letters.edit(letter.id, model: model) } }.disabled(working) }
                } else { SettingsAction("Check delivery status") { Task { await model.letters.check(letter.id, model: model) } }.disabled(working) }
            }
            DisclosureGroup("Details") { Text(letter.id).font(.caption.monospaced()).textSelection(.enabled).environment(\.layoutDirection, .leftToRight) }
        }.sheet(item: $editing) { draft in NavigationStack { LetterComposerView(draft: draft) } }
    }
}
