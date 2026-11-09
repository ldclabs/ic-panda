import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class FileTransferTests: XCTestCase {
    func testTransferWatchWaitsForForegroundToUncoverTheWorkspace() async throws {
        let model = SessionModel()
        model.installFixtures(SessionSnapshot(generation: 1, sequence: 1, state: "unlocked",
            account: "040g2081040g2081040g", deviceId: "fixture-device", pendingSync: 0, conflicts: 0,
            approvalsWaiting: 0, fastUnlock: false, loginExpiresAtMs: nil, loginRenewable: false, passkeyAvailable: false, setupPending: false, lastSyncAtMs: nil, syncError: nil), items: [])
        model.lifecycle = .resuming
        var exited = false
        let watch = Task {
            await model.fileTransfers.watch(model)
            exited = true
        }
        defer { watch.cancel() }
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertFalse(exited, "Scene activation can precede the asynchronous foreground resume")
        model.lifecycle = .active
        // With no engine installed, reaching the uncovered iteration finishes.
        await watch.value
        XCTAssertTrue(exited)
    }

    func testRetryDeadlinesAndBlockedFailures() {
        XCTAssertTrue(FileTransfers.due(error: nil, retryAt: nil, now: 0))
        XCTAssertFalse(FileTransfers.due(error: "UNAVAILABLE", retryAt: 2_000, now: 1_999))
        XCTAssertTrue(FileTransfers.due(error: "UNAVAILABLE", retryAt: 2_000, now: 2_000))
        XCTAssertFalse(FileTransfers.due(error: "INTEGRITY_FAILED", retryAt: nil, now: .max))
    }

    func testCompletedBytesRetainTheCoreVerificationStage() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let json = #"{"upload_id":"one","stage":"verifying","done":100,"total":100,"bytes":104857600,"total_bytes":104857600,"error":null,"retry_at":null}"#
        let job = try decoder.decode(FileTransfers.Download.self, from: Data(json.utf8))
        XCTAssertEqual(job.bytes, job.totalBytes)
        XCTAssertEqual(job.stage, "verifying")
        XCTAssertNotEqual(job.stage, "done")
    }
}
