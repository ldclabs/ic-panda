import XCTest
@testable import dMsg

@MainActor
final class FileTransferTests: XCTestCase {
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
