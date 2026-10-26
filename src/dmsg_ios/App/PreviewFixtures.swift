#if DEBUG
import Foundation
@preconcurrency import DMsgCore

extension SessionModel {
    /// UI-test-only, in-memory presentation fixtures. Never unlocks Rust,
    /// writes a real workspace, or appears in a Release build.
    func loadFixtures() {
        installFixtures(
            SessionSnapshot(generation: 0, sequence: 0, state: "unlocked", account: "040g2081040g2081040g", deviceId: "fixture-device",
                            pendingSync: 2, conflicts: 0, approvalsWaiting: 1, fastUnlock: false, setupPending: false, lastSyncAtMs: nil, syncError: nil),
            items: [
                VaultItem(id: "fixture-github", revision: "1", kind: "login", title: "GitHub", subtitle: "lin@studio.example", notes: "Personal development account", favorite: true, synced: true, conflict: false, reserved: false, files: [],
                          fields: [VaultField(id: "username", kind: "text", label: "Username", value: "lin@studio.example", concealed: false),
                                   VaultField(id: "password", kind: "concealed", label: "Password", value: "••••••••", concealed: true)]),
                VaultItem(id: "fixture-mail", revision: "1", kind: "login", title: "Work email", subtitle: "hello@studio.example", notes: "", favorite: true, synced: true, conflict: false, reserved: false, files: [], fields: []),
                VaultItem(id: "fixture-note", revision: "1", kind: "note", title: "Project notes", subtitle: "A place for unfinished ideas", notes: "", favorite: false, synced: false, conflict: false, reserved: false, files: [], fields: []),
            ])
        if ProcessInfo.processInfo.arguments.contains("--ui-recovery-pending") {
            let account = "040g2081040g2081040g"
            recordRecoveryObservation(RecoveryObservation(account: account, pending: true,
                digest: "fixture-recovery", observedAtMs: UInt64(Date.now.timeIntervalSince1970 * 1000) + 1,
                stale: false), for: account)
        }
    }
}
#endif
