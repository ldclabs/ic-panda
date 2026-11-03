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
    @State private var loading = false
    @State private var failed = false
    @State private var scope: UInt64 = 0
    @State private var visible = true
    private struct Draft: Codable { let name: String; let muted: Bool }
    private var dirty: Bool { details.map { name != $0.name || muted != $0.muted } ?? false }
    struct Member: Decodable { let role: String; let canRotate: Bool }
    struct Ledger: Decodable { let members: [String: Member]; let status: String }
    struct Details: Decodable {
        let name: String; let muted: Bool; let hidden: Bool; let ledger: Ledger?
        let mutedUntil: UInt64?; let notificationPending: Bool?; let notificationConflict: Bool?
    }
    struct Pending: Decodable, Identifiable { let key: String; let action: String; var id: String { key } }
    struct Change: Equatable { let action: String; let account: String?; let role: String?; var canRotate: Bool = false }
    private var myRole: String? { details?.ledger?.members[model.session?.account ?? ""]?.role }
    var body: some View {
        Form {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            Section("Conversation") {
                SettingsField("Conversation name", counter: "\(name.count) / 100") { TextField("Conversation name", text: $name) }
                Toggle("Mute notifications", isOn: $muted)
                Menu(NoticeCopy.text("noticeLater")) {
                    Button(NoticeCopy.text("noticePause15")) { Task { await mute(until: NotificationTime.now + 900_000) } }
                    Button(NoticeCopy.text("noticePauseHour")) { Task { await mute(until: NotificationTime.now + 3_600_000) } }
                    Button(NoticeCopy.text("noticePauseTomorrow")) { Task { await mute(until: NotificationTime.tomorrow()) } }
                }.disabled(working || details == nil)
                if let until = details?.mutedUntil, until > NotificationTime.now {
                    Text(Date(timeIntervalSince1970: Double(until) / 1000), style: .relative).foregroundStyle(.secondary)
                }
                if details?.notificationPending == true { NoticeBar(text: NoticeCopy.text("noticePending")) }
                if details?.notificationConflict == true {
                    Button("Retry") { Task {
                        if muted { await mute(until: details?.mutedUntil) } else { await save() }
                    } }.disabled(working)
                }
                SettingsAction("Save settings", prominent: true) { Task { await save() } }.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || name.count > 100 || working || !dirty)
            }.disabled(details == nil || working)
            Section("Members") {
                ForEach((details?.ledger?.members ?? [:]).keys.sorted(), id: \.self) { account in
                    VStack(alignment: .leading, spacing: 6) {
                        MessageIdentity(account: account, showID: true)
                        Text(channelRoleTitle(details?.ledger?.members[account]?.role ?? "")).font(.caption)
                        if myRole == "owner" && account != model.session?.account && details?.ledger?.status != "archived" {
                            Toggle("Allow key rotation", isOn: Binding(get: { details?.ledger?.members[account]?.canRotate ?? false }, set: { allowed in
                                selectedAction = Change(action: "role", account: account, role: details?.ledger?.members[account]?.role, canRotate: allowed)
                            }))
                            Menu("Change role") {
                                ForEach(["admin", "member", "publisher"], id: \.self) { role in
                                    Button(channelRoleTitle(role)) { selectedAction = Change(action: "role", account: account, role: role, canRotate: details?.ledger?.members[account]?.canRotate ?? false) }
                                }
                            }
                        }
                        if (myRole == "owner" || myRole == "admin" && ["member", "publisher"].contains(details?.ledger?.members[account]?.role)) && account != model.session?.account && details?.ledger?.status != "archived" {
                            Button("Remove member", role: .destructive) { selectedAction = Change(action: "remove", account: account, role: nil) }
                        }
                    }
                }
            }
            if details?.ledger?.status != "archived" && (["owner", "admin"].contains(myRole) || details?.ledger?.members[model.session?.account ?? ""]?.canRotate == true) {
                Button("Rotate conversation keys") { selectedAction = Change(action: "rekey", account: nil, role: nil) }
            }
            if myRole == "owner" && details?.ledger?.status != "archived" {
                Button("Archive conversation", role: .destructive) { selectedAction = Change(action: "archive", account: nil, role: nil) }
            } else if myRole != nil && details?.ledger?.status != "archived" {
                Button("Leave conversation", role: .destructive) { selectedAction = Change(action: "leave", account: nil, role: nil) }
            }
            if let pendingChange { Button("Retry original change") { Task { await change(pendingChange) } } }
            ForEach(pending) { item in
                Button("Reconcile original operation") { Task {
                    do { _ = try await model.engine?.resumeChannelOperation(channel: channel, key: item.key); await load() }
                    catch { model.report(error) }
                } }
            }
        }.disabled(working).privateScreen().navigationTitle("Conversation settings").confirmLeavingDraft(dirty)
            .task { scope = model.viewGeneration; await load(); if let draft = model.formDraft("channel:" + channel, as: Draft.self) { name = draft.name; muted = draft.muted } }
            .refreshable { await load() }
            .onAppear { visible = true }
            .onDisappear { visible = false; if dirty { model.keepFormDraft("channel:" + channel, value: Draft(name: name, muted: muted), generation: scope) } }
            .confirmationDialog(actionTitle, isPresented: Binding(get: { selectedAction != nil }, set: { if !$0 { selectedAction = nil } }), titleVisibility: .visible) {
                Button("Confirm change", role: selectedAction?.action == "role" ? nil : .destructive) { if let action = selectedAction { Task { await change(action) } } }
            } message: {
                if let account = selectedAction?.account { Text(account + (selectedAction?.role.map { " · " + channelRoleTitle($0) } ?? "")) }
            }
    }
    private var actionTitle: String {
        switch selectedAction?.action {
        case "remove": String(localized: "Remove member")
        case "role": String(localized: "Change role")
        case "rekey": String(localized: "Rotate conversation keys")
        case "archive": String(localized: "Archive conversation")
        case "leave": String(localized: "Leave conversation")
        default: String(localized: "Apply this conversation change?")
        }
    }
    private func load() async {
        guard !loading else { return }
        let epoch = model.epoch, editing = dirty
        loading = true; failed = false; defer { if epoch == model.epoch { loading = false } }
        do {
            if let result = try await model.engine?.channelDetails(channel: channel) {
                guard epoch == model.epoch, !Task.isCancelled else { return }
                let next: Details = try model.decode(result)
                let keepInput = editing || dirty
                details = next
                if !keepInput { name = next.name; muted = next.muted }
                if let jobs = try await model.engine?.pendingChannelOperations(channel: channel), epoch == model.epoch, !Task.isCancelled { pending = try model.decode(jobs) }
            }
        } catch { if epoch == model.epoch, !Task.isCancelled { failed = true } }
    }
    private func mute(until: UInt64?) async {
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; defer { working = false }
        do {
            _ = try await engine.channelMuteUntil(channel: channel, until: until)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            await load(); await model.notifications.refresh(model, receive: true)
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func save() async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            let result = try await engine.channelSettings(channel: channel, name: name.trimmingCharacters(in: .whitespacesAndNewlines), muted: muted, hidden: details?.hidden ?? false)
            if epoch == model.epoch, visible {
                let next: Details = try model.decode(result); details = next; name = next.name; muted = next.muted
                model.settingsDrafts.removeValue(forKey: "channel:" + channel); model.notice = String(localized: "Saved")
                await model.loadChannels()
            }
        }
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

func channelRoleTitle(_ role: String) -> String {
    switch role {
    case "owner": String(localized: "Owner")
    case "admin": String(localized: "Administrator")
    case "member": String(localized: "Member")
    case "publisher": String(localized: "Publisher")
    default: role
    }
}
