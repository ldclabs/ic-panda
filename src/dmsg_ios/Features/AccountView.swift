import SwiftUI
import StoreKit
import DMsgCore

struct AccountView: View {
    private enum Destination: Hashable { case profile, plan }
    @Environment(SessionModel.self) private var model
    @State private var signingOut = false
    @State private var destination: Destination?
    @State private var managing = false
    @State private var profile: AccountProfile?
    @State private var avatar: UIImage?
    @State private var display = DisplayLoad()
    @State private var avatarTask: Task<Void, Never>?
    @State private var loadedProfileRevision: UInt64 = 0
    @State private var plan: SubscriptionView.Plan?
    @State private var planDisplay = DisplayLoad()
    @State private var planAccount: String?
    @State private var loadedPurchaseRevision = 0
    @State private var loginCheckedAt = Date.now
    @Environment(\.dynamicTypeSize) private var typeSize
    var body: some View {
        List {
            accountHeader
                .listRowBackground(Color.clear).listRowSeparator(.hidden)
            if model.unlocked && model.session?.loginRenewable != true && (model.session?.loginExpiresAtMs ?? 0) <= UInt64(loginCheckedAt.timeIntervalSince1970 * 1000) {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Your local content remains available. Sign in with Internet Identity when an account action requires it.")
                        .font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier("account.login-hint")
                    SettingsAction("Continue with Internet Identity") { Task { await model.login(usingInternetIdentity: true) } }
                        .disabled(model.busy).accessibilityIdentifier("account.login")
                }
            }
            DisplayLoadStatus(state: planDisplay) { Task { await loadPlan(force: true) } }
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
                SettingsAction("Sign out", role: .destructive) { signingOut = true }.disabled(model.busy).accessibilityIdentifier("account.sign-out")
            }
        }.privateScreen().navigationTitle("Account").workspaceRootTitle()
            .task(id: model.session?.loginExpiresAtMs) {
                loginCheckedAt = .now
                guard let expires = model.session?.loginExpiresAtMs else { return }
                let delay = Double(expires) / 1000 - loginCheckedAt.timeIntervalSince1970
                guard delay > 0 else { return }
                do { try await Task.sleep(for: .seconds(delay)) } catch { return }
                guard !Task.isCancelled else { return }
                loginCheckedAt = .now
            }
            .navigationDestination(item: $destination) { target in
                switch target {
                case .profile: PublicProfileView()
                case .plan: SubscriptionView()
                }
            }
            .manageSubscriptionsSheet(isPresented: $managing)
            .task(id: "\(model.epoch):\(model.concealed):\(model.session?.generation ?? 0):\(model.session?.lastSyncAtMs ?? 0):\(model.profileRevision)") { await loadProfile(force: loadedProfileRevision != model.profileRevision) }
            // Unlock can create this view before authentication removes the privacy
            // shield. Resume reads when it lifts, even if the account is unchanged.
            .task(id: "plan:\(model.epoch):\(model.concealed):\(model.session?.account ?? ""):\(model.session?.generation ?? 0):\(model.purchases.revision)") {
                await loadPlan(force: loadedPurchaseRevision != model.purchases.revision)
            }
            .onChange(of: managing) { wasPresented, isPresented in
                if wasPresented && !isPresented { Task { await loadPlan(force: true) } }
            }
            .onDisappear { avatarTask?.cancel() }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { ActivityButton() }
                ToolbarItem(placement: .primaryAction) { Button("Lock workspace", systemImage: "lock") { Task { await model.lock() } }.accessibilityIdentifier("account.lock") }
            }
            .alert("Sign out of this account?", isPresented: $signingOut) {
                Button("Sign out", role: .destructive) { Task { await logout() } }
                Button("Cancel", role: .cancel) { }
            } message: { Text("Your encrypted files remain here. Saved login and quick unlock are removed.") }
    }

    private var managesSubscription: Bool {
        model.purchases.showsControls && plan?.managesSubscription(environment: model.purchases.storeEnvironment) == true
    }

    @ViewBuilder private var accountHeader: some View {
        if typeSize.isAccessibilitySize {
            VStack(alignment: .leading, spacing: 8) { profileButton; membershipBadge }
        } else {
            HStack(spacing: 12) { profileButton; membershipBadge.fixedSize(horizontal: true, vertical: false) }
        }
    }

    private var profileButton: some View {
        Button { destination = .profile } label: {
            HStack(spacing: 12) { identityImage; identityText }
                .frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
        }.buttonStyle(.plain).accessibilityIdentifier("account.profile")
    }

    @ViewBuilder private var membershipBadge: some View {
        if let plan {
            Button {
                if managesSubscription { managing = true }
                else { destination = .plan }
            } label: {
                HStack(spacing: 8) {
                    AppGlyph.plan.image
                    Text(verbatim: plan.plan)
                    Image(systemName: "chevron.forward").font(.caption.weight(.semibold))
                }
                .font(.subheadline.weight(.semibold)).foregroundStyle(Theme.forest)
                .padding(.horizontal, 10).padding(.vertical, 6)
                .background(Theme.forest.opacity(0.08), in: Capsule())
                .frame(minHeight: 44).contentShape(Rectangle())
            }.buttonStyle(.plain).disabled(model.purchases.working)
                .accessibilityLabel(Text("Plan") + Text(verbatim: ": \(plan.plan)"))
                .accessibilityHint(managesSubscription ? Text("Manage subscriptions") : Text("Plan and usage"))
                .accessibilityIdentifier("account.membership-badge")
        } else if !planDisplay.failed {
            ProgressView().controlSize(.small).accessibilityLabel("Loading…")
                .accessibilityIdentifier("account.membership-loading")
        }
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
            HStack(spacing: 4) {
                Text("Public profile")
                Image(systemName: "chevron.forward").font(.caption2).accessibilityHidden(true)
            }.font(.caption).foregroundStyle(Theme.forest)
        }.frame(maxWidth: .infinity, alignment: .leading)
    }

    private func loadPlan(force: Bool = false) async {
        if planAccount != model.session?.account { plan = nil; planAccount = model.session?.account }
        guard model.unlocked, !model.concealed, !Task.isCancelled else { return }
        #if DEBUG
        if model.uiTesting { plan = SubscriptionView.Plan.fixture; return }
        #endif
        let revision = model.purchases.revision, epoch = model.epoch
        await planDisplay.load(model, kind: "plan", force: force) { (incoming: SubscriptionView.Plan) in
            plan = incoming
        }
        if epoch == model.epoch, !Task.isCancelled, planDisplay.loaded, !planDisplay.failed { loadedPurchaseRevision = revision }
    }

    private func loadProfile(force: Bool = false) async {
        guard model.unlocked, !model.concealed, !Task.isCancelled else { return }
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
