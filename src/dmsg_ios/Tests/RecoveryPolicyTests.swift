#if DEBUG
import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class RecoveryPolicyTests: XCTestCase {
    private let account = "040g2081040g2081040g"

    private func model() -> SessionModel {
        let model = SessionModel()
        model.installFixtures(SessionSnapshot(generation: 1, sequence: 1, state: "unlocked",
            account: account, deviceId: "fixture-device", pendingSync: 0, conflicts: 0,
            approvalsWaiting: 0, fastUnlock: false, setupPending: false, lastSyncAtMs: nil, syncError: nil), items: [])
        return model
    }

    func testCompletedTakeoverCanResumeWithNoPendingRequest() throws {
        let status = try JSONDecoder().decode(RecoveryView.RecoveryState.self,
            from: Data(#"{"enabled":true,"ready":false,"member":true,"pending":null}"#.utf8))
        XCTAssertTrue(status.canComplete)
    }

    func testRecoveryCompletionStillRequiresAnEnabledPolicyAndEligibleDevice() throws {
        let decoder = JSONDecoder()
        for json in [
            #"{"enabled":false,"ready":false,"member":true}"#,
            #"{"enabled":true,"ready":false,"member":false}"#
        ] {
            let status = try decoder.decode(RecoveryView.RecoveryState.self, from: Data(json.utf8))
            XCTAssertFalse(status.canComplete)
        }
        let waiting = try decoder.decode(RecoveryView.RecoveryState.self,
            from: Data(#"{"enabled":true,"ready":true,"member":false}"#.utf8))
        XCTAssertTrue(waiting.canComplete)
    }

    func testRecoveryWarningRejectsOldAndForeignObservationsAndDeduplicatesNotices() {
        let model = model(), now = UInt64(Date.now.timeIntervalSince1970 * 1000) + 1000
        let pending = RecoveryObservation(account: account, pending: true, digest: "one", observedAtMs: now, stale: false)
        model.recordRecoveryObservation(pending, for: account)
        XCTAssertTrue(model.recoveryWarning?.pending == true)
        XCTAssertNotNil(model.notice)
        model.notice = nil
        model.recordRecoveryObservation(pending, for: account)
        XCTAssertNil(model.notice)
        model.recordRecoveryObservation(nil, for: account)
        XCTAssertEqual(model.recoveryWarning, pending)
        model.recordRecoveryObservation(RecoveryObservation(account: account, pending: false, digest: nil,
            observedAtMs: now - 1, stale: false), for: account)
        XCTAssertEqual(model.recoveryWarning, pending)
        model.recordRecoveryObservation(RecoveryObservation(account: "another-account", pending: false, digest: nil,
            observedAtMs: now + 1, stale: false), for: account)
        XCTAssertEqual(model.recoveryWarning, pending)
        model.recordRecoveryObservation(RecoveryObservation(account: account, pending: false, digest: nil,
            observedAtMs: now + 1, stale: false), for: account)
        XCTAssertFalse(model.recoveryWarning?.pending ?? true)
    }

    func testWarningSurvivesAppBackgroundButProtectedDataLossClearsVisibleMetadata() async {
        let model = model(), now = UInt64(Date.now.timeIntervalSince1970 * 1000) + 1000
        model.recordRecoveryObservation(RecoveryObservation(account: account, pending: true, digest: "one",
            observedAtMs: now, stale: false), for: account)
        model.background()
        XCTAssertTrue(model.recoveryWarning?.pending == true)
        XCTAssertTrue(model.concealed)
        model.protectedDataUnavailable()
        XCTAssertNil(model.recoveryWarning)
        XCTAssertNil(model.session)
    }

    func testFreshnessExpiresWithoutAReplacementObservation() {
        let observation = RecoveryObservation(account: account, pending: false, digest: nil,
            observedAtMs: 1_000_000, stale: false)
        XCTAssertFalse(observation.isStale(at: Date(timeIntervalSince1970: 1120)))
        XCTAssertTrue(observation.isStale(at: Date(timeIntervalSince1970: 1121)))
    }
}
#endif
