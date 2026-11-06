import SwiftUI
import DMsgCore

struct AccountView: View {
    @Environment(SessionModel.self) private var model
    @State private var signingOut = false
    @State private var profile: AccountProfile?
    @State private var avatar: UIImage?
    @State private var display = DisplayLoad()
    @State private var avatarTask: Task<Void, Never>?
    @State private var loadedProfileRevision: UInt64 = 0
    @Environment(\.dynamicTypeSize) private var typeSize
    var body: some View {
        List {
            VStack(alignment: .leading, spacing: 4) {
                NavigationLink { PublicProfileView() } label: {
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: 12) { identityImage; identityText }
                        VStack(alignment: .leading, spacing: 12) { identityImage; identityText }
                    }
                }.accessibilityIdentifier("account.profile")
                if let account = model.session?.account {
                    HStack(spacing: 4) {
                        Text(account).font(.caption.monospaced()).foregroundStyle(.secondary)
                            .lineLimit(1).truncationMode(.middle).environment(\.layoutDirection, .leftToRight)
                        AccountIDCopyButton(account: account).accessibilityIdentifier("account.copy-id")
                    }
                }
            }.listRowBackground(Color.clear).listRowSeparator(.hidden)
            DisplayLoadStatus(state: display) { Task { await loadProfile(force: true) } }
            Section {
                NavigationLink { ApprovalsView() } label: {
                    Label { LabeledContent("Approvals", value: (model.session?.approvalsWaiting ?? 0).formatted()) } icon: { AppGlyph.approvals.image }
                }.accessibilityIdentifier("account.approvals")
                NavigationLink { AccountSecurityView() } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Label("Account security", glyph: .security)
                        if model.recoveryWarning?.pending == true { Text("Account recovery is pending").font(.caption).foregroundStyle(.orange) }
                    }
                }.accessibilityIdentifier("account.security")
                NavigationLink { DevicesView() } label: { Label("Authorized devices", glyph: .device) }.accessibilityIdentifier("account.devices")
                NavigationLink { AccountsView() } label: { Label("Switch account", glyph: .account) }.accessibilityIdentifier("account.switch")
            } header: { Text("Account") }
            Section("Sync and storage") {
                NavigationLink { SyncSettingsView() } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Label("Account synchronization", systemImage: "arrow.triangle.2.circlepath")
                        if let count = model.session?.pendingSync, count > 0 {
                            LabeledContent("Waiting to sync", value: count.formatted()).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }.accessibilityIdentifier("account.sync-settings")
                NavigationLink { SubscriptionView() } label: { Label("Plan and usage", glyph: .plan) }.accessibilityIdentifier("account.plan")
                NavigationLink { SystemIntegrationsView() } label: { Label("System integrations", systemImage: "square.stack.3d.up") }
            }
            Section("Settings") {
                NavigationLink { NotificationSettingsView() } label: { Label("Notifications", systemImage: "bell") }
                LanguageSettingsLink()
                NavigationLink { AdvancedAccountView() } label: { Label("Advanced", systemImage: "slider.horizontal.3") }.accessibilityIdentifier("account.advanced")
                NavigationLink { AboutDMsgView() } label: { Label("About dMsg", systemImage: "info.circle") }.accessibilityIdentifier("account.about")
            }
            Section {
                SettingsAction("Sign out", role: .destructive) { signingOut = true }.disabled(model.busy)
            }
        }.privateScreen().navigationTitle("Account").workspaceRootTitle()
            .task(id: "\(model.session?.generation ?? 0):\(model.session?.lastSyncAtMs ?? 0):\(model.profileRevision)") { await loadProfile(force: loadedProfileRevision != model.profileRevision) }
            .onDisappear { avatarTask?.cancel() }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { ActivityButton() }
                ToolbarItem(placement: .primaryAction) { Button("Lock workspace", systemImage: "lock") { Task { await model.lock() } }.accessibilityIdentifier("account.lock") }
            }
            .confirmationDialog("Sign out of this account?", isPresented: $signingOut) {
                Button("Sign out", role: .destructive) { Task { await logout() } }
            } message: { Text("Your encrypted files remain here. Saved login and quick unlock are removed.") }
    }

    private var identityImage: some View {
        Group {
            if let avatar { Image(uiImage: avatar).resizable().scaledToFill() }
            else { AppGlyph.profile.image.resizable().scaledToFit().padding(8).foregroundStyle(Theme.forest) }
        }.frame(width: 52, height: 52).background(Theme.forest.opacity(0.08))
            .clipShape(RoundedRectangle(cornerRadius: 14)).accessibilityIdentifier("account.avatar")
    }

    private var identityText: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(profile?.displayName ?? String(localized: "My account"))
                .font(.headline).lineLimit(typeSize.isAccessibilitySize ? 3 : 2)
                .accessibilityIdentifier("account.name")
            Text("Public profile").font(.caption).foregroundStyle(Theme.forest)
        }.frame(maxWidth: .infinity, alignment: .leading)
    }

    private func loadProfile(force: Bool = false) async {
        #if DEBUG
        if model.uiTesting {
            let name = ProcessInfo.processInfo.arguments.contains("--ui-long-profile")
                ? "Studio Research · Design, Security and Collaboration" : "Studio"
            profile = AccountProfile(name: name, description: nil, avatarUrl: nil, updatedAt: 1)
            avatar = UIImage(named: "BrandMark")
            return
        }
        #endif
        let epoch = model.epoch
        await display.load(model, kind: "profile-document", force: force) { (incoming: AccountProfile) in
            profile = incoming; loadedProfileRevision = model.profileRevision
            model.rememberIdentity(name: incoming.displayName, avatar: model.accountIdentities[model.session?.account ?? ""]?.avatar)
            avatarTask?.cancel()
            avatarTask = Task {
                if let url = incoming.avatarUrl {
                    let saved = try? await model.engine?.cachedProfileAvatar(url: url)
                    let cached = await AvatarImage.decode(saved)
                    guard epoch == model.epoch, !Task.isCancelled, profile?.updatedAt == incoming.updatedAt else { return }
                    avatar = cached
                    let bytes = try? await model.engine?.profileAvatar(url: url)
                    let image = await AvatarImage.decode(bytes)
                    guard epoch == model.epoch, !Task.isCancelled, profile?.updatedAt == incoming.updatedAt else { return }
                    if let image { avatar = image; model.rememberIdentity(name: incoming.displayName, avatar: image.preparingThumbnail(of: CGSize(width: 64, height: 64))?.jpegData(compressionQuality: 0.8)) }
                } else { avatar = nil; model.rememberIdentity(name: incoming.displayName, avatar: nil) }
            }
        }
    }

    private func logout() async {
        let account = model.session?.account
        if let account { await model.cancelBackgroundForAccount(account) }
        model.invalidateSystemExports()
        await model.push.setEnabled(false, model: model)
        await model.lock()
        guard account == model.session?.account else { return }
        do { _ = try await model.engine?.logout(); await model.updateSession() } catch { model.report(error) }
    }
}
