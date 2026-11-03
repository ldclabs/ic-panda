import SwiftUI

struct RecoveryView: View {
    @Environment(SessionModel.self) private var model
    @State private var display = DisplayLoad()
    @State private var confirming = false
    @State private var recovery: RecoveryState?
    @State private var working = false
    @State private var failed = false
    struct RecoveryState: Decodable {
        let enabled: Bool
        let ready: Bool
        let member: Bool
        struct Pending: Decodable { let executeAfter: UInt64; let expiresAt: UInt64; let thisDevice: Bool }
        var pending: Pending? = nil
        var recoveryDelayMs: UInt64? = nil

        var canComplete: Bool {
            // Takeover clears the pending request before root recovery and
            // rotation finish. Completion is safe to repeat for a member.
            enabled && (ready || member)
        }
    }
    var body: some View {
        List {
            if model.unlocked { DisplayLoadStatus(state: display) { Task { await refresh(force: true) } } }
            if recovery?.enabled == false { NoticeBar(text: String(localized: "Keep an authorized device you can unlock to move to a new device. If all authorized devices are lost, recovering your login will not recover cloud content.")) }
            NoticeBar(text: String(localized: "Recovery uses your bound login and a waiting period. An authorized device can dispute it. Unsynced local content cannot be recovered."))
            if let recovery {
                Section("Recovery status") {
                    LabeledContent("On-chain recovery", value: String(localized: recovery.enabled ? "Enabled" : "Disabled"))
                    if let delay = recovery.recoveryDelayMs { LabeledContent("Waiting period", value: String(localized: "\(delay / 86_400_000) days")) }
                    if let pending = recovery.pending {
                        LabeledContent("Available after") { Text(Date(timeIntervalSince1970: Double(pending.executeAfter) / 1000), style: .date) }
                        LabeledContent("Expires") { Text(Date(timeIntervalSince1970: Double(pending.expiresAt) / 1000), style: .date) }
                    } else if recovery.member { Label("This device is authorized", systemImage: "checkmark.circle") }
                    else { Text("No recovery request") }
                }
                if !recovery.member && recovery.pending == nil {
                    SettingsAction("Request account recovery") { confirming = true }.disabled(!recovery.enabled || working)
                }
                if recovery.ready || recovery.member && model.session?.state == "root_required" {
                    SettingsAction("Complete recovery after waiting period", prominent: true) { Task { await complete() } }.disabled(!recovery.canComplete || working)
                }
                if model.unlocked { NavigationLink("Account security") { AccountSecurityView() } }
            }
            SettingsLoadStatus(loading: working, failed: failed) { Task { await refresh(force: true) } }
        }.privateScreen().navigationTitle("Recovery").task { await refresh() }.refreshable { await refresh(force: true) }
            .confirmationDialog("Start account recovery?", isPresented: $confirming) {
                Button("Request recovery") { Task {
                    guard !working else { return }; working = true; defer { working = false }
                    let epoch = model.epoch
                    do { _ = try await model.engine?.requestRecovery(); if epoch == model.epoch { await refresh(force: true) } }
                    catch { if epoch == model.epoch { model.report(error) } }
                } }
            }
    }
    private func refresh(force: Bool = false) async {
        let epoch = model.epoch; failed = false
        if model.unlocked {
            await display.load(model, kind: "recovery", force: force) { (next: RecoveryState) in
                recovery = next
            }
            return
        }
        do { if let result = try await model.engine?.recoveryStatus() {
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
            recovery = try decoder.decode(RecoveryState.self, from: Data(result.json.utf8))
        } }
        catch { if epoch == model.epoch, !Task.isCancelled { failed = true } }
    }
    private func complete() async {
        guard !working else { return }; working = true; defer { working = false }
        let epoch = model.epoch
        do { _ = try await model.engine?.completeRecovery(); if epoch == model.epoch { await model.updateSession(); await refresh(force: true) } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
}
