import SwiftUI

struct RootView: View {
    @Environment(SessionModel.self) private var model
    @State private var showApprovals = false

    var body: some View {
        @Bindable var model = model
        ZStack {
            VStack(spacing: 0) {
                RecoveryNotice()
                    .fixedSize(horizontal: false, vertical: true)
                Group {
                    if model.unlocked {
                        TabView(selection: $model.selectedTab) {
                            VaultView().tabItem { Label("Vault", systemImage: "key.horizontal") }.tag(AppTab.vault)
                            MessagesView().tabItem { Label("Messages", systemImage: "bubble.left.and.bubble.right") }.tag(AppTab.messages)
                            FilesView().tabItem { Label("Files", systemImage: "folder") }.tag(AppTab.files)
                            NavigationStack { AccountView() }.tabItem { Label("Account", systemImage: "person.crop.circle") }.tag(AppTab.account)
                        }
                        .onChange(of: model.selectedTab) { _, _ in model.activity(); Task { await model.refresh() } }
                        .environment(\.openApprovals, { showApprovals = true })
                        .sheet(isPresented: $showApprovals) { NavigationStack { ApprovalsView() } }
                        .id(model.viewGeneration)
                    } else { NavigationStack { AccessView() } }
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
            if model.concealed {
                Theme.paper.ignoresSafeArea()
                Image(systemName: "lock.shield").font(.largeTitle).foregroundStyle(Theme.forest)
                    .accessibilityLabel("Private workspace hidden")
            }
        }
        .alert("dMsg", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("OK", role: .cancel) { model.error = nil }
        } message: { Text(model.error ?? "") }
        .onChange(of: model.epoch) { _, _ in showApprovals = false }
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
        Button(action: open) {
            Label("Approvals", systemImage: "checkmark.shield")
                .overlay(alignment: .topTrailing) {
                    if (model.session?.approvalsWaiting ?? 0) > 0 { Circle().fill(.orange).frame(width: 7, height: 7) }
                }
        }.accessibilityIdentifier("approvals.open")
    }
}

struct AccessView: View {
    @Environment(SessionModel.self) private var model
    @State private var authentication: Authentication?
    private enum Authentication { case login, unlock }
    private var fastUnlock: Bool { model.session?.fastUnlock == true }
    private var needsSetup: Bool { model.session?.state == "no_account" || model.session?.setupPending == true }
    private var needsAuthorization: Bool { model.session?.state == "pairing_required" || model.session?.state == "root_required" }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                VStack(alignment: .leading, spacing: 16) {
                    Image("BrandMark").resizable().scaledToFit().frame(width: 64, height: 64).accessibilityHidden(true)
                    Text(model.session?.account == nil ? "Your private workspace." : "Welcome back.")
                        .font(.largeTitle.bold()).fixedSize(horizontal: false, vertical: true)
                    Text("Keep your secrets, conversations and files together. Only your authorized devices can open them.")
                        .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                VStack(spacing: 12) {
                    if fastUnlock {
                        Button { Task {
                            authentication = .unlock; defer { authentication = nil }
                            await model.fastUnlock()
                        } } label: {
                            AccessActionLabel("Unlock with system authentication", symbol: "lock.open", working: authentication == .unlock)
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
                        authentication = .login; defer { authentication = nil }
                        await model.login()
                    } } label: {
                        AccessActionLabel("Continue with Internet Identity", symbol: "person.crop.circle", working: authentication == .login)
                    }.modifier(AccessActionStyle(prominent: !fastUnlock && !needsSetup && !needsAuthorization))
                        .disabled(model.engine == nil && !model.uiTesting).accessibilityIdentifier("login.start")
                }.disabled(model.busy)
                NoticeBar(text: String(localized: "Signing in proves your identity. An existing device must authorize a new device before it can read your content."))
            }.padding(24).frame(maxWidth: 520).frame(maxWidth: .infinity)
        }.frame(maxWidth: .infinity).background(Theme.paper).navigationTitle("dMsg")
            .navigationBarTitleDisplayMode(.inline)
    }

    private func initialize() async {
        do { _ = try await model.engine?.initializeAccount(); await model.updateSession() }
        catch { model.report(error) }
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
