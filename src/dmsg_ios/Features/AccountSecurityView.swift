import SwiftUI
@preconcurrency import DMsgCore

enum AccountSecurityPage { case overview, logins, recovery }

/// The overview holds what people change often; logins and recovery policy
/// each have their own page, sharing this view's loading and review flow.
struct AccountSecurityView: View {
    var page = AccountSecurityPage.overview
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @State private var display = DisplayLoad()
    @State private var security: SecurityInfo?
    @State private var request: ApprovalView?
    @State private var receipts = ApprovalReceipts()
    @State private var token = ""
    @State private var delay = 3
    @State private var recoveryEnabled = false
    @State private var actionBusy = false
    @State private var bindingInternetIdentity = false
    @State private var resettingPasskey = false
    @State private var passkeyBinding = PasskeyBinding()
    @State private var passkeyChoices = false
    #if DEBUG
    @State private var fixtureCompletions = 0
    #endif
    private var busy: Bool { actionBusy || passkeyBinding.working }
    @State private var settingUp = false
    @State private var canManage: Bool?
    @State private var permissionFailed = false
    @State private var scope: UInt64 = 0
    struct Draft: Codable { let token: String; let delay: Int; let enabled: Bool }
    private var dirty: Bool { !token.isEmpty || (security != nil && (recoveryEnabled != security?.recoveryEnabled || delay != Int((security?.recoveryDelayMs ?? 0) / 86_400_000))) }
    struct Login: Decodable, Identifiable { let principal: String; let current: Bool; var id: String { principal } }
    struct SecurityInfo: Decodable { let logins: [Login]; let recoveryDelayMs: UInt64; let recoveryEnabled: Bool; let recoveryUpdating: Bool }
    var body: some View {
        Form {
            if page == .overview { overview }
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            if page != .overview {
                if security != nil && canManage == false { NoticeBar(text: String(localized: "An administrator device is required to change account security.")) }
                SettingsLoadStatus(loading: security != nil && canManage == nil && !permissionFailed, failed: permissionFailed) { Task { await load(force: true) } }
            }
            if page == .logins { logins }
            if page == .recovery { recoveryPolicy }
            if page == .overview {
                Section {
                    NavigationLink("Account recovery") { RecoveryView() }.accessibilityIdentifier("security.recovery")
                    NavigationLink("Remove local data") { LocalAccountRemovalView() }.accessibilityIdentifier("account.local-data")
                    NavigationLink("Delete account") { AccountDeletionView() }.foregroundStyle(.red).accessibilityIdentifier("account.delete")
                }
            }
            if actionBusy && !bindingInternetIdentity && !resettingPasskey { ProgressView() }
        }.privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle(title).confirmLeavingDraft(dirty)
            .task(id: model.contentRevision) { scope = model.viewGeneration; await load(force: true) }
            .onDisappear { if dirty { model.keepFormDraft("security", value: Draft(token: token, delay: delay, enabled: recoveryEnabled), generation: scope) } }
            .refreshable { await load(force: true) }
            .approvalDestination($request, receipts: receipts, model: model)
            .navigationDestination(isPresented: $settingUp) { NotificationSettingsView() }
            .confirmationDialog("Add a passkey login", isPresented: $passkeyChoices, titleVisibility: .visible) {
                Button("Use an existing passkey") { Task { await bind(passkey: .bindExisting) } }.accessibilityIdentifier("security.use-passkey")
                Button("Create a passkey") { Task { await bind(passkey: .bind) } }.accessibilityIdentifier("security.create-passkey")
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Use a saved passkey if you already created one. Create a new passkey only if you need another login identity.")
            }
            .onDisappear {
                if passkeyBinding.working || passkeyBinding.hasPending || bindingInternetIdentity || resettingPasskey {
                    model.passkeyLogin.cancel(); model.webLogin.cancel(); model.engine?.cancelLogin()
                    passkeyBinding.cancel()
                }
            }
            .onChange(of: model.session?.generation) {
                // The core binds the attempt to the session it began in. After a
                // background pause or lock, offer the saved passkey instead of Continue.
                if passkeyBinding.hasPending && !passkeyBinding.working { passkeyBinding.cancel(); model.engine?.cancelLogin() }
            }
    }
    private var title: LocalizedStringKey {
        switch page {
        case .overview: "Account security"
        case .logins: "Login identities"
        case .recovery: "On-chain recovery"
        }
    }

    @ViewBuilder private var overview: some View {
        @Bindable var model = model
        Section("Privacy") {
            Toggle(Biometry.quickUnlockTitle, isOn: Binding(get: { model.session?.fastUnlock ?? false }, set: { enabled in Task { await model.setFastUnlock(enabled: enabled) } })).disabled(model.busy)
            Text("Device authentication unlocks local content even after your Internet Identity login expires.")
                .font(.footnote).foregroundStyle(.secondary)
            Toggle("Lock immediately in background", isOn: $model.immediateBackgroundLock)
            Text("Otherwise, dMsg locks after 3 minutes away. Device lock always locks your workspace immediately.").font(.footnote).foregroundStyle(.secondary)
            NavigationLink("Hardware security") { HardwareSecurityView() }.accessibilityIdentifier("account.hardware")
        }
        Section { NotificationSetupPrompt { settingUp = true } }
        Section {
            NavigationLink("Login identities") { AccountSecurityView(page: .logins) }.accessibilityIdentifier("security.logins")
        }
    }

    @ViewBuilder private var logins: some View {
        Section {
            ForEach(security?.logins ?? []) { login in
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        FingerprintRow(title: "Login identity", value: login.principal)
                        Spacer(minLength: 8)
                        IDCopyButton(value: login.principal, label: "Copy")
                            .accessibilityIdentifier("security.copy-login.\(login.principal)")
                    }
                    if login.current { Text("Current login").font(.caption).foregroundStyle(Theme.forest) }
                    Button("Review removing this login", role: .destructive) { Task { await remove(login.principal) } }
                        .buttonStyle(.borderless).disabled(busy || canManage != true || login.current || (security?.logins.count ?? 0) <= 1)
                }
            }
            SettingsAction("Bind another Internet Identity", working: bindingInternetIdentity) { Task { await bind() } }
                .disabled(busy || canManage != true).accessibilityIdentifier("security.bind-ii")
            if model.session?.passkeyAvailable == true {
                SettingsAction(passkeyBinding.hasSavedCredential && !passkeyBinding.working ? "Continue adding this passkey" : "Add a passkey login", working: passkeyBinding.working) {
                    if let saved = passkeyBinding.savedOperation { Task { await bind(passkey: saved, resuming: true) } }
                    else { passkeyChoices = true }
                }.disabled(busy || canManage != true).accessibilityIdentifier("security.bind-passkey")
                if passkeyBinding.hasSavedCredential && !passkeyBinding.working {
                    NoticeBar(text: String(localized: "The passkey is saved. Retry to finish linking it to this account; no new passkey will be created."))
                        .accessibilityIdentifier("security.passkey-pending")
                }
                if passkeyBinding.limitedCredential != nil {
                    NoticeBar(text: String(localized: "This passkey has too many signed-in devices. Sign out its devices to continue. Other passkeys and saved content are not affected."))
                        .accessibilityIdentifier("security.passkey-capacity")
                    SettingsAction("Sign out this passkey’s devices and continue", role: .destructive, working: resettingPasskey) {
                        Task { await resetPasskeySessions() }
                    }.disabled(busy || canManage != true).accessibilityIdentifier("security.reset-passkey")
                }
            }
            if model.session?.loginRenewable == true {
                SettingsAction("Sign out all devices", role: .destructive) { passkeyBinding.cancel(); Task { await model.login(passkeyOperation: .logoutAll) } }
                    .disabled(busy || model.busy).accessibilityIdentifier("security.logout-all")
                Text("Existing login authorizations may remain valid for up to 12 hours. Your content devices stay authorized.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
        }
        Section("Approve a binding request") {
            SettingsField("Paste binding request") { TextField("Paste binding request", text: $token, axis: .vertical).lineLimit(2...4).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
            SettingsAction("Review binding") { Task { await action { try await $0.reviewBinding(token: token.trimmingCharacters(in: .whitespacesAndNewlines)) } } }.disabled(token.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .disabled(busy || canManage != true)
    }

    @ViewBuilder private var recoveryPolicy: some View {
        let submitted = receipts["recovery.policy"] != nil
        Section {
            Text("Prepare while you still have access: add another authorized device and review the optional on-chain recovery settings. On-chain recovery is off by default for new accounts and must be enabled in advance from an authorized administrator device that can still unlock.")
                .font(.footnote).foregroundStyle(.secondary).accessibilityIdentifier("recovery.preparation")
            Toggle("On-chain recovery", isOn: $recoveryEnabled).disabled(busy || security == nil || submitted)
            NoticeBar(text: String(localized: recoveryEnabled ? "If every device is lost, a bound login can recover uploaded content after the waiting period. A stolen login can do the same unless an authorized device cancels in time." : "Keep an authorized device you can unlock to move to a new device. If all authorized devices are lost, recovering your login will not recover cloud content."))
            if recoveryEnabled { Stepper("\(delay) days", value: $delay, in: 1...7).disabled(busy || submitted) }
            if security?.recoveryUpdating == true { Text("Finishing the security update. New content waits until it completes.").font(.caption) }
            SettingsAction("Review request") { Task { await reviewRecovery() } }
                .disabled(busy || security == nil || submitted || (recoveryEnabled == security?.recoveryEnabled && (!recoveryEnabled || delay == Int((security?.recoveryDelayMs ?? 0) / 86_400_000))))
            ApprovalReceiptRow(receipts: receipts, key: "recovery.policy") { request = $0 }
        }
        .disabled(busy || canManage != true)
    }

    private func load(force: Bool = false) async {
        #if DEBUG
        if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-passkey-binding") {
            security = SecurityInfo(logins: [Login(principal: "rrkah-fqaaa-aaaaa-aaaaq-cai", current: true)], recoveryDelayMs: 259200000, recoveryEnabled: false, recoveryUpdating: false)
            canManage = true; return
        }
        if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-member-security") {
            security = SecurityInfo(logins: [Login(principal: "rrkah-fqaaa-aaaaa-aaaaq-cai", current: true), Login(principal: "ryjl3-tyaaa-aaaaa-aaaba-cai", current: false)], recoveryDelayMs: 259200000, recoveryEnabled: true, recoveryUpdating: false)
            recoveryEnabled = true; canManage = false; return
        }
        #endif
        let epoch = model.epoch
        await display.load(model, kind: "account", force: force) { (next: SecurityInfo) in
            let editing = security != nil && (recoveryEnabled != security?.recoveryEnabled || delay != Int((security?.recoveryDelayMs ?? 0) / 86_400_000))
            if let previous = security, next.logins.count > previous.logins.count, let account = model.session?.account {
                model.setNotificationSetupDismissed(false, account: account)
            }
            security = next
            if !editing { delay = Int(next.recoveryDelayMs / 86_400_000); recoveryEnabled = next.recoveryEnabled }
        }
        if page != .overview, let restored = model.formDraft("security", as: Draft.self) {
            token = restored.token; delay = restored.delay; recoveryEnabled = restored.enabled
            model.settingsDrafts.removeValue(forKey: "security")
        }
        permissionFailed = false
        do {
            if let value = try await model.engine?.devices(), epoch == model.epoch && visible, !Task.isCancelled {
                let state: DevicesView.AccountDevices = try model.decode(value)
                canManage = state.devices.contains { $0.current && $0.managesAccount }
            }
        } catch { if epoch == model.epoch && visible, !Task.isCancelled { permissionFailed = true } }
        await receipts.settle(model)
    }
    private func reviewRecovery() async {
        guard !busy, let security else { return }
        actionBusy = true
        let epoch = model.epoch
        defer { actionBusy = false }
        do {
            let days = recoveryEnabled ? delay : Int(security.recoveryDelayMs / 86_400_000)
            let value = try await receipts.request("recovery.policy", model: model) {
                try await $0.changeRecoveryPolicy(enabled: recoveryEnabled, days: UInt32(days))
            }
            guard let value, epoch == model.epoch && visible, !Task.isCancelled else { return }
            request = value
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }
    private func remove(_ principal: String) async {
        await action { try await $0.removeLogin(principal: principal) }
    }

    private func action(_ operation: @MainActor (MobileEngine) async throws -> ObjectSnapshot) async {
        guard let engine = model.engine, !busy else { return }
        let epoch = model.epoch; actionBusy = true; model.activity(); defer { actionBusy = false }
        do {
            let value = try await operation(engine)
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }; request = try model.decode(value)
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }

    private func bind(passkey: PasskeyOperation? = nil, resuming: Bool = false, credentialID: Data? = nil) async {
        #if DEBUG
        if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-passkey-binding"), let passkey {
            await bindFixture(passkey, resuming: resuming, credentialID: credentialID); return
        }
        #endif
        guard let engine = model.engine, !busy else { return }
        model.activity()
        let epoch = model.epoch
        do {
            let result: ObjectSnapshot
            if let passkey {
                result = try await passkeyBinding.run(resuming: resuming) {
                    try await engine.beginPasskey(operation: passkey, credentialId: credentialID)
                } authenticate: { started in
                    try await model.passkeyLogin.authenticate(started)
                } complete: { started, response in
                    await model.waitUntilActive()
                    guard epoch == model.epoch && visible else { throw CancellationError() }
                    return try await engine.completePasskeyBinding(state: started.state, response: response)
                }
            } else {
                actionBusy = true; bindingInternetIdentity = true
                defer { actionBusy = false; bindingInternetIdentity = false }
                passkeyBinding.cancel()
                let started = try await engine.beginBinding()
                guard let url = URL(string: started.url) else { throw CancellationError() }
                let callback = try await model.webLogin.authenticate(url: url)
                await model.waitUntilActive()
                guard epoch == model.epoch && visible else { return }
                result = try await engine.completeBinding(callback: callback.absoluteString)
            }
            guard epoch == model.epoch && visible else { return }; request = try model.decode(result)
        } catch { if epoch == model.epoch && visible {
            if !passkeyBinding.hasSavedCredential { engine.cancelLogin() }
            reportBindingFailure(error, operation: passkey)
        } }
    }

    private func reportBindingFailure(_ error: Error, operation: PasskeyOperation?) {
        if operation == .bindExisting, case MobileError.Failure(code: "NOT_FOUND") = error {
            model.error = String(localized: "This passkey is not registered with dMsg. Choose another passkey. If an earlier creation failed, remove that unused passkey from your password manager before creating a new one.")
        } else if passkeyBinding.limitedCredential != nil {
            model.error = String(localized: "This passkey has too many signed-in devices. Sign out its devices to continue. Other passkeys and saved content are not affected.")
        } else { model.report(error) }
    }

    private func resetPasskeySessions() async {
        guard let credential = passkeyBinding.limitedCredential, !busy else { return }
        #if DEBUG
        if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-passkey-capacity") {
            await resetFixture(credential); return
        }
        #endif
        guard let engine = model.engine else { return }
        let epoch = model.epoch
        model.activity()
        do {
            actionBusy = true; resettingPasskey = true
            defer { actionBusy = false; resettingPasskey = false }
            let started = try await engine.beginPasskey(operation: .resetSessions, credentialId: credential)
            let response = try await model.passkeyLogin.authenticate(started)
            await model.waitUntilActive()
            guard epoch == model.epoch && visible else { return }
            _ = try await engine.completePasskey(state: started.state, response: response)
            guard epoch == model.epoch && visible else { return }
            passkeyBinding.cancel()
        } catch {
            if epoch == model.epoch && visible { engine.cancelLogin(); model.report(error) }
            return
        }
        // The reset does not end the current account's session. Authenticate the
        // same saved credential again, keeping this operation an account binding.
        await bind(passkey: .bindExisting, credentialID: credential)
    }

    #if DEBUG
    /// The real retry controller with a deterministic post-authentication outage.
    private func bindFixture(_ operation: PasskeyOperation, resuming: Bool, credentialID: Data? = nil) async {
        guard !busy else { return }
        let capacity = ProcessInfo.processInfo.arguments.contains("--ui-passkey-capacity")
        do {
            let value = try await passkeyBinding.run(resuming: resuming) {
                precondition(fixtureCompletions == 0 || (capacity && operation == .bindExisting && credentialID == Data([3])), "Retry must keep the selected credential and binding intent")
                return PasskeyStart(state: Data(repeating: 1, count: 32), challenge: Data(repeating: 2, count: 32), userId: Data(repeating: 3, count: 32), credentialId: nil, operation: operation, deadlineMs: UInt64(Date.now.timeIntervalSince1970 * 1000) + 300_000)
            } authenticate: { _ in
                NativePasskeyResponse(credentialId: Data([3]), clientDataJson: Data([4]), authenticatorData: Data([5]), signature: Data())
            } complete: { _, _ in
                fixtureCompletions += 1
                try await Task.sleep(for: .seconds(3))
                if fixtureCompletions == 1 { throw MobileError.Failure(code: capacity ? "SESSION_LIMIT_EXCEEDED" : "NETWORK_UNAVAILABLE") }
                return ObjectSnapshot(generation: model.session?.generation ?? 0, json: "{\"id\":\"fixture-passkey-binding\",\"digest\":\"fixture\",\"type\":\"account\",\"state\":\"awaiting_user\",\"source\":\"gui\",\"risk\":\"high\",\"expires_at\":1999999999999,\"mobile_supported\":true}")
            }
            guard visible else { return }
            request = try model.decode(value)
        } catch { if visible { reportBindingFailure(error, operation: operation) } }
    }

    private func resetFixture(_ credential: Data) async {
        precondition(credential == Data([3]) && model.session?.loginRenewable == false)
        actionBusy = true; resettingPasskey = true
        try? await Task.sleep(for: .seconds(3))
        actionBusy = false; resettingPasskey = false
        guard visible else { return }
        passkeyBinding.cancel()
        await bindFixture(.bindExisting, resuming: false, credentialID: credential)
    }
    #endif
}
