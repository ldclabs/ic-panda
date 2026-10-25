import SwiftUI

struct AccountSecurityView: View {
    @Environment(SessionModel.self) private var model
    @State private var security: SecurityInfo?
    @State private var request: ApprovalView?
    @State private var token = ""
    @State private var delay = 3
    @State private var busy = false
    struct Login: Decodable, Identifiable { let principal: String; let current: Bool; var id: String { principal } }
    struct Recovery: Decodable { let opId: String; let executeAfter: UInt64 }
    struct SecurityInfo: Decodable { let logins: [Login]; let recoveryDelayMs: UInt64; let pendingRecovery: Recovery? }
    var body: some View {
        Form {
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
            Section("Recovery waiting period") {
                Stepper("\(delay) days", value: $delay, in: 1...7)
                Button("Review waiting-period change") { Task {
                    do { if let result = try await model.engine?.changeRecoveryDelay(days: UInt32(delay)) { request = try model.decode(result) } }
                    catch { model.report(error) }
                } }
                if let recovery = security?.pendingRecovery {
                    Text(Date(timeIntervalSince1970: Double(recovery.executeAfter) / 1000), style: .date)
                    Button("Dispute pending recovery", role: .destructive) { Task {
                        do { if let result = try await model.engine?.disputeRecovery(operation: recovery.opId) { request = try model.decode(result) } }
                        catch { model.report(error) }
                    } }
                }
            }
        }.privateScreen().navigationTitle("Account security").task { await load() }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
    }
    private func load() async {
        do { if let result = try await model.engine?.security() { security = try model.decode(result); delay = Int((security?.recoveryDelayMs ?? 259_200_000) / 86_400_000) } }
        catch { model.report(error) }
    }
    private func remove(_ principal: String) async {
        do { if let result = try await model.engine?.removeLogin(principal: principal) { request = try model.decode(result) } }
        catch { model.report(error) }
    }
    private func bind() async {
        guard let engine = model.engine, !busy else { return }
        busy = true; model.authenticating = true
        let epoch = model.epoch
        defer { busy = false; if epoch == model.epoch { model.authenticating = false } }
        do {
            let started = try await engine.beginBinding(legacyOrigin: false)
            guard let url = URL(string: started.url) else { throw CancellationError() }
            let callback = try await model.webLogin.authenticate(url: url)
            await model.waitUntilActive()
            guard epoch == model.epoch else { return }
            let result = try await engine.completeBinding(callback: callback.absoluteString)
            guard epoch == model.epoch else { return }; request = try model.decode(result)
        } catch { if epoch == model.epoch { engine.cancelLogin(); model.report(error) } }
    }
}
