import SwiftUI

struct RootView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @State private var showApprovals = false
    @State private var showIncoming = false
    private var unreadMessages: Int { model.channels.reduce(0) { $0 + ($1.muted ? 0 : Int(clamping: $1.unread)) } }

    var body: some View {
        @Bindable var model = model
        @Bindable var connect = ConnectNavigation.shared
        @Bindable var channelNavigation = ChannelNavigation.shared
        VStack(spacing: 0) {
            if model.environment == "staging" {
                Text("Test environment · Separate accounts and test purchases").font(.caption.weight(.semibold)).padding(8).frame(maxWidth: .infinity).background(.orange.opacity(0.12))
            }
            Group {
                if model.unlocked {
                    TabView(selection: $model.selectedTab) {
                        VaultView().tabItem { Label(AppTab.vault.title, glyph: AppTab.vault.icon) }.tag(AppTab.vault)
                        MessagesView().tabItem { Label(AppTab.messages.title, glyph: AppTab.messages.icon) }.tag(AppTab.messages).badge(unreadMessages)
                        FilesView().tabItem { Label(AppTab.files.title, glyph: AppTab.files.icon) }.tag(AppTab.files)
                        AccountWorkspace().tabItem { Label(AppTab.account.title, glyph: AppTab.account.icon) }.tag(AppTab.account)
                    }
                    .onChange(of: model.selectedTab) { _, _ in model.activity(); Task { await model.refreshSelectedTab() } }
                    .environment(\.openApprovals, { showApprovals = true })
                    .sheet(isPresented: $showApprovals) { NavigationStack { ApprovalsView(showsDone: true) } }
                    .id(model.viewGeneration)
                } else if model.session?.state == "moved" { NavigationStack { MovedView() } }
                else { NavigationStack { AccessView() } }
            }.frame(maxWidth: .infinity, minHeight: 0, maxHeight: .infinity)
        }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .modifier(WorkspaceOverlays())
        .background(TouchActivityProbe { model.activity() })
        .sheet(item: $connect.pending) { link in NavigationStack { ConnectLandingView(link: link) } }
        .sheet(item: $model.reminderEntry) { entry in NavigationStack { RemindersView(entry: entry) } }
        .sheet(item: Binding(get: { model.unlocked ? channelNavigation.pending : nil }, set: { if model.unlocked { channelNavigation.pending = $0 } })) { invitation in
            NavigationStack { ConversationSetupView(invitation: invitation.value) { _ in model.selectedTab = .messages } }
        }
        .onChange(of: connect.pending) { _, _ in showApprovals = false; showIncoming = false }
        .sheet(isPresented: $showIncoming) { NavigationStack { IncomingSharesView() }.opacity(model.unlocked ? 1 : 0).allowsHitTesting(model.unlocked) }
        .task { routeIntent() }
        .onChange(of: IntentNavigation.shared.destination) { _, _ in routeIntent() }
        .onChange(of: model.unlocked) { _, unlocked in if unlocked { routeIntent(); model.announceIncomingShares() } }
        .onChange(of: scenePhase) { _, phase in if phase == .active { model.announceIncomingShares() } }
        .onChange(of: model.incomingSharesArrived) { _, _ in showIncoming = true }
        .onChange(of: model.epoch) { _, _ in showApprovals = false; showIncoming = false }
    }
    private func routeIntent() {
        guard model.unlocked, let destination = IntentNavigation.shared.destination else { return }
        IntentNavigation.shared.destination = nil
        switch destination {
        case .vault: model.selectedTab = .vault
        case .messages: model.selectedTab = .messages
        case .files: model.selectedTab = .files
        case .approvals: model.selectedTab = .account; showApprovals = true
        case .imports: model.selectedTab = .files; showIncoming = true
        }
    }

}

/// Keep transient presentation state from rebuilding the workspace's navigation bindings.
private struct WorkspaceOverlays: ViewModifier {
    @Environment(SessionModel.self) private var model
    @State private var rootBottomInset: CGFloat = 0
    private var bottomInset: CGFloat { (model.noticeHosts.isEmpty ? rootBottomInset : model.screenBottomInset) + 8 }

    func body(content: Content) -> some View {
        ZStack {
            content
            if let plan = model.notifications.presented, !model.concealed {
                VStack {
                    Spacer()
                    HStack(alignment: .top, spacing: 12) {
                        Image(systemName: plan.category == .security ? "shield" : "bell").foregroundStyle(Theme.forest)
                        Text(plan.body).font(.callout).frame(maxWidth: .infinity, alignment: .leading)
                        Button("Open") { model.reminderEntry = ReminderEntry(account: plan.account, target: plan.target); model.notifications.presented = nil }
                    }.padding(14).background(Theme.surface, in: RoundedRectangle(cornerRadius: 14)).padding(.horizontal, 16).padding(.bottom, bottomInset)
                }.ignoresSafeArea(.all, edges: .bottom)
                .accessibilityIdentifier("notifications.toast")
                .task(id: plan.id) {
                    try? await Task.sleep(for: .seconds(6))
                    if model.notifications.presented?.id == plan.id { model.notifications.presented = nil }
                }
            }
            if model.concealed {
                Theme.paper.ignoresSafeArea()
                Image(systemName: "lock.shield").font(.largeTitle).foregroundStyle(Theme.forest)
                    .accessibilityLabel("Private workspace hidden")
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.safeAreaInsets.bottom } action: { rootBottomInset = $0 }
        .overlay(alignment: .bottom) {
            Group {
                WorkspaceFeedback(bottomInset: bottomInset)
                if model.noticeHosts.isEmpty { TransientNotice().padding(.bottom, bottomInset) }
            }.ignoresSafeArea(.all, edges: .bottom)
        }
        .workspaceAlert(presenting: model.noticeHosts.isEmpty)
    }
}

/// On a wide screen the account's pages open beside its list.
private struct AccountWorkspace: View {
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        if sizeClass == .compact { NavigationStack { AccountView() } }
        else {
            NavigationSplitView { AccountView() } detail: {
                ContentUnavailableView("Account", image: AppGlyph.account.assetName)
            }.navigationSplitViewStyle(.balanced)
        }
    }
}

private struct ApprovalActionKey: EnvironmentKey { static let defaultValue: @MainActor () -> Void = {} }
extension EnvironmentValues {
    var openApprovals: @MainActor () -> Void {
        get { self[ApprovalActionKey.self] }
        set { self[ApprovalActionKey.self] = newValue }
    }
}

/// One toolbar entry for what waits on the person: approvals, reminders and sync.
/// Its place never changes, so status transitions do not move the toolbar.
struct ActivityButton: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.openApprovals) private var openApprovals
    @State private var showingStatus = false

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { timeline in
            let status = WorkspaceStatus.current(model, at: timeline.date)
            let approvals = model.session?.approvalsWaiting ?? 0
            let reminders = model.notifications.status?.counts
            let waiting = approvals > 0 || (reminders?.actionable ?? 0) > 0 || reminders?.unverified == true || status != nil
            Menu {
                Button(action: openApprovals) {
                    Text("Approvals")
                    if approvals > 0 { Text(approvals.formatted()) }
                    AppGlyph.approvals.image
                }.accessibilityIdentifier("approvals.open")
                Button { model.reminderEntry = ReminderEntry(account: model.session?.account) } label: {
                    Text(NoticeCopy.text("noticeTitle"))
                    if let count = reminders?.actionable, count > 0 { Text(count.formatted()) }
                    Image(systemName: "clock")
                }.accessibilityIdentifier("reminders.open")
                Button { showingStatus = true } label: {
                    Text(status?.title ?? "Account synchronization")
                    if let count = status?.count, count > 0 { Text(count.formatted()) }
                    Image(systemName: status?.symbol ?? "arrow.triangle.2.circlepath")
                }.accessibilityIdentifier("workspace.status")
            } label: {
                Image(systemName: status?.urgent == true ? status?.symbol ?? "bell" : "bell")
                    .foregroundStyle(status?.urgent == true ? .orange : Theme.forest)
                    .frame(minWidth: 28, minHeight: 44)
                    .overlay(alignment: .topTrailing) {
                        if waiting { Circle().fill(.orange).frame(width: 7, height: 7).padding(.top, 8).accessibilityHidden(true) }
                    }
            }.accessibilityLabel(status?.urgent == true ? status?.title ?? "Activity" : "Activity")
                .accessibilityValue(waiting ? (approvals + (reminders?.actionable ?? 0)).formatted() : "")
                .accessibilityIdentifier("activity.open")
        }.sheet(isPresented: $showingStatus) { NavigationStack { WorkspaceStatusView() } }
    }
}

struct ReminderButton: View {
    @Environment(SessionModel.self) private var model
    var body: some View {
        Button { model.reminderEntry = ReminderEntry(account: model.session?.account) } label: {
            Label(NoticeCopy.text("noticeTitle"), systemImage: "clock")
                .overlay(alignment: .topTrailing) {
                    if (model.notifications.status?.counts.actionable ?? 0) > 0 || model.notifications.status?.counts.unverified == true {
                        Circle().fill(.orange).frame(width: 7, height: 7)
                    }
                }
        }.accessibilityIdentifier("reminders.open")
    }
}

struct AccessView: View {
    @Environment(SessionModel.self) private var model
    @State private var signingIn = false
    @State private var initializationIsSlow = false
    @State private var copies: [MovedCopy] = []
    private var fastUnlock: Bool { !model.signingIntoAnotherAccount && (model.session?.fastUnlock == true || model.session?.loginRenewable == true) }
    private var needsSetup: Bool { !model.signingIntoAnotherAccount && (model.session?.state == "no_account" || model.session?.setupPending == true) }
    private var needsAuthorization: Bool { !model.signingIntoAnotherAccount && (model.session?.state == "pairing_required" || model.session?.state == "root_required") }
    private var guidance: String {
        if model.signingIntoAnotherAccount {
            return String(localized: "Use a login already linked to the account you want to open.")
        }
        if model.initializingAccount {
            return initializationIsSlow
                ? String(localized: "The network is taking longer than usual. Still waiting for confirmation; your setup progress is kept.")
                : String(localized: "Setting up your encrypted workspace. Please keep dMsg open.")
        }
        if needsSetup {
            return String(localized: "Create your encrypted workspace to start saving passwords, notes and files.")
        }
        if needsAuthorization {
            return String(localized: "To use your existing account on this iPhone, approve this device from one of your authorized devices.")
        }
        if model.session?.account != nil {
            return fastUnlock || model.session?.loginRenewable == true
                ? String(localized: "Use system authentication to unlock the encrypted workspace on this iPhone.")
                : String(localized: "Sign in again to unlock your encrypted workspace on this iPhone.")
        }
        return String(localized: "On your first visit, sign in and follow the steps to create your encrypted workspace.")
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                VStack(alignment: .leading, spacing: 16) {
                    Image("BrandMark").resizable().scaledToFit().frame(width: 64, height: 64).accessibilityHidden(true)
                    Text(LocalizedStringKey(model.signingIntoAnotherAccount ? "Sign in to another account" : model.session?.account == nil ? "Your private workspace." : "Welcome back."))
                        .font(.largeTitle.bold()).fixedSize(horizontal: false, vertical: true)
                    Text("Keep your secrets, conversations and files together. Only your authorized devices can open them.")
                        .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                VStack(spacing: 12) {
                    if fastUnlock {
                        Button { Task {
                            await model.fastUnlock()
                        } } label: {
                            AccessActionLabel(Biometry.unlockTitle, symbol: "lock.open", working: model.unlocking)
                        }.modifier(AccessActionStyle(prominent: true)).accessibilityIdentifier("login.unlock")
                    }
                    if needsSetup {
                        Button { Task { await model.initializeAccount() } } label: {
                            AccessActionLabel(model.initializingAccount ? "Setting up account…" : "Set up this account", symbol: "person.crop.circle.badge.plus", working: model.initializingAccount)
                        }.modifier(AccessActionStyle(prominent: !fastUnlock))
                            .accessibilityIdentifier("login.initialize")
                        if model.session?.state == "no_account" {
                            NavigationLink { LoginBindingRequestView() } label: {
                                AccessActionLabel("Join an existing account", symbol: "person.2")
                            }.modifier(AccessActionStyle(prominent: false)).accessibilityIdentifier("login.join")
                        }
                    } else if needsAuthorization {
                        NavigationLink { PairView() } label: {
                            AccessActionLabel("Authorize this device", symbol: "checkmark.shield")
                        }.modifier(AccessActionStyle(prominent: !fastUnlock)).accessibilityIdentifier("login.authorize")
                        if model.session?.state == "pairing_required" {
                            NavigationLink { SuccessionRequestView() } label: {
                                AccessActionLabel("Replace a device", symbol: "arrow.left.arrow.right")
                            }.modifier(AccessActionStyle(prominent: false)).accessibilityIdentifier("login.succession")
                        }
                    }
                    if model.session?.passkeyAvailable == true {
                        NavigationLink { PasskeyAccessView() } label: {
                            AccessActionLabel("Continue with a passkey", symbol: "key")
                        }.modifier(AccessActionStyle(prominent: !fastUnlock && !needsSetup && !needsAuthorization, quiet: fastUnlock || needsSetup || needsAuthorization))
                            .disabled(model.engine == nil && !model.uiTesting).accessibilityIdentifier("login.start")
                        Button { Task { await model.login(usingInternetIdentity: true) } } label: {
                            AccessActionLabel("Continue with Internet Identity", symbol: "person.crop.circle")
                        }.modifier(AccessActionStyle(prominent: false, quiet: true)).accessibilityIdentifier("login.ii")
                    } else {
                        Button { Task {
                            signingIn = true; defer { signingIn = false }
                            await model.login(usingInternetIdentity: true)
                        } } label: {
                            AccessActionLabel("Continue with Internet Identity", symbol: "person.crop.circle", working: signingIn)
                        }.modifier(AccessActionStyle(prominent: !fastUnlock && !needsSetup && !needsAuthorization, quiet: fastUnlock || needsSetup || needsAuthorization))
                            .disabled(model.engine == nil && !model.uiTesting).accessibilityIdentifier("login.start")
                    }
                }.disabled(model.busy)
                NoticeBar(text: guidance)
                MovedCopiesSection(copies: copies) { await loadCopies() }
                AccessFooter()
            }.padding(24).frame(maxWidth: 520).frame(maxWidth: .infinity)
        }.frame(maxWidth: .infinity).background(Theme.paper).navigationTitle("dMsg")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    if model.signingIntoAnotherAccount {
                        Button("Cancel") { model.signingIntoAnotherAccount = false }
                            .disabled(model.busy).accessibilityIdentifier("login.cancel-account")
                    }
                }
                ToolbarItem(placement: .primaryAction) {
                    if model.session?.account != nil, (model.notifications.status?.counts.actionable ?? 0) > 0 || model.notifications.status?.counts.unverified == true {
                        ReminderButton()
                    }
                }
            }
            .task(id: model.session?.generation) { await loadCopies() }
            .task(id: model.initializingAccount) {
                initializationIsSlow = false
                guard model.initializingAccount else { return }
                do { try await Task.sleep(for: .seconds(15)) } catch { return }
                guard model.initializingAccount, !Task.isCancelled else { return }
                initializationIsSlow = true
            }
    }

    private func loadCopies() async {
        guard let engine = model.engine else { return }
        do { copies = try model.decode(try await engine.movedCopies()) }
        catch { model.report(error) }
    }
}

private struct PasskeyAccessView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var creating = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                Text("Use an existing passkey").font(.title.bold()).fixedSize(horizontal: false, vertical: true)
                Text("Use a login already linked to the account you want to open.")
                    .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                Button { Task { await model.login() } } label: {
                    AccessActionLabel("Continue with a passkey", symbol: "key", working: model.busy && !creating)
                }.modifier(AccessActionStyle(prominent: true)).accessibilityIdentifier("passkey.login")
                if model.canResetPasskeySessions {
                    Button { Task { await model.login(passkeyOperation: .logoutAll) } } label: {
                        AccessActionLabel("Sign out all devices", symbol: "rectangle.portrait.and.arrow.right")
                    }.modifier(AccessActionStyle(prominent: false, quiet: true)).accessibilityIdentifier("login.reset-devices")
                }
                Divider()
                Text("Create a passkey").font(.headline)
                Text("Each new passkey creates a separate login identity. To add one to your current account, use Add a passkey login in account security.")
                    .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                Button { Task {
                    creating = true; defer { creating = false }
                    await model.login(passkeyOperation: .register)
                } } label: {
                    AccessActionLabel("Create a passkey", symbol: "person.crop.circle.badge.plus", working: creating)
                }.modifier(AccessActionStyle(prominent: false)).accessibilityIdentifier("login.create-passkey")
            }.disabled(model.busy || (model.engine == nil && !model.uiTesting))
                .padding(24).frame(maxWidth: 520).frame(maxWidth: .infinity)
        }.background(Theme.paper).navigationTitle("Continue with a passkey").navigationBarTitleDisplayMode(.inline)
            .navigationBarBackButtonHidden(model.busy)
            .onChange(of: model.session?.generation) { _, _ in
                if model.session?.state == "no_account" || model.session?.state == "pairing_required" || model.session?.state == "root_required" { dismiss() }
            }
            .onChange(of: model.session?.passkeyAvailable) { _, available in
                if available != true { dismiss() }
            }
    }
}

/// A workspace copied here from another iPhone never acts as the old device.
/// Setting it aside keeps it, read-only, for import and makes this iPhone a
/// new device of the account.
struct MovedView: View {
    @Environment(SessionModel.self) private var model
    @State private var working = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                VStack(alignment: .leading, spacing: 16) {
                    Image(systemName: "iphone").font(.largeTitle).foregroundStyle(Theme.forest).accessibilityHidden(true)
                    Text("Workspace copied from another iPhone").font(.title.bold()).fixedSize(horizontal: false, vertical: true)
                    Text("This workspace was made on another iPhone and copied here, for example by a restored backup or a device transfer. To protect your account it never acts as the old device: it does not unlock here, and nothing in it is erased.")
                        .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                Button { Task { await setAside() } } label: {
                    AccessActionLabel("Set up this iPhone as a new device", symbol: "iphone", working: working)
                }.modifier(AccessActionStyle(prominent: true)).disabled(working || model.busy).accessibilityIdentifier("moved.setAside")
                NoticeBar(text: String(localized: "The copy stays on this iPhone, read-only, for importing. This iPhone then joins your account as a new device: sign in, then approve it from another device or recover the account. The old iPhone keeps working."))
                Text("To replace the old iPhone instead, set the copy aside and sign in, then choose Replace the old iPhone on the copy; once this iPhone has joined, import the copy's files in Devices.")
                    .font(.footnote).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }.padding(24).frame(maxWidth: 520).frame(maxWidth: .infinity)
        }.frame(maxWidth: .infinity).background(Theme.paper).navigationTitle("dMsg")
            .navigationBarTitleDisplayMode(.inline)
    }

    private func setAside() async {
        working = true; defer { working = false }
        do { _ = try await model.engine?.setAsideMoved(); await model.updateSession() }
        catch { model.report(error) }
    }
}

/// The copies set aside on this iPhone, kept for import until deleted. A
/// copy of this account's administrator device can approve this iPhone in
/// its place before it joins.
private struct MovedCopiesSection: View {
    let copies: [MovedCopy]
    let reload: @MainActor () async -> Void
    @Environment(SessionModel.self) private var model
    @State private var deleting: MovedCopy?
    @State private var working = false
    @State private var request: ApprovalView?

    var body: some View {
        if !copies.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                Text("Copies from other iPhones").font(.headline)
                Text("Read-only copies kept for importing. Deleting one does not affect the old iPhone or your account.")
                    .font(.footnote).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                ForEach(copies) { copy in
                    HStack(spacing: 12) {
                        VStack(alignment: .leading, spacing: 2) {
                            if let account = copy.account { Text(verbatim: account).font(.footnote.monospaced()) }
                            Text(verbatim: String(copy.deviceId.prefix(8))).font(.caption.monospaced()).foregroundStyle(.secondary)
                            Text(Date(timeIntervalSince1970: TimeInterval(copy.movedAt) / 1000), style: .date).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        VStack(alignment: .trailing, spacing: 8) {
                            if copy.account != nil && copy.account == model.session?.account && model.session?.state == "pairing_required" {
                                Button("Replace the old iPhone") { Task { await replace(copy) } }
                                    .disabled(working).accessibilityIdentifier("moved.replace")
                            }
                            Button("Delete copy", role: .destructive) { deleting = copy }.accessibilityIdentifier("moved.delete")
                        }
                    }
                }
            }
            .confirmationDialog("Delete copy", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
                Button("Delete copy", role: .destructive) { if let copy = deleting { Task { await discard(copy) } } }
                Button("Cancel", role: .cancel) { deleting = nil }
            } message: { Text("Anything that exists only in this copy, such as files that were never backed up, is lost for good.") }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
        }
    }

    /// The copy's device approves this iPhone as its successor, once the person approves here.
    private func replace(_ copy: MovedCopy) async {
        guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }
        do { request = try model.decode(try await engine.movedApproveSuccessor(id: copy.id)) }
        catch { model.report(error) }
    }

    private func discard(_ copy: MovedCopy) async {
        deleting = nil
        do { _ = try await model.engine?.discardMoved(id: copy.id) }
        catch { model.report(error) }
        await reload()
    }
}

private struct AccessFooter: View {
    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 24) { links }.fixedSize(horizontal: true, vertical: false)
            VStack(spacing: 0) { links }
        }.font(.footnote).multilineTextAlignment(.center).frame(maxWidth: .infinity)
    }

    @ViewBuilder private var links: some View {
        LanguageSettingsLink(iconSize: 14).frame(minHeight: 44)
        NavigationLink(NoticeCopy.text("noticeSettings")) { NotificationSettingsView() }
            .frame(minHeight: 44).accessibilityIdentifier("access.notifications")
        NavigationLink("Privacy policy") { PrivacyPolicyView() }
            .frame(minHeight: 44).accessibilityIdentifier("access.privacy")
    }
}

private struct AccessActionLabel: View {
    let title: LocalizedStringKey
    let symbol: String
    var working = false

    init(_ title: LocalizedStringKey, symbol: String, working: Bool = false) {
        self.title = title; self.symbol = symbol; self.working = working
    }

    var body: some View {
        HStack(spacing: 10) {
            if working { ProgressView().accessibilityHidden(true) }
            else { Image(systemName: symbol).accessibilityHidden(true) }
            Text(title).multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
        }.font(.body.weight(.semibold)).frame(maxWidth: .infinity, minHeight: 28).padding(.vertical, 4)
    }
}

private struct AccessActionStyle: ViewModifier {
    let prominent: Bool
    var quiet = false

    @ViewBuilder
    func body(content: Content) -> some View {
        Group {
            if prominent { content.buttonStyle(.borderedProminent) }
            else if quiet { content.buttonStyle(.borderless) }
            else { content.buttonStyle(.bordered) }
        }.controlSize(.large).buttonBorderShape(.roundedRectangle(radius: 16))
    }
}
