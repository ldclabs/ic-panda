import SwiftUI

struct AccountSecurityView: View {
    @Environment(SessionModel.self) private var model
    @State private var security: SecurityInfo?
    @State private var request: ApprovalView?
    @State private var token = ""
    @State private var delay = 3
    @State private var recoveryEnabled = false
    @State private var busy = false
    @State private var settingUp = false
    struct Login: Decodable, Identifiable { let principal: String; let current: Bool; var id: String { principal } }
    struct Recovery: Decodable { let opId: String; let executeAfter: UInt64 }
    struct SecurityInfo: Decodable { let logins: [Login]; let recoveryDelayMs: UInt64; let recoveryEnabled: Bool; let recoveryUpdating: Bool; let pendingRecovery: Recovery? }
    var body: some View {
        Form {
            Section { NotificationSetupPrompt { settingUp = true } }
            Section("Login identities") {
                ForEach(security?.logins ?? []) { login in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(login.principal).font(.caption.monospaced()).textSelection(.enabled)
                        if login.current { Text("Current login").font(.caption).foregroundStyle(Theme.forest) }
                        Button("Review removing this login", role: .destructive) { Task { await remove(login.principal) } }
                    }
                }
                Button("Bind another Internet Identity") { Task { await bind() } }.disabled(busy)
            }
            Section("Approve a binding request") {
                TextField("Paste binding request", text: $token, axis: .vertical).lineLimit(2...4).textInputAutocapitalization(.never)
                Button("Review binding") { Task {
                    do { if let result = try await model.engine?.reviewBinding(token: token) { request = try model.decode(result) } }
                    catch { model.report(error) }
                } }.disabled(token.isEmpty)
            }
            Section("On-chain recovery") {
                Toggle("On-chain recovery", isOn: $recoveryEnabled).disabled(busy || security == nil)
                NoticeBar(text: String(localized: recoveryEnabled ? "If every device is lost, a bound login can recover uploaded content after the waiting period. A stolen login can do the same unless an authorized device cancels in time." : "Keep an authorized device you can unlock to move to a new device. If all authorized devices are lost, recovering your login will not recover cloud content."))
                if recoveryEnabled { Stepper("\(delay) days", value: $delay, in: 1...7).disabled(busy) }
                if security?.recoveryUpdating == true { Text("Finishing the security update. New content waits until it completes.").font(.caption) }
                Button("Review in approval center") { Task { await reviewRecovery() } }
                    .disabled(busy || security == nil || (recoveryEnabled == security?.recoveryEnabled && (!recoveryEnabled || delay == Int((security?.recoveryDelayMs ?? 0) / 86_400_000))))
                if let recovery = security?.pendingRecovery {
                    Text(Date(timeIntervalSince1970: Double(recovery.executeAfter) / 1000), style: .date)
                        .reminderVisible(.recovery(model.recoveryWarning?.digest ?? ""), ready: model.recoveryWarning != nil)
                    Button("Dispute pending recovery", role: .destructive) { Task {
                        do { if let result = try await model.engine?.disputeRecovery(operation: recovery.opId) { request = try model.decode(result) } }
                        catch { model.report(error) }
                    } }
                }
            }
        }.privateScreen().navigationTitle("Account security").task { await load() }.refreshable { await load() }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
            .navigationDestination(isPresented: $settingUp) { NotificationSettingsView() }
    }
    private func load() async {
        let epoch = model.epoch
        do {
            if let result = try await model.engine?.security() {
                guard epoch == model.epoch, !Task.isCancelled else { return }
                let next: SecurityInfo = try model.decode(result)
                // A newly bound login can start a recovery: offer security alerts again.
                if let previous = security, next.logins.count > previous.logins.count, let account = model.session?.account {
                    model.setNotificationSetupDismissed(false, account: account)
                }
                security = next; delay = Int(next.recoveryDelayMs / 86_400_000); recoveryEnabled = next.recoveryEnabled
                await model.refreshRecoveryWarning()
                await model.notifications.refresh(model)
            }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func reviewRecovery() async {
        guard !busy, let security else { return }
        busy = true
        let epoch = model.epoch
        defer { busy = false }
        do {
            let days = recoveryEnabled ? delay : Int(security.recoveryDelayMs / 86_400_000)
            if let result = try await model.engine?.changeRecoveryPolicy(enabled: recoveryEnabled, days: UInt32(days)) {
                guard epoch == model.epoch, !Task.isCancelled else { return }
                request = try model.decode(result)
            }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func remove(_ principal: String) async {
        do { if let result = try await model.engine?.removeLogin(principal: principal) { request = try model.decode(result) } }
        catch { model.report(error) }
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
            guard epoch == model.epoch else { return }
            let result = try await engine.completeBinding(callback: callback.absoluteString)
            guard epoch == model.epoch else { return }; request = try model.decode(result)
        } catch { if epoch == model.epoch { engine.cancelLogin(); model.report(error) } }
    }
}
