import SwiftUI
@preconcurrency import DMsgCore

struct InboxSettingsView: View {
    @Environment(SessionModel.self) private var model
    @State private var mode = "closed"
    @State private var allowed = ""
    @State private var loaded = false
    @State private var working = false
    @State private var confirming = false
    @State private var pending: Change?
    struct Policy: Decodable { let mode: String; let allow: [String]; let capacity: UInt64 }
    struct Change { let mode: String; let allow: [String] }
    var body: some View {
        Form {
            Text("Choose who may send you free letters. Blocked accounts remain blocked until you unblock them.").font(.footnote).foregroundStyle(.secondary)
            Section("Receiving policy") {
                Picker("Who may contact you", selection: $mode) {
                    Text("Closed").tag("closed")
                    Text("Selected accounts").tag("contacts")
                    Text("Anyone, free of charge").tag("free")
                    if mode == "paid" { Text("Current policy requires payment").tag("paid") }
                }
                TextField("Allowed account IDs, one per line", text: $allowed, axis: .vertical).lineLimit(3...8)
                    .textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight)
            }.disabled(!loaded || working || pending != nil)
            Text("Existing block rules, invitations and inbox capacity are preserved. Choosing a free or closed policy replaces any previous paid contact setting.").font(.footnote).foregroundStyle(.secondary)
            if pending != nil {
                Button("Retry original change") { Task { if let pending { await save(pending) } } }.disabled(working)
            } else {
                Button("Save receiving policy") { confirming = true }.disabled(!loaded || working || mode == "paid")
            }
            if !loaded && !working { Button("Retry") { Task { await load() } } }
            if working { ProgressView() }
        }.privateScreen().navigationTitle("Inbox settings").task { await load() }
            .confirmationDialog("Apply these receiving rules?", isPresented: $confirming) {
                Button("Save settings") {
                    let ids = Array(Set(allowed.components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty })).sorted()
                    Task { await save(Change(mode: mode, allow: ids)) }
                }
            }
    }
    private func load() async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            let next: Policy? = try model.decode(try await engine.inboxPolicy())
            guard epoch == model.epoch, !Task.isCancelled else { return }
            mode = next?.mode ?? "closed"; allowed = next?.allow.joined(separator: "\n") ?? ""; loaded = true
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func save(_ change: Change) async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        guard change.allow.allSatisfy({ $0.range(of: "^[0-9a-v]{19}[0g]$", options: .regularExpression) != nil }) else {
            model.error = String(localized: "Enter valid account IDs, one per line."); return
        }
        working = true; pending = change; model.activity(); defer { working = false }
        do {
            _ = try await coreOperation { try await engine.configureInbox(mode: change.mode, allow: change.allow, operation: $0) }
            guard epoch == model.epoch else { return }; pending = nil; model.notice = String(localized: "Saved")
        } catch {
            if epoch == model.epoch {
                if case MobileError.Failure(let code) = error,
                   ["INVALID_INPUT","FORBIDDEN","QUOTA_EXCEEDED","VERSION_CONFLICT","IDEMPOTENCY_CONFLICT"].contains(code) { pending = nil }
                model.report(error)
            }
        }
    }
}
