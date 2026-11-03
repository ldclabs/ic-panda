import XCTest
import DMsgCore
@testable import dMsg

@MainActor
final class FileLibraryTests: XCTestCase {
    func testFileFailuresPreserveSessionRevocationAndIgnoreCancellation() {
        let model = SessionModel()
        model.installFixtures(SessionSnapshot(generation: 1, sequence: 1, state: "unlocked", account: "test-account", deviceId: "test-device",
            pendingSync: 0, conflicts: 0, approvalsWaiting: 0, fastUnlock: false, setupPending: false, lastSyncAtMs: nil, syncError: nil), items: [])
        XCTAssertNil(model.fileFailureMessage(CocoaError(.userCancelled)))
        XCTAssertNil(model.fileFailureMessage(MobileError.Failure(code: "CANCELLED")))
        XCTAssertTrue(model.unlocked)
        XCTAssertNotNil(model.fileFailureMessage(MobileError.Failure(code: "UNAVAILABLE")))
        let epoch = model.epoch
        model.fileOperations.insert("restore:test")
        XCTAssertNil(model.fileFailureMessage(MobileError.Failure(code: "DEVICE_NOT_APPROVED")))
        XCTAssertFalse(model.unlocked)
        XCTAssertGreaterThan(model.epoch, epoch)
        XCTAssertTrue(model.cloudBackups.isEmpty)
        XCTAssertTrue(model.fileOperations.isEmpty)
    }

    func testBackupAdmissionDoesNotReportSkippedWorkAsQueued() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let queued = try decoder.decode(BackupAdmission.self, from: Data(#"{"started":[{"upload_id":"new"}],"skipped":[]}"#.utf8))
        XCTAssertTrue(queued.accepted)
        for (reason, accepted) in [("backed_up", true), ("in_progress", true), ("too_large", false), ("not_local", false), ("future_reason", false)] {
            let result = try decoder.decode(BackupAdmission.self, from: Data("{\"started\":[],\"skipped\":[{\"reason\":\"\(reason)\"}]}".utf8))
            XCTAssertEqual(result.accepted, accepted)
            XCTAssertNotEqual(result.message, queued.message, "\(reason) must not claim that a new transfer was queued")
        }
        XCTAssertFalse(BackupAdmission(started: [], skipped: []).accepted)
    }

    func testHistoricalAvailabilityUsesTheSelectedVersion() {
        let file = LibraryFile(id: "file", name: "report.pdf", mime: "application/pdf", size: 200,
            current: "new", sha256: "new-hash", status: "backed_up", versions: 2, local: true, folder: "folder")
        let version = LibraryVersion(id: "old", size: 100, sha256: "old-hash", createdAt: 20,
            current: false, local: false, status: "backed_up")
        let historical = version.file(in: file)
        XCTAssertFalse(historical.canOpen)
        XCTAssertEqual(historical.current, "old")
        XCTAssertEqual(historical.sha256, "old-hash")
        XCTAssertEqual(historical.folder, "folder")
        XCTAssertEqual(historical.size, 100)
    }

    func testCloudSearchFiltersBothNamesAndOriginalFolders() {
        let a = CloudBackup(uploadId: "a", name: "Notes.pdf", size: 20, bytes: 40, needsRoot: false, local: false, path: ["Projects"], committedAt: 10)
        let b = CloudBackup(uploadId: "b", name: "Notes.pdf", size: 10, bytes: 20, needsRoot: false, local: true, path: ["Personal"], committedAt: 20)
        XCTAssertEqual(FileSort.name.backups([a,b], query: " PROJECTS ").map(\.id), ["a"])
        XCTAssertEqual(FileSort.date.backups([a,b], query: "NOTES").map(\.id), ["b","a"])
        XCTAssertEqual(FileSort.size.backups([a,b], query: "").map(\.id), ["a","b"])
        XCTAssertTrue(FileSort.name.backups([a,b], query: "not found").isEmpty)
    }

    func testMissingDatesStillUseAStableNameOrder() {
        let a = LibraryFile(id: "a", name: "a.txt", mime: "text/plain", size: 1, current: "v", sha256: "", status: "local", versions: 1)
        let b = LibraryFile(id: "b", name: "b.txt", mime: "text/plain", size: 1, current: "v", sha256: "", status: "local", versions: 1, updatedAt: 0)
        XCTAssertEqual(FileSort.date.files([b,a]).map(\.id), ["a","b"])
    }

    func testCapacityNeverUnderflowsWhenReservationsExceedSpace() {
        XCTAssertNil(FileBackupCapacity(backupMaxFileBytes: nil, storageLimit: nil, committed: 10, reserved: 10).remaining)
        XCTAssertEqual(FileBackupCapacity(backupMaxFileBytes: 100, storageLimit: 100, committed: 80, reserved: 30).remaining, 0)
        XCTAssertEqual(FileBackupCapacity(backupMaxFileBytes: 100, storageLimit: 100, committed: 60, reserved: 10).remaining, 30)
    }

    func testFileErrorsDoNotReferToSecretSnapshotLimits() {
        let failure = fileFailure(MobileError.Failure(code: "QUOTA_EXCEEDED"))
        XCTAssertFalse(failure.contains("8 MiB"))
        XCTAssertNotEqual(fileFailureCode("INTEGRITY_FAILED"), fileFailureCode("NETWORK_UNAVAILABLE"))
    }

    func testNamesRejectPathsControlsAndOverlongUnicode() {
        for name in ["", "  ", ".", "..", "a/b", "a\\b", "a\nb", "a\u{7f}", String(repeating: "文", count: 256)] {
            XCTAssertFalse(FileNameEditor.valid(name), name)
        }
        XCTAssertTrue(FileNameEditor.valid(" 项目 notes.txt "))
        XCTAssertTrue(FileNameEditor.valid(String(repeating: "文", count: 255)))
    }

    func testTransferAtFullBytesRetainsVerificationAndCommitMeaning() {
        XCTAssertNotEqual(FileTransferProgressView.title(stage: "verifying", failed: false), FileTransferProgressView.title(stage: "done", failed: false))
        XCTAssertNotEqual(FileTransferProgressView.title(stage: "committing", failed: false), FileTransferProgressView.title(stage: "done", failed: false))
        XCTAssertNotEqual(FileTransferProgressView.title(stage: "downloading", failed: false), FileTransferProgressView.title(stage: "downloading", failed: true))
    }
}
