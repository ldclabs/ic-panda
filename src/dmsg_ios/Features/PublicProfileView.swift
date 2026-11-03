import SwiftUI
import PhotosUI
@preconcurrency import DMsgCore

struct PublicProfileLink: Codable, Equatable, Identifiable {
    var name: String
    var url: String
    var rel: String = "related"
    var id = UUID()
    enum CodingKeys: String, CodingKey { case name, url, rel }
    static func == (lhs: Self, rhs: Self) -> Bool { lhs.name == rhs.name && lhs.url == rhs.url && lhs.rel == rhs.rel }
}

struct ProfileDraft: Codable, Equatable {
    var name = ""
    var about = ""
    var links: [PublicProfileLink] = []
    var avatarURL: String?
    var avatarBytes: Data?
    var removingAvatar = false
    var valid: Bool {
        name.unicodeScalars.count <= 80 && about.unicodeScalars.count <= 2000 && links.count <= 100 && links.allSatisfy {
            guard let url = URLComponents(string: $0.url) else { return false }
            return !$0.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && url.scheme?.lowercased() == "https" && url.host != nil && url.user == nil && url.password == nil && $0.url.utf8.count <= 2048
        }
    }
}

struct ProfileDraftRecord: Codable { let value: ProfileDraft; let expectedVersion: Int64? }

struct PublicProfileView: View {
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @State private var display = DisplayLoad()
    @State private var draft = ProfileDraft()
    @State private var saved = ProfileDraft()
    @State private var updated: Int64?
    @State private var status = ""
    @State private var confirming = false
    @State private var working = false
    @State private var pending = false
    @State private var canEdit: Bool?
    @State private var permissionFailed = false
    @State private var conflict = false
    @State private var photo: PhotosPickerItem?
    @State private var avatarPreview: UIImage?
    @State private var uploadedAvatar: String?
    @State private var changingAvatar = false
    @State private var scope: UInt64 = 0
    private var dirty: Bool { draft != saved }
    struct Publication: Decodable { let published: Bool; let updatedAt: Int64?; let pending: Bool? }

    var body: some View {
        Form {
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            Section {
                if let account = model.session?.account {
                    HStack {
                        FingerprintRow(title: "Account ID", value: account)
                        Spacer(minLength: 8)
                        AccountIDCopyButton(account: account).accessibilityIdentifier("profile.copy-id")
                    }
                }
            }
            NoticeBar(text: String(localized: "This profile is public. Do not include secrets or private contact information."))
            Section("Public profile") {
                SettingsField("Name", counter: "\(draft.name.unicodeScalars.count) / 80", invalid: draft.name.unicodeScalars.count > 80) {
                    TextField("Name", text: $draft.name, prompt: Text(verbatim: "")).accessibilityIdentifier("profile.name")
                }
                SettingsField("About", counter: "\(draft.about.unicodeScalars.count) / 2000", invalid: draft.about.unicodeScalars.count > 2000) {
                    TextField("About", text: $draft.about, prompt: Text(verbatim: ""), axis: .vertical).lineLimit(3...8).accessibilityIdentifier("profile.about")
                }
            }.disabled(working || pending || updated == nil)
            Section("Public avatar") {
                if let avatarPreview { Image(uiImage: avatarPreview).resizable().scaledToFit().frame(height: 96).accessibilityLabel("Public avatar") }
                PhotosPicker(selection: $photo, matching: .images) {
                    Label("Choose avatar", systemImage: "photo").labelStyle(.titleAndIcon)
                        .frame(maxWidth: .infinity).multilineTextAlignment(.center)
                        .alignmentGuide(.listRowSeparatorLeading) { _ in 0 }
                }
                if draft.avatarURL != nil || draft.avatarBytes != nil {
                    SettingsAction("Remove avatar", role: .destructive) { draft.avatarURL = nil; draft.avatarBytes = nil; draft.removingAvatar = true; uploadedAvatar = nil; avatarPreview = nil }
                }
                if changingAvatar { ProgressView() }
                Text("The selected image stays on this device until you publish. Published avatars are public and copies may remain outside dMsg.").font(.footnote).foregroundStyle(.secondary)
            }.disabled(working || pending || changingAvatar || updated == nil)
            Section("Links") {
                ForEach($draft.links) { $link in
                    VStack(alignment: .leading, spacing: 8) {
                        SettingsField("Name") { TextField("Name", text: $link.name) }
                        SettingsField("URL") { TextField("URL", text: $link.url, prompt: Text(verbatim: "https://")).textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.URL).environment(\.layoutDirection, .leftToRight) }
                        Button("Remove", role: .destructive) { draft.links.removeAll { $0.id == link.id } }
                    }
                }
                SettingsAction("Add link") { draft.links.append(PublicProfileLink(name: "", url: "https://")) }.disabled(draft.links.count >= 100)
                if !draft.valid { Text("Check field lengths and use named HTTPS links.").font(.footnote).foregroundStyle(.red) }
            }.disabled(working || pending || updated == nil)
            if updated != nil && canEdit == false { Text("An administrator device is required to publish this profile.").font(.footnote).foregroundStyle(.secondary) }
            SettingsLoadStatus(loading: updated != nil && canEdit == nil && !permissionFailed, failed: permissionFailed) { Task { await load(force: true) } }
            if !status.isEmpty { Text(status).font(.footnote).foregroundStyle(.secondary) }
            if conflict {
                NoticeBar(text: String(localized: "Saved data changed. Reload it, then review your preserved draft before publishing."))
                SettingsAction("Reload saved profile") { Task { await load(force: true, rebase: true) } }.disabled(working)
            }
            if working { ProgressView() }
            if pending { SettingsAction("Retry publication", prominent: true) { Task { await publish(retry: true) } }.disabled(working) }
            else { SettingsAction("Publish profile", prominent: true) { confirming = true }.disabled(updated == nil || working || changingAvatar || !dirty || !draft.valid || conflict || canEdit != true).accessibilityIdentifier("profile.publish") }
            if updated == nil && !working { SettingsAction("Retry loading profile") { Task { await load(force: true) } } }
        }.privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle("Public profile").confirmLeavingDraft(dirty && !pending)
            .toolbar {
                if let account = model.session?.account, let url = URL(string: "https://dmsg.net/" + account) {
                    Menu {
                        Link("Open public profile", destination: url)
                        ShareLink(item: url) { Label("Share", systemImage: "square.and.arrow.up") }
                    } label: { Label("Share", systemImage: "square.and.arrow.up") }
                }
            }
            .task { scope = model.epoch; await load() }
            .onDisappear { if dirty { model.keepFormDraft("profile", value: ProfileDraftRecord(value: draft, expectedVersion: updated), epoch: scope) } }
            .onChange(of: photo) { _, selected in Task { await choosePhoto(selected) } }
            .confirmationDialog("Publish these details publicly?", isPresented: $confirming) {
                Button("Publish") { Task { await publish(retry: false) } }
            }
    }

    private func choosePhoto(_ selected: PhotosPickerItem?) async {
        let epoch = model.epoch; changingAvatar = true
        defer { if selected == photo { changingAvatar = false } }
        do {
            guard let selected, let imported = try await selected.loadTransferable(type: PhotoImport.self) else { return }
            defer { imported.finish() }
            let bytes = try await AvatarImage.prepare(imported.file)
            guard epoch == model.epoch && visible, selected == photo else { return }
            draft.avatarBytes = bytes; draft.removingAvatar = false; uploadedAvatar = nil
            avatarPreview = await AvatarImage.decode(bytes)
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }

    private func load(force: Bool = false, rebase: Bool = false) async {
        guard !working else { return }
        let wasDirty = dirty
        var shown = draft
        let restored: ProfileDraftRecord? = model.formDraft("profile", as: ProfileDraftRecord.self)
        #if DEBUG
        if model.uiTesting {
            canEdit = true; saved = ProfileDraft(name: "Studio"); draft = restored?.value ?? saved; updated = restored?.expectedVersion ?? 1
            return
        }
        #endif
        await display.load(model, kind: "profile-editor", force: force) { (profile: AccountProfile) in
            let fresh = ProfileDraft(name: profile.name ?? "", about: profile.description ?? "", links: profile.links ?? [], avatarURL: profile.avatarUrl)
            let apply = !wasDirty && draft == shown
            saved = fresh
            if apply { draft = restored?.value ?? fresh; shown = draft }
            if rebase { updated = profile.updatedAt; conflict = false }
            else if apply {
                updated = restored?.expectedVersion ?? profile.updatedAt
                conflict = updated != profile.updatedAt
            } else if let updated, updated != profile.updatedAt { conflict = true }
        }
        let epoch = model.epoch
        permissionFailed = false
        do {
            if let value = try await model.engine?.devices(), epoch == model.epoch && visible, !Task.isCancelled {
                let account: DevicesView.AccountDevices = try model.decode(value)
                canEdit = account.devices.contains { $0.current && $0.managesAccount }
            }
        } catch { if epoch == model.epoch && visible, !Task.isCancelled { permissionFailed = true } }
        if let bytes = draft.avatarBytes { avatarPreview = await AvatarImage.decode(bytes) }
        else if !draft.removingAvatar, let url = draft.avatarURL {
            let bytes = try? await model.engine?.profileAvatar(url: url)
            if epoch == model.epoch && visible, !Task.isCancelled { avatarPreview = await AvatarImage.decode(bytes) }
        }
        do {
            if let result = try await model.engine?.profilePublication(), epoch == model.epoch && visible, !Task.isCancelled {
                let value: Publication = try model.decode(result)
                pending = value.pending == true || !value.published
                if pending { status = String(localized: "Saved. Publication is pending.") }
            }
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }

    private func publish(retry: Bool) async {
        guard let engine = model.engine, let updated, !working else { return }
        let epoch = model.epoch, submitted = draft
        working = true; model.activity(); defer { working = false }
        do {
            if !retry, let bytes = submitted.avatarBytes, uploadedAvatar == nil {
                uploadedAvatar = try await coreOperation { try await engine.uploadProfileAvatar(bytes: bytes, operation: $0) }
                guard epoch == model.epoch && visible else { return }
            }
            let links = String(decoding: try JSONEncoder().encode(submitted.links), as: UTF8.self)
            let value = try await (retry ? engine.retryProfilePublication() : engine.publishProfile(
                name: submitted.name, description: submitted.about, expectedUpdatedAt: updated,
                avatarUrl: submitted.removingAvatar ? "" : uploadedAvatar, linksJson: links))
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }
            let result: Publication = try model.decode(value)
            pending = result.pending == true || !result.published
            if let version = result.updatedAt { self.updated = version }
            saved = draft; model.settingsDrafts.removeValue(forKey: "profile")
            status = String(localized: pending ? "Saved. Publication is pending." : "Published")
            model.profileDidChange()
        } catch {
            guard epoch == model.epoch && visible else { return }
            if case MobileError.Failure(let code) = error, code == "VERSION_CONFLICT" { conflict = true }
            if let result = try? await engine.profilePublication(), epoch == model.epoch && visible,
               let value: Publication = try? model.decode(result) {
                pending = value.pending == true || !value.published
            }
            if epoch == model.epoch && visible { model.report(error) }
        }
    }
}
