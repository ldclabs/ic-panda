import SwiftUI

struct ConversationSetupView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var joining = false
    @State private var name = ""
    @State private var invitation = ""
    @State private var working = false
    var body: some View {
        Form {
            Picker("Conversation", selection: $joining) { Text("Create").tag(false); Text("Join").tag(true) }.pickerStyle(.segmented)
            if joining { TextField("Paste invitation", text: $invitation, axis: .vertical).lineLimit(3...8).textInputAutocapitalization(.never) }
            else { TextField("Conversation name", text: $name) }
            Button(joining ? "Join conversation" : "Create conversation") { Task { await complete() } }
                .disabled(working || (joining ? invitation.isEmpty : name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty))
        }.privateScreen().navigationTitle("New conversation")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
    }
    private func complete() async {
        guard let engine = model.engine else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            if joining { _ = try await coreOperation { try await engine.joinChannel(invitation: invitation, operation: $0) } }
            else { _ = try await coreOperation { try await engine.createChannel(name: name, operation: $0) } }
            guard epoch == model.epoch else { return }; await model.refresh(); dismiss()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct ChannelInvitationView: View {
    let channel: String
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var account = ""
    @State private var invitation: String?
    @State private var working = false
    var body: some View {
        Form {
            TextField("Member account ID", text: $account).textInputAutocapitalization(.never).autocorrectionDisabled()
            NoticeBar(text: String(localized: "The invitation is bound to this account. Sharing the link does not grant access to another account."))
            Button("Create invitation") { Task {
                guard let engine = model.engine else { return }; working = true; defer { working = false }
                do { invitation = try await coreOperation { try await engine.inviteChannel(channel: channel, account: account, operation: $0) } }
                catch { model.report(error) }
            } }.disabled(account.count != 20 || working)
            if let invitation { ShareLink(item: invitation) { Label("Share invitation", systemImage: "square.and.arrow.up") } }
        }.privateScreen().navigationTitle("Invite member")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
    }
}

struct AttachmentPicker: View {
    let folder: String
    let choose: (LibraryFile) -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var listing = LibraryListing.empty
    var body: some View {
        List {
            ForEach(listing.folders) { folder in NavigationLink(folder.name) { AttachmentPicker(folder: folder.id, choose: choose) } }
            ForEach(listing.files) { file in Button { choose(file) } label: { FileRow(file: file) } }
            if listing.files.isEmpty && listing.folders.isEmpty { Text("Import a file in Files before attaching it.").foregroundStyle(.secondary) }
        }.privateScreen().navigationTitle("Choose an attachment")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task {
                do { if let result = try await model.engine?.files(folder: folder) { listing = try model.decode(result) } }
                catch { model.report(error) }
            }
    }
}
