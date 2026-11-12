import SwiftUI

struct RecoveryView: View {
    @Environment(SessionModel.self) private var model
    @State private var display = DisplayLoad()
    @State private var confirming = false
    @State private var recovery: RecoveryState?
    @State private var working = false
    @State private var failed = false
    @State private var visible = true
    @State private var request: ApprovalView?
    @State private var receipts = ApprovalReceipts()
    struct RecoveryState: Decodable {
        let enabled: Bool
        let ready: Bool
        let member: Bool
        struct Pending: Decodable { let opId: String; let executeAfter: UInt64; let expiresAt: UInt64; let thisDevice: Bool }
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
                NoticeBar(text: String(localized: "Recovery uses your bound login and a waiting period. An authorized device can cancel the recovery request. Unsynced local content cannot be recovered."))
                if let recovery {
                    Section {
                        LabeledContent("On-chain recovery", value: String(localized: recovery.enabled ? "Enabled" : "Disabled"))
                        if let delay = recovery.recoveryDelayMs { LabeledContent("Waiting period", value: String(localized: "\(delay / 86_400_000) days")) }
                        if recovery.pending == nil {
                            if recovery.member { Label("This device is authorized", systemImage: "checkmark.circle") }
                            Text("No recovery request")
                        }
                    }
                    if let pending = recovery.pending {
                        Section("Pending account recovery") {
                            LabeledContent("Waiting period ends") { Text(Date(timeIntervalSince1970: Double(pending.executeAfter) / 1000), format: .dateTime) }
                                .reminderVisible(.recovery(model.recoveryWarning?.digest ?? ""), ready: model.recoveryWarning?.pending == true)
                            LabeledContent("Expires") { Text(Date(timeIntervalSince1970: Double(pending.expiresAt) / 1000), format: .dateTime) }
                            if Double(pending.expiresAt) <= timeline.date.timeIntervalSince1970 * 1000 { Text("Expired").foregroundStyle(.secondary) }
                            if model.unlocked && recovery.member {
                                SettingsAction("Cancel this recovery request", role: .destructive) { Task { await cancelRecovery(pending.opId) } }
                                    .disabled(working || receipts["recovery.cancel"] != nil).accessibilityIdentifier("recovery.cancel")
                                ApprovalReceiptRow(receipts: receipts, key: "recovery.cancel") { request = $0 }
                            }
                        }
                    }
                    if recovery.canRequest(at: timeline.date) {
                        SettingsAction("Request account recovery") { confirming = true }.disabled(working)
                            .accessibilityIdentifier("recovery.request")
                    }
                    if recovery.canComplete(at: timeline.date) && (!recovery.member || model.session?.state == "root_required") {
                        SettingsAction("Complete recovery after waiting period", prominent: true) { Task { await complete() } }.disabled(working)
                            .accessibilityIdentifier("recovery.complete")
                    }
                }
                if model.unlocked {
                    NavigationLink("On-chain recovery") { AccountSecurityView(page: .recovery) }
                        .accessibilityIdentifier("recovery.policy")
                }
                SettingsLoadStatus(loading: working, failed: failed) { Task { await refresh(force: true) } }
            }.refreshable { await refresh(force: true) }
        }.reminderReadOnDisplay(ready: recovery != nil && recovery?.pending == nil).privateScreen().navigationTitle("Account recovery")
            .onAppear { visible = true }.onDisappear { visible = false }
            .task(id: model.contentRevision) { await refresh() }
            .approvalDestination($request, receipts: receipts, model: model)
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
                pending: .init(opId: String(repeating: "a", count: 64), executeAfter: now - 86_400_000, expiresAt: now - 1, thisDevice: true))
            return
        }
        if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-member-security") {
            let now = UInt64(Date.now.timeIntervalSince1970 * 1000)
            recovery = RecoveryState(enabled: true, ready: false, member: true,
                pending: .init(opId: String(repeating: "a", count: 64), executeAfter: now + 259200000, expiresAt: now + 864000000, thisDevice: false), recoveryDelayMs: 259200000)
            return
        }
        #endif
        if model.unlocked {
            await display.load(model, kind: "recovery", force: force) { (next: RecoveryState) in
                recovery = next
            }
            await receipts.settle(model)
            return
        }
        do { if let result = try await model.engine?.recoveryStatus() {
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
            recovery = try decoder.decode(RecoveryState.self, from: Data(result.json.utf8))
        } }
        catch { if epoch == model.epoch, !Task.isCancelled { failed = true } }
    }

    private func cancelRecovery(_ operation: String) async {
        guard model.unlocked, recovery?.member == true, !working else { return }
        let epoch = model.epoch
        working = true; model.activity(); defer { working = false }
        do {
            let value = try await receipts.request("recovery.cancel", model: model) { try await $0.disputeRecovery(operation: operation) }
            guard let value, epoch == model.epoch, visible, !Task.isCancelled else { return }
            request = value
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }
    private func complete() async {
        guard !working else { return }; working = true; defer { working = false }
        let epoch = model.epoch
        do { _ = try await model.engine?.completeRecovery(); if epoch == model.epoch { await model.updateSession(); await refresh(force: true) } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
}
