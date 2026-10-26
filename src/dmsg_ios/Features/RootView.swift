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
    @State private var legacy = false
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                Spacer(minLength: 52)
                Image("BrandMark").resizable().scaledToFit().frame(width: 64, height: 64).accessibilityHidden(true)
                Text(model.session?.account == nil ? "Your private workspace." : "Welcome back.")
                    .font(.largeTitle.bold()).fixedSize(horizontal: false, vertical: true)
                Text("Keep your secrets, conversations and files together. Only your authorized devices can open them.")
                    .foregroundStyle(.secondary)
                if model.session?.state == "no_account" || model.session?.setupPending == true {
                    Button("Set up this account") { Task { await initialize() } }.buttonStyle(.borderedProminent)
                    if model.session?.state == "no_account" { NavigationLink("Join an existing account") { LoginBindingRequestView() } }
                } else if model.session?.state == "pairing_required" || model.session?.state == "root_required" {
                    NavigationLink("Authorize this device") { PairView() }.buttonStyle(.borderedProminent)
                }
                if model.session?.fastUnlock == true {
                    Button("Unlock with system authentication") { Task { await model.fastUnlock() } }
                        .buttonStyle(.borderedProminent).controlSize(.large)
                }
                Button { Task { await model.login(legacy: legacy) } } label: {
                    HStack { if model.busy { ProgressView() }; Text("Continue with Internet Identity") }
                        .frame(maxWidth: .infinity).frame(minHeight: 36)
                }.buttonStyle(.borderedProminent).disabled(model.busy || model.engine == nil)
                    .accessibilityIdentifier("login.start")
                Toggle("Use my existing panda.fans identity", isOn: $legacy).font(.footnote)
                NoticeBar(text: String(localized: "Signing in proves your identity. An existing device must authorize a new device before it can read your content."))
                Spacer(minLength: 24)
            }.padding(24).frame(maxWidth: 520)
        }.frame(maxWidth: .infinity).background(Theme.paper).navigationTitle("dMsg")
    }

    private func initialize() async {
        do { _ = try await model.engine?.initializeAccount(); await model.updateSession() }
        catch { model.report(error) }
    }
}
