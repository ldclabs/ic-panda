import SwiftUI
@preconcurrency import DMsgCore

struct AccountSecurityView: View {
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @State private var display = DisplayLoad()
    @State private var security: SecurityInfo?
    @State private var request: ApprovalView?
    @State private var token = ""
    @State private var delay = 3
    @State private var recoveryEnabled = false
    @State private var busy = false
    @State private var settingUp = false
    @State private var canManage: Bool?
    @State private var permissionFailed = false
    @State private var scope: UInt64 = 0
    struct Draft: Codable { let token: String; let delay: Int; let enabled: Bool }
    private var dirty: Bool { !token.isEmpty || (security != nil && (recoveryEnabled != security?.recoveryEnabled || delay != Int((security?.recoveryDelayMs ?? 0) / 86_400_000))) }
    struct Login: Decodable, Identifiable { let principal: String; let current: Bool; var id: String { principal } }
    struct Recovery: Decodable { let opId: String; let executeAfter: UInt64 }
    struct SecurityInfo: Decodable { let logins: [Login]; let recoveryDelayMs: UInt64; let recoveryEnabled: Bool; let recoveryUpdating: Bool; let pendingRecovery: Recovery? }
    var body: some View {
        @Bindable var model = model
        Form {
            Section("Privacy") {
                Toggle("Quick unlock with system authentication", isOn: Binding(get: { model.session?.fastUnlock ?? false }, set: { enabled in Task { await model.setFastUnlock(enabled: enabled) } })).disabled(model.busy)
                Toggle("Lock immediately in background", isOn: $model.immediateBackgroundLock)
                Text("Otherwise, dMsg locks after 3 minutes away. Device lock always locks your workspace immediately.").font(.footnote).foregroundStyle(.secondary)
                NavigationLink("Hardware security") { HardwareSecurityView() }.accessibilityIdentifier("account.hardware")
            }
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            Section { NotificationSetupPrompt { settingUp = true } }
            if security != nil && canManage == false { NoticeBar(text: String(localized: "An administrator device is required to change account security.")) }
            SettingsLoadStatus(loading: security != nil && canManage == nil && !permissionFailed, failed: permissionFailed) { Task { await load(force: true) } }
            Section("Login identities") {
                ForEach(security?.logins ?? []) { login in
                    VStack(alignment: .leading, spacing: 8) {
                        FingerprintRow(title: "Principal", value: login.principal)
                        if login.current { Text("Current login").font(.caption).foregroundStyle(Theme.forest) }
                        Button("Review removing this login", role: .destructive) { Task { await remove(login.principal) } }
                            .disabled(login.current || (security?.logins.count ?? 0) <= 1)
                    }
                }
                SettingsAction("Bind another Internet Identity") { Task { await bind() } }.disabled(busy)
            }
            .disabled(busy || canManage != true)
            Section("Approve a binding request") {
                SettingsField("Paste binding request") { TextField("Paste binding request", text: $token, axis: .vertical).lineLimit(2...4).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
                SettingsAction("Review binding") { Task { await action { try await $0.reviewBinding(token: token.trimmingCharacters(in: .whitespacesAndNewlines)) } } }.disabled(token.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .disabled(busy || canManage != true)
            Section("On-chain recovery") {
                Toggle("On-chain recovery", isOn: $recoveryEnabled).disabled(busy || security == nil)
                NoticeBar(text: String(localized: recoveryEnabled ? "If every device is lost, a bound login can recover uploaded content after the waiting period. A stolen login can do the same unless an authorized device cancels in time." : "Keep an authorized device you can unlock to move to a new device. If all authorized devices are lost, recovering your login will not recover cloud content."))
                if recoveryEnabled { Stepper("\(delay) days", value: $delay, in: 1...7).disabled(busy) }
                if security?.recoveryUpdating == true { Text("Finishing the security update. New content waits until it completes.").font(.caption) }
                SettingsAction("Review in approval center") { Task { await reviewRecovery() } }
                    .disabled(busy || security == nil || (recoveryEnabled == security?.recoveryEnabled && (!recoveryEnabled || delay == Int((security?.recoveryDelayMs ?? 0) / 86_400_000))))
            }
            .disabled(busy || canManage != true)
            if let recovery = security?.pendingRecovery {
                Section("Recovery status") {
                    Text(Date(timeIntervalSince1970: Double(recovery.executeAfter) / 1000), style: .date)
                        .reminderVisible(.recovery(model.recoveryWarning?.digest ?? ""), ready: model.recoveryWarning != nil)
                    SettingsAction("Dispute pending recovery", role: .destructive) { Task { await action { try await $0.disputeRecovery(operation: recovery.opId) } } }.disabled(busy)
                }
            }
            Section {
                NavigationLink("Account recovery") { RecoveryView() }
                NavigationLink("Remove local data") { LocalAccountRemovalView() }.accessibilityIdentifier("account.local-data")
                NavigationLink("Delete account") { AccountDeletionView() }.foregroundStyle(.red).accessibilityIdentifier("account.delete")
            }
            if busy { ProgressView() }
        }.privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle("Account security").confirmLeavingDraft(dirty)
            .task(id: model.contentRevision) { scope = model.epoch; await load(force: true) }
            .onDisappear { if dirty { model.keepFormDraft("security", value: Draft(token: token, delay: delay, enabled: recoveryEnabled), epoch: scope) } }
            .refreshable { await load(force: true) }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
            .navigationDestination(isPresented: $settingUp) { NotificationSettingsView() }
    }
    private func load(force: Bool = false) async {
        #if DEBUG
        if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-member-security") {
            security = SecurityInfo(logins: [], recoveryDelayMs: 259200000, recoveryEnabled: true, recoveryUpdating: false, pendingRecovery: Recovery(opId: "fixture", executeAfter: 1800000000000))
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
        if let restored = model.formDraft("security", as: Draft.self) {
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
    }
    private func reviewRecovery() async {
        guard !busy, let security else { return }
        busy = true
        let epoch = model.epoch
        defer { busy = false }
        do {
            let days = recoveryEnabled ? delay : Int(security.recoveryDelayMs / 86_400_000)
            if let result = try await model.engine?.changeRecoveryPolicy(enabled: recoveryEnabled, days: UInt32(days)) {
                guard epoch == model.epoch && visible, !Task.isCancelled else { return }
                request = try model.decode(result)
            }
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }
    private func remove(_ principal: String) async {
        await action { try await $0.removeLogin(principal: principal) }
    }

    private func action(_ operation: @MainActor (MobileEngine) async throws -> ObjectSnapshot) async {
        guard let engine = model.engine, !busy else { return }
        let epoch = model.epoch; busy = true; model.activity(); defer { busy = false }
        do {
            let value = try await operation(engine)
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }; request = try model.decode(value)
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }

    private func bind() async {
        guard let engine = model.engine, !busy else { return }
        busy = true
        let epoch = model.epoch
        defer { busy = false }
        do {
            let started = try await engine.beginBinding()
            guard let url = URL(string: started.url) else { throw CancellationError() }
            let callback = try await model.webLogin.authenticate(url: url)
            await model.waitUntilActive()
            guard epoch == model.epoch && visible else { return }
            let result = try await engine.completeBinding(callback: callback.absoluteString)
            guard epoch == model.epoch && visible else { return }; request = try model.decode(result)
        } catch { if epoch == model.epoch && visible { engine.cancelLogin(); model.report(error) } }
    }
}
