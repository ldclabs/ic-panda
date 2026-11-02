import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class ContentSyncTests: XCTestCase {
    func testFailureDeadlineOverridesTheNormalOneMinuteInterval() {
        let failed = ContentSyncStatus.fixture(error: "NETWORK_UNAVAILABLE", retry: 5_000)
        XCTAssertFalse(ContentSyncPolicy.isDue(status: failed, edited: true, idleDue: true, now: 4_999))
        XCTAssertTrue(ContentSyncPolicy.isDue(status: failed, edited: false, idleDue: false, now: 5_000))
        let blocked = ContentSyncStatus.fixture(error: "INTEGRITY_FAILED")
        XCTAssertFalse(ContentSyncPolicy.isDue(status: blocked, edited: true, idleDue: true, now: 100_000))
        XCTAssertTrue(ContentSyncPolicy.isDue(status: .fixture(), edited: false, idleDue: true, now: 0))
        XCTAssertFalse(ContentSyncPolicy.isDue(status: .fixture(running: true), edited: true, idleDue: true, now: 0))
    }

    func testNavigationKeepsTheRunningTokenButBackgroundCancelsIt() async throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: base) }
        let engine = try await MobileEngine.open(directory: base.appending(path: "account").path,
            staging: base.appending(path: "imports").path, platform: CoreBridgeTests.NoHardware())
        let model = SessionModel()
        await model.runAutomaticSync { token in
            // Check the real Rust cancellation token after the same events as
            // tab changes, privateScreen.onAppear and editing.
            for tab in AppTab.allCases {
                model.selectedTab = tab
                model.activity()
                await Task.yield()
            }
            do { _ = try await engine.sync(automatic: true, operation: token); XCTFail("Locked core accepted sync") }
            catch MobileError.Failure(let code) { XCTAssertNotEqual(code, "CANCELLED") }
            model.background()
            do { _ = try await engine.sync(automatic: true, operation: token); XCTFail("Background did not cancel") }
            catch MobileError.Failure(let code) { XCTAssertEqual(code, "CANCELLED") }
        }
        await model.runAutomaticSync { token in
            do { _ = try await engine.sync(automatic: true, operation: token); XCTFail("Locked core accepted sync") }
            catch MobileError.Failure(let code) { XCTAssertNotEqual(code, "CANCELLED", "New visit reuses no cancelled token") }
        }
        try await engine.close()
    }
}
