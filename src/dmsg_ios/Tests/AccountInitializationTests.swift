import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class AccountInitializationTests: XCTestCase {
    func testUnlockRequiresAnExistingWorkspaceWithNoPendingSetup() {
        let model = SessionModel()
        for (state, account, pending, fast, renewable, expected) in [
            ("no_account", false, false, false, true, false),
            ("no_account", false, false, true, true, false),
            ("root_required", true, true, true, true, false),
            ("pairing_required", false, false, false, true, false),
            ("locked", true, false, true, false, true),
            ("locked", true, false, false, true, true),
            ("locked", true, false, false, false, false),
            ("root_required", true, false, true, false, true),
            ("logged_out", false, false, false, false, false),
            ("unlocked", true, false, true, true, false),
        ] {
            model.installFixtures(SessionSnapshot(generation: 0, sequence: 0, state: state,
                account: account ? "040g2081040g2081040g" : nil, deviceId: "device",
                pendingSync: 0, conflicts: 0, approvalsWaiting: 0, fastUnlock: fast,
                loginExpiresAtMs: nil, loginRenewable: renewable, passkeyAvailable: true,
                setupPending: pending, lastSyncAtMs: nil, syncError: nil), items: [])
            model.signingIntoAnotherAccount = false
            XCTAssertEqual(model.needsAccountSetup, state == "no_account" || pending, state)
            XCTAssertEqual(model.canUnlockWorkspace, expected, state)
            model.signingIntoAnotherAccount = true
            XCTAssertFalse(model.canUnlockWorkspace, "Switching accounts must not unlock the previous one")
        }
    }

    func testFailedInitializationRefreshesTheNativeHandleBeforeAnotherAttempt() async throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: base) }
        let engine = try await MobileEngine.open(directory: base.appending(path: "account").path,
            staging: base.appending(path: "imports").path, platform: CoreBridgeTests.NoHardware())
        let model = SessionModel()
        try await model.installEngineFixture(engine)

        // A real failed Rust transition advances the generation even without
        // login/network access. A stale handle cannot start the second one.
        for _ in 0..<2 {
            let before = try XCTUnwrap(model.session).generation
            await model.initializeAccount()
            let after = try XCTUnwrap(model.session).generation
            XCTAssertGreaterThan(after, before)
            XCTAssertEqual(after, engine.controlGeneration())
            XCTAssertFalse(model.busy)
            XCTAssertFalse(model.initializingAccount)
            let current = try XCTUnwrap(model.engine)
            let observation = try await current.recoveryWarning()
            XCTAssertEqual(observation.json, "null")
        }

        // Another active operation owns the gate; setup cannot invalidate it.
        let operation = model.beginBusyOperation()
        let before = engine.controlGeneration()
        await model.initializeAccount()
        XCTAssertEqual(engine.controlGeneration(), before)
        XCTAssertTrue(model.busy)
        XCTAssertFalse(model.initializingAccount)
        XCTAssertTrue(model.finishBusyOperation(operation))
        try await engine.close()
    }
}
