import SwiftUI
@preconcurrency import DMsgCore

struct ChannelSettingsView: View {
    let channel: String
    @Environment(SessionModel.self) private var model
    @State private var details: Details?
    @State private var name = ""
    @State private var muted = false
    @State private var selectedAction: Change?
    @State private var pendingChange: Change?
    @State private var pending: [Pending] = []
    @State private var working = false
    @State private var operationID: String?
    struct Member: Decodable { let role: String; let canRotate: Bool }
    struct Ledger: Decodable { let members: [String: Member] }
    struct Details: Decodable { let name: String; let muted: Bool; let hidden: Bool; let ledger: Ledger? }
    struct Pending: Decodable, Identifiable { let key: String; let action: String; var id: String { key } }
    struct Change: Equatable { let action: String; let account: String?; let role: String?; var canRotate: Bool = false }
    private var myRole: String? { details?.ledger?.members[model.session?.account ?? ""]?.role }
    var body: some View {
        Form {
            Section("Conversation") {
                TextField("Conversation name", text: $name)
                Toggle("Mute notifications", isOn: $muted)
                Button("Save settings") { Task { await save() } }.disabled(name.isEmpty || working)
            }
            Section("Members") {
                ForEach((details?.ledger?.members ?? [:]).keys.sorted(), id: \.self) { account in
                    VStack(alignment: .leading, spacing: 6) {
                        FingerprintRow(title: "Account ID", value: account)
                        Text(details?.ledger?.members[account]?.role ?? "").font(.caption)
                        if myRole == "owner" && account != model.session?.account {
                            Toggle("Allow key rotation", isOn: Binding(get: { details?.ledger?.members[account]?.canRotate ?? false }, set: { allowed in
                                selectedAction = Change(action: "role", account: account, role: details?.ledger?.members[account]?.role, canRotate: allowed)
                            }))
                            Menu("Change role") {
                                ForEach(["admin", "member", "publisher"], id: \.self) { role in
                                    Button(role) { selectedAction = Change(action: "role", account: account, role: role, canRotate: details?.ledger?.members[account]?.canRotate ?? false) }
                                }
                            }
                        }
                        if ["owner", "admin"].contains(myRole) && account != model.session?.account {
                            Button("Remove member", role: .destructive) { selectedAction = Change(action: "remove", account: account, role: nil) }
                        }
                    }
                }
            }
            if ["owner", "admin"].contains(myRole) || details?.ledger?.members[model.session?.account ?? ""]?.canRotate == true {
                Button("Rotate conversation keys") { selectedAction = Change(action: "rekey", account: nil, role: nil) }
            }
            if myRole == "owner" {
                Button("Archive conversation", role: .destructive) { selectedAction = Change(action: "archive", account: nil, role: nil) }
            } else {
                Button("Leave conversation", role: .destructive) { selectedAction = Change(action: "leave", account: nil, role: nil) }
            }
            if let pendingChange { Button("Retry original change") { Task { await change(pendingChange) } } }
            ForEach(pending) { item in
                Button("Reconcile original operation") { Task {
                    do { _ = try await model.engine?.resumeChannelOperation(channel: channel, key: item.key); await load() }
                    catch { model.report(error) }
                } }
            }
        }.disabled(working).privateScreen().navigationTitle("Conversation settings").task { await load() }.refreshable { await load() }
            .confirmationDialog("Apply this conversation change?", isPresented: Binding(get: { selectedAction != nil }, set: { if !$0 { selectedAction = nil } })) {
                Button("Confirm change", role: .destructive) { if let action = selectedAction { Task { await change(action) } } }
            }
    }
    private func load() async {
        do {
            if let result = try await model.engine?.channelDetails(channel: channel) {
                let next: Details = try model.decode(result); details = next; name = next.name; muted = next.muted
                if let jobs = try await model.engine?.pendingChannelOperations(channel: channel) { pending = try model.decode(jobs) }
            }
        } catch { model.report(error) }
    }
    private func save() async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do { _ = try await engine.channelSettings(channel: channel, name: name, muted: muted, hidden: details?.hidden ?? false); if epoch == model.epoch { await load(); await model.loadChannels() } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
    private func change(_ change: Change) async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        selectedAction = nil
        if let pendingChange, pendingChange != change { return }
        pendingChange = change
        // Keep the request ID on failure. Do not silently repeat a remote mutation.
        if operationID == nil { operationID = UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased() + UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased() }
        guard let operationID else { return }
        working = true; defer { working = false }
        do {
            _ = try await coreOperation { try await engine.channelMemberAction(change: ChannelMemberChange(channel: channel, action: change.action,
                account: change.account, role: change.role, canRotate: change.canRotate, requestId: operationID), operation: $0) }
            guard epoch == model.epoch else { return }
            self.operationID = nil; pendingChange = nil; selectedAction = nil; await load(); await model.loadChannels()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
