import SwiftUI
@preconcurrency import DMsgCore

struct InboxSettingsView: View {
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @State private var display = DisplayLoad()
    @State private var draft = Change(mode: "closed", allow: [])
    @State private var saved = Change(mode: "closed", allow: [])
    @State private var loaded = false
    @State private var working = false
    @State private var confirming = false
    @State private var pending: Change?
    @State private var scope: UInt64 = 0
    struct Policy: Decodable { let mode: String; let allow: [String]; let capacity: UInt64 }
    struct Change: Codable, Equatable {
        var mode: String; var allow: [String]
        var normalized: Change { Change(mode: mode, allow: allow.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }) }
        var valid: Bool { Set(allow).count == allow.count && allow.allSatisfy { $0.range(of: "^[0-9a-v]{19}[0g]$", options: .regularExpression) != nil } }
    }
    struct Draft: Codable { let change: Change; let pending: Change? }

    var body: some View {
        Form {
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            Text("Choose who may send you free letters. Blocked accounts remain blocked until you unblock them.").font(.footnote).foregroundStyle(.secondary)
            Section("Receiving policy") {
                Picker("Who may contact you", selection: $draft.mode) {
                    Text("Closed").tag("closed")
                    Text("Selected accounts").tag("contacts")
                    Text("Anyone, free of charge").tag("free")
                    if draft.mode == "paid" { Text("Current policy requires payment").tag("paid") }
                }
            }.disabled(!loaded || working || pending != nil)
            Section("Selected accounts") {
                ForEach(draft.allow.indices, id: \.self) { index in
                    VStack(alignment: .leading, spacing: 6) {
                        SettingsField("Account ID") { TextField("Account ID", text: $draft.allow[index]).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
                        if let name = model.accountIdentities[draft.allow[index]]?.name { Text(name).font(.caption) }
                        if !Change(mode: draft.mode, allow: [draft.normalized.allow[index]]).valid { Text("Enter a valid account ID.").font(.caption).foregroundStyle(.red) }
                        Button("Remove", role: .destructive) { draft.allow.remove(at: index) }.buttonStyle(.borderless)
                    }
                }
                SettingsAction("Add account") { draft.allow.append("") }
                if !draft.normalized.valid { Text("Check account IDs and remove duplicates.").font(.footnote).foregroundStyle(.red) }
            }.disabled(!loaded || working || pending != nil)
            NavigationLink("Blocked accounts") { BlockedAccountsView() }
            Text("Existing block rules, invitations and inbox capacity are preserved. Choosing a free or closed policy replaces any previous paid contact setting.").font(.footnote).foregroundStyle(.secondary)
            if pending != nil {
                SettingsAction("Retry original change", prominent: true) { Task { if let pending { await save(pending) } } }.disabled(working)
            } else {
                SettingsAction("Save receiving policy", prominent: true) { confirming = true }.disabled(!loaded || working || draft.mode == "paid" || !draft.normalized.valid || draft.normalized == saved)
            }
            if !loaded && !working { SettingsAction("Retry") { Task { await load(force: true) } } }
            if working { ProgressView() }
        }.privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle("Inbox settings").confirmLeavingDraft(draft != saved)
            .task { scope = model.viewGeneration; await load() }
            .onDisappear { if draft != saved || pending != nil { model.keepFormDraft("inbox", value: Draft(change: draft, pending: pending), generation: scope) } }
            .confirmationDialog("Apply these receiving rules?", isPresented: $confirming) {
                Button("Save settings") { Task { await save(draft.normalized) } }
            }
    }

    private func load(force: Bool = false) async {
        guard !working else { return }
        let editing = loaded && draft != saved
        var shown = draft
        await display.load(model, kind: "inbox-policy", force: force) { (next: Policy) in
            let apply = !editing && pending == nil && draft == shown
            saved = Change(mode: next.mode, allow: next.allow)
            if apply { draft = saved; shown = draft }
            loaded = true
        }
        if let restored = model.formDraft("inbox", as: Draft.self) { draft = restored.change; pending = restored.pending; model.settingsDrafts.removeValue(forKey: "inbox") }
        #if DEBUG
        if model.uiTesting { loaded = true }
        #endif
    }

    private func save(_ change: Change) async {
        guard let engine = model.engine, !working, change.valid else { return }; let epoch = model.epoch
        working = true; pending = change; model.activity(); defer { working = false }
        do {
            _ = try await coreOperation { try await engine.configureInbox(mode: change.mode, allow: change.allow, operation: $0) }
            guard epoch == model.epoch && visible else { return }
            pending = nil; draft = change; saved = change; model.settingsDrafts.removeValue(forKey: "inbox"); model.notice = String(localized: "Saved")
        } catch {
            if epoch == model.epoch && visible {
                if case MobileError.Failure(let code) = error, ["INVALID_INPUT","FORBIDDEN","QUOTA_EXCEEDED","VERSION_CONFLICT","IDEMPOTENCY_CONFLICT"].contains(code) { pending = nil }
                model.report(error)
            }
        }
    }
}
