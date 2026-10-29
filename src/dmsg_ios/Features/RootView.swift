import SwiftUI

struct RootView: View {
    @Environment(SessionModel.self) private var model
    @State private var showApprovals = false
    @State private var showIncoming = false

    var body: some View {
        @Bindable var model = model
        @Bindable var connect = ConnectNavigation.shared
        ZStack {
            VStack(spacing: 0) {
                if model.environment == "staging" {
                    Text("Test environment · Separate accounts and test purchases").font(.caption.weight(.semibold)).padding(8).frame(maxWidth: .infinity).background(.orange.opacity(0.12))
                }
                RecoveryNotice()
                    .fixedSize(horizontal: false, vertical: true)
                Group {
                    if model.unlocked {
                        TabView(selection: $model.selectedTab) {
                            VaultView().tabItem { Label(AppTab.vault.title, glyph: AppTab.vault.icon) }.tag(AppTab.vault)
                            MessagesView().tabItem { Label(AppTab.messages.title, glyph: AppTab.messages.icon) }.tag(AppTab.messages)
                            FilesView().tabItem { Label(AppTab.files.title, glyph: AppTab.files.icon) }.tag(AppTab.files)
                            NavigationStack { AccountView() }.tabItem { Label(AppTab.account.title, glyph: AppTab.account.icon) }.tag(AppTab.account)
                        }
                        .onChange(of: model.selectedTab) { _, _ in model.activity(); Task { await model.refresh() } }
                        .environment(\.openApprovals, { showApprovals = true })
                        .sheet(isPresented: $showApprovals) { NavigationStack { ApprovalsView() } }
                        .id(model.viewGeneration)
                    } else if model.session?.state == "moved" { NavigationStack { MovedView() } }
                    else { NavigationStack { AccessView() } }
                }.frame(maxWidth: .infinity, minHeight: 0, maxHeight: .infinity)
            }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            if let notice = model.notice, !model.concealed {
                VStack { Spacer(); NoticeBar(text: notice).padding(.horizontal, 20).padding(.bottom, 86) }
                    .allowsHitTesting(false)
                    .task(id: notice) {
                        try? await Task.sleep(for: .seconds(4))
                        if model.notice == notice { model.notice = nil }
                    }
            }
            if let plan = model.notifications.presented, !model.concealed {
                VStack {
                    Spacer()
                    HStack(alignment: .top, spacing: 12) {
                        Image(systemName: plan.category == .security ? "shield" : "bell").foregroundStyle(Theme.forest)
                        Text(plan.body).font(.callout).frame(maxWidth: .infinity, alignment: .leading)
                        Button("Open") { model.reminderEntry = ReminderEntry(account: plan.account, target: plan.target); model.notifications.presented = nil }
                    }.padding(14).background(Theme.surface, in: RoundedRectangle(cornerRadius: 14)).padding(.horizontal, 16).padding(.bottom, 82)
                }
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
        .alert("dMsg", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("OK", role: .cancel) { model.error = nil }
        } message: { Text(model.error ?? "") }
        .sheet(item: $connect.pending) { link in NavigationStack { ConnectLandingView(link: link) } }
        .sheet(item: $model.reminderEntry) { entry in NavigationStack { RemindersView(entry: entry) } }
        .onChange(of: connect.pending) { _, _ in showApprovals = false; showIncoming = false }
        .sheet(isPresented: $showIncoming) { NavigationStack { IncomingSharesView() }.opacity(model.unlocked ? 1 : 0).allowsHitTesting(model.unlocked) }
        .task { routeIntent() }
        .onChange(of: IntentNavigation.shared.destination) { _, _ in routeIntent() }
        .onChange(of: model.unlocked) { _, unlocked in if unlocked { routeIntent() } }
        .onChange(of: model.epoch) { _, _ in showApprovals = false; showIncoming = false }
    }
    private func routeIntent() {
        guard model.unlocked, let destination = IntentNavigation.shared.destination else { return }
        IntentNavigation.shared.destination = nil
        switch destination {
        case .vault: model.selectedTab = .vault
        case .approvals: model.selectedTab = .account; showApprovals = true
        case .imports: model.selectedTab = .files; showIncoming = true
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

struct ApprovalButton: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.openApprovals) private var open
    var body: some View {
        HStack {
        ReminderButton()
        Button(action: open) {
            Label("Approvals", glyph: .approvals)
                .overlay(alignment: .topTrailing) {
                    if (model.session?.approvalsWaiting ?? 0) > 0 { Circle().fill(.orange).frame(width: 7, height: 7) }
                }
        }.accessibilityIdentifier("approvals.open")
        }
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
    private var fastUnlock: Bool { model.session?.fastUnlock == true }
    private var needsSetup: Bool { model.session?.state == "no_account" || model.session?.setupPending == true }
    private var needsAuthorization: Bool { model.session?.state == "pairing_required" || model.session?.state == "root_required" }
    private var guidance: String {
        if needsSetup {
            return String(localized: "Create your encrypted workspace to start saving passwords, notes and files.")
        }
        if needsAuthorization {
            return String(localized: "To use your existing account on this iPhone, approve this device from one of your authorized devices.")
        }
        if model.session?.account != nil {
            return fastUnlock
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
                    Text(LocalizedStringKey(model.session?.account == nil ? "Your private workspace." : "Welcome back."))
                        .font(.largeTitle.bold()).fixedSize(horizontal: false, vertical: true)
                    Text("Keep your secrets, conversations and files together. Only your authorized devices can open them.")
                        .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                VStack(spacing: 12) {
                    if fastUnlock {
                        Button { Task {
                            await model.fastUnlock()
                        } } label: {
                            AccessActionLabel("Unlock with system authentication", symbol: "lock.open", working: model.unlocking)
                        }.modifier(AccessActionStyle(prominent: true)).accessibilityIdentifier("login.unlock")
                    }
                    if needsSetup {
                        Button { Task { await initialize() } } label: {
                            AccessActionLabel("Set up this account", symbol: "person.crop.circle.badge.plus")
                        }.modifier(AccessActionStyle(prominent: !fastUnlock))
                        if model.session?.state == "no_account" {
                            NavigationLink { LoginBindingRequestView() } label: {
                                AccessActionLabel("Join an existing account", symbol: "person.2")
                            }.modifier(AccessActionStyle(prominent: false))
                        }
                    } else if needsAuthorization {
                        NavigationLink { PairView() } label: {
                            AccessActionLabel("Authorize this device", symbol: "checkmark.shield")
                        }.modifier(AccessActionStyle(prominent: !fastUnlock)).accessibilityIdentifier("login.authorize")
                    }
                    Button { Task {
                        signingIn = true; defer { signingIn = false }
                        await model.login()
                    } } label: {
                        AccessActionLabel("Continue with Internet Identity", symbol: "person.crop.circle", working: signingIn)
                    }.modifier(AccessActionStyle(prominent: !fastUnlock && !needsSetup && !needsAuthorization))
                        .disabled(model.engine == nil && !model.uiTesting).accessibilityIdentifier("login.start")
                }.disabled(model.busy)
                NoticeBar(text: guidance)
                MovedCopiesSection()
                AccessFooter()
            }.padding(24).frame(maxWidth: 520).frame(maxWidth: .infinity)
        }.frame(maxWidth: .infinity).background(Theme.paper).navigationTitle("dMsg")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .primaryAction) { ReminderButton() } }
    }

    private func initialize() async {
        do { _ = try await model.engine?.initializeAccount(); await model.updateSession() }
        catch { model.report(error) }
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

/// The copies set aside on this iPhone, kept for import until deleted.
private struct MovedCopiesSection: View {
    private struct Copy: Decodable, Identifiable {
        let id: String
        let account: String?
        let deviceId: String
        let movedAt: UInt64
    }

    @Environment(SessionModel.self) private var model
    @State private var copies: [Copy] = []
    @State private var deleting: Copy?

    var body: some View {
        Group {
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
                            Button("Delete copy", role: .destructive) { deleting = copy }.accessibilityIdentifier("moved.delete")
                        }
                    }
                }
            }
        }
        .task(id: model.session?.generation) { await load() }
        .confirmationDialog("Delete copy", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
            Button("Delete copy", role: .destructive) { if let copy = deleting { Task { await discard(copy) } } }
            Button("Cancel", role: .cancel) { deleting = nil }
        } message: { Text("Anything that exists only in this copy, such as files that were never backed up, is lost for good.") }
    }

    private func load() async {
        guard let engine = model.engine else { return }
        do { copies = try model.decode(try await engine.movedCopies()) }
        catch { model.report(error) }
    }

    private func discard(_ copy: Copy) async {
        deleting = nil
        do { _ = try await model.engine?.discardMoved(id: copy.id) }
        catch { model.report(error) }
        await load()
    }
}

private struct AccessFooter: View {
    var body: some View {
        VStack(spacing: 0) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 24) { workspaceLinks }.fixedSize(horizontal: true, vertical: false)
                VStack(spacing: 0) { workspaceLinks }
            }
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 24) { supportLinks }.fixedSize(horizontal: true, vertical: false)
                VStack(spacing: 0) { supportLinks }
            }.foregroundStyle(.secondary)
        }.font(.footnote).multilineTextAlignment(.center).frame(maxWidth: .infinity)
    }

    @ViewBuilder private var workspaceLinks: some View {
        LanguageSettingsLink(iconSize: 14).frame(minHeight: 44)
        NavigationLink("Try the local demo") { LocalDemoView() }
            .frame(minHeight: 44).accessibilityIdentifier("access.demo")
    }

    @ViewBuilder private var supportLinks: some View {
        NavigationLink("Privacy policy") { PrivacyPolicyView() }
            .frame(minHeight: 44).accessibilityIdentifier("access.privacy")
        Link("Contact support", destination: URL(string: "https://dmsg.net/_/support")!).frame(minHeight: 44)
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

    @ViewBuilder
    func body(content: Content) -> some View {
        Group {
            if prominent { content.buttonStyle(.borderedProminent) }
            else { content.buttonStyle(.bordered) }
        }.controlSize(.large).buttonBorderShape(.roundedRectangle(radius: 16))
    }
}
