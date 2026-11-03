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

        func canRequest(at now: Date) -> Bool {
            guard enabled, !member else { return false }
            return pending.map { Double($0.expiresAt) <= now.timeIntervalSince1970 * 1000 } ?? true
        }

        func canComplete(at now: Date) -> Bool {
            // Takeover clears the pending request before root recovery and
            // rotation finish. Completion is safe to repeat for a member.
            guard enabled else { return false }
            if member { return true }
            guard let pending, pending.thisDevice else { return false }
            let milliseconds = now.timeIntervalSince1970 * 1000
            return milliseconds >= Double(pending.executeAfter) && milliseconds < Double(pending.expiresAt)
        }
    }
    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { timeline in
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
                            if Double(pending.expiresAt) <= timeline.date.timeIntervalSince1970 * 1000 { Text("Expired").foregroundStyle(.secondary) }
                        } else if recovery.member { Label("This device is authorized", systemImage: "checkmark.circle") }
                        else { Text("No recovery request") }
                    }
                    if recovery.canRequest(at: timeline.date) {
                        SettingsAction("Request account recovery") { confirming = true }.disabled(working)
                            .accessibilityIdentifier("recovery.request")
                    }
                    if recovery.canComplete(at: timeline.date) && (!recovery.member || model.session?.state == "root_required") {
                        SettingsAction("Complete recovery after waiting period", prominent: true) { Task { await complete() } }.disabled(working)
                            .accessibilityIdentifier("recovery.complete")
                    }
                    if model.unlocked { NavigationLink("Account security") { AccountSecurityView() } }
                }
                SettingsLoadStatus(loading: working, failed: failed) { Task { await refresh(force: true) } }
            }.refreshable { await refresh(force: true) }
        }.privateScreen().navigationTitle("Recovery").task { await refresh() }
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
        #if DEBUG
        if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-recovery-expired") {
            let now = UInt64(Date.now.timeIntervalSince1970 * 1000)
            recovery = RecoveryState(enabled: true, ready: true, member: false,
                pending: .init(executeAfter: now - 86_400_000, expiresAt: now - 1, thisDevice: true))
            return
        }
        #endif
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
