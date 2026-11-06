import SwiftUI
import VisionKit
import AVFoundation

struct ConversationSetupView: View {
    var invitee = ""
    var onCreated: (String) -> Void = { _ in }
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var joining: Bool
    @State private var name = ""
    @State private var invitation: String
    @State private var working = false
    @State private var failure: String?
    @State private var created: String?
    @State private var visible = true
    @State private var leaving = false
    @State private var scope: UInt64 = 0
    @State private var completed = false
    @State private var scanning = false
    private struct Draft: Codable { var joining: Bool; var name: String; var invitation: String }
    private var summary: ChannelInvitationSummary? { ChannelInvitationSummary.parse(invitation) }
    private var valid: Bool {
        if joining { return summary?.account == model.session?.account && (summary?.expiresAt ?? 0) > UInt64(Date.now.timeIntervalSince1970 * 1000) }
        return !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && name.count <= 100 && !name.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }
    init(invitation: String = "", invitee: String = "", onCreated: @escaping (String) -> Void = { _ in }) {
        self.invitee = invitee; self.onCreated = onCreated
        _invitation = State(initialValue: invitation); _joining = State(initialValue: !invitation.isEmpty)
    }
    var body: some View {
        Group {
            if let created { ChannelInvitationView(channel: created, account: invitee) }
            else {
                Form {
                    Picker("Conversation", selection: $joining) { Text("Create").tag(false); Text("Join").tag(true) }.pickerStyle(.segmented)
                    if joining {
                        SettingsField("Paste invitation") { TextField("Paste invitation", text: $invitation, axis: .vertical).lineLimit(3...8).textInputAutocapitalization(.never).autocorrectionDisabled().accessibilityIdentifier("channel.invitation.input") }
                        if invitation.isEmpty {
                            HStack {
                                PasteButton(payloadType: String.self) { values in
                                    let text = values.first?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                                    Task { @MainActor in if !text.isEmpty { invitation = text } }
                                }.buttonBorderShape(.capsule).labelStyle(.titleAndIcon)
                                if DataScannerViewController.isSupported {
                                    Button("Scan invitation", systemImage: "qrcode.viewfinder") { Task {
                                        if await AVCaptureDevice.requestAccess(for: .video) { scanning = true }
                                    } }.buttonStyle(.bordered).buttonBorderShape(.capsule)
                                }
                            }
                        }
                        if let summary {
                            LabeledContent("Conversation name", value: summary.name)
                            MessageIdentity(account: summary.account, showID: true)
                            LabeledContent("Expires") { Text(Date(timeIntervalSince1970: Double(summary.expiresAt) / 1000), format: .dateTime) }
                            if summary.account != model.session?.account { NoticeBar(text: String(localized: "Switch to the invited account before joining.")) }
                            Text("The invitation is verified when you choose Join conversation.").font(.footnote).foregroundStyle(.secondary)
                        } else if !invitation.isEmpty { Text("The invitation is invalid.").foregroundStyle(.red) }
                    } else { SettingsField("Conversation name", counter: "\(name.count) / 100") { TextField("Conversation name", text: $name) } }
                    if let failure { NoticeBar(text: failure) }
                    if working { ProgressView() }
                    SettingsAction(LocalizedStringKey(joining ? "Join conversation" : "Create conversation"), prominent: true) { Task { await complete() } }.disabled(!valid || working)
                }.disabled(working)
            }
        }.privateScreen().navigationTitle("New conversation")
            .toolbar { if created == nil { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { if name.isEmpty && invitation.isEmpty { dismiss() } else { leaving = true } }.disabled(working) } } }
            .sheet(isPresented: $scanning) { QRCodeScanner { text in invitation = text; scanning = false } }
            .interactiveDismissDisabled(working || !name.isEmpty || !invitation.isEmpty)
            .confirmationDialog("Leave this form?", isPresented: $leaving) { Button("Keep draft and leave") { dismiss() } }
            .onAppear {
                visible = true; scope = model.viewGeneration
                if invitation.isEmpty, let saved = model.formDraft("conversation-setup", as: Draft.self) { joining = saved.joining; name = saved.name; invitation = saved.invitation }
            }
            .onDisappear {
                visible = false
                if created == nil && !completed { model.keepFormDraft("conversation-setup", value: Draft(joining: joining, name: name, invitation: invitation), generation: scope) }
            }
    }
    private func complete() async {
        guard let engine = model.engine, !working, valid else { return }
        let epoch = model.epoch; working = true; failure = nil; defer { working = false }
        do {
            let id: String
            if joining {
                _ = try await coreOperation { try await engine.joinChannel(invitation: invitation, operation: $0) }
                id = summary?.channel ?? ""
            } else { id = try await coreOperation { try await engine.createChannel(name: name.trimmingCharacters(in: .whitespacesAndNewlines), operation: $0) } }
            guard epoch == model.epoch, visible else { return }
            await model.loadChannels(); model.settingsDrafts.removeValue(forKey: "conversation-setup")
            guard epoch == model.epoch, visible else { return }; completed = true
            onCreated(id)
            // A new conversation has one member, so inviting someone is the next step.
            name = ""
            if joining { invitation = ""; dismiss() } else { created = id }
        } catch { if epoch == model.epoch, visible { failure = InboxLoading.message(error) } }
    }
}

struct ChannelInvitationView: View {
    let channel: String
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var account: String
    @State private var invitation: String?
    @State private var working = false
    @State private var failure: String?
    @State private var visible = true
    @State private var scope: UInt64 = 0
    @State private var sharing = false
    init(channel: String, account: String = "") { self.channel = channel; _account = State(initialValue: account) }
    var body: some View {
        Form {
            MessageRecipientField(title: "Member account ID", account: $account)
            NoticeBar(text: String(localized: "The invitation is bound to this account. Sharing the link does not grant access to another account."))
            if let failure { NoticeBar(text: failure) }
            if working { ProgressView() }
            SettingsAction("Share invitation", systemImage: "square.and.arrow.up", prominent: true) { Task { await share() } }
                .disabled(!validMessageAccount(account) || working).accessibilityIdentifier("channel.invitation.share")
            if invitation != nil {
                Label("Invitation created", systemImage: "checkmark.circle").font(.footnote).foregroundStyle(.secondary)
                    .accessibilityIdentifier("channel.invitation.ready")
            }
        }.privateScreen().navigationTitle("Invite member")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .sheet(isPresented: $sharing) { if let invitation { ShareSheet(items: [invitation]) } }
            .onChange(of: account) { _, _ in invitation = nil; failure = nil }
            .onAppear { visible = true; scope = model.viewGeneration; if account.isEmpty, let saved = model.formDraft("invite:" + channel, as: String.self) { account = saved } }
            .onDisappear { visible = false; model.keepFormDraft("invite:" + channel, value: account, generation: scope) }
    }
    private func share() async {
        if invitation == nil { await create() }
        if invitation != nil && !model.uiTesting { sharing = true }
    }

    private func create() async {
        guard !working, validMessageAccount(account) else { return }
        let epoch = model.epoch, recipient = account; working = true; invitation = nil; failure = nil; defer { working = false }
        do {
            let result: String
            #if DEBUG
            if model.uiTesting { result = "fixture-invitation-" + recipient }
            else { guard let engine = model.engine else { return }; result = try await coreOperation { try await engine.inviteChannel(channel: channel, account: recipient, operation: $0) } }
            #else
            guard let engine = model.engine else { return }; result = try await coreOperation { try await engine.inviteChannel(channel: channel, account: recipient, operation: $0) }
            #endif
            guard epoch == model.epoch, visible, recipient == account else { return }; invitation = result
        } catch { if epoch == model.epoch, visible, recipient == account { failure = InboxLoading.message(error) } }
    }
}

struct InviteLetterSenderView: View {
    let account: String
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        List {
            MessageIdentity(account: account, showID: true)
            ForEach(model.channels.filter(\.canInvite), id: \.id) { channel in
                NavigationLink(channel.name) { ChannelInvitationView(channel: channel.id, account: account) }
            }
            NavigationLink("New conversation") { ConversationSetupView(invitee: account) }
        }.privateScreen().navigationTitle("Invite to a conversation")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
    }
}
