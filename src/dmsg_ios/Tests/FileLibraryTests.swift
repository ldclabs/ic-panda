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

    func testFileRequestsDecodeWhatTheCoreSends() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let link = try decoder.decode(FileRequest.self, from: Data("""
        {"request_id":"r1","url":"https://dmsg.net/r/a/r1#k","title":"Tax","instructions":"","label":"Ada","expires_at":5,"max_total_bytes":6,
         "created_at":1,"revoked_at":null,"status":"full","files":2,"bytes":3,"last_stored_at":4,"unread":true,"last_checked_at":null}
        """.utf8))
        XCTAssertEqual(link.id, "r1"); XCTAssertEqual(link.label, "Ada"); XCTAssertTrue(link.active); XCTAssertTrue(link.unread)
        XCTAssertEqual(link.stateTitle, String(localized: "Full"))
        let blind = try decoder.decode(FileRequest.self, from: Data("""
        {"request_id":"r2","url":null,"title":null,"instructions":null,"label":null,"expires_at":5,"max_total_bytes":6,
         "created_at":1,"revoked_at":2,"status":"revoked","files":0,"bytes":0,"last_stored_at":null,"unread":false,"last_checked_at":null}
        """.utf8))
        XCTAssertNil(blind.url); XCTAssertNil(blind.title); XCTAssertFalse(blind.active)
        XCTAssertEqual(blind.stateTitle, String(localized: "Link closed"))

        let file = try decoder.decode(ReceivedFile.self, from: Data("""
        {"request_id":"r1","upload_id":"u1","state":"saved","name":"a.pdf","mime":"application/pdf","size":9,"sender":{"name":"Ada","note":"Hi"},
         "replaces":"u0","sent_at":1,"stored_at":2,"bytes":10,"received_at":3,"withdrawn_at":null,"saved_file":"f1"}
        """.utf8))
        XCTAssertEqual(file.id, "u1"); XCTAssertEqual(file.sender?.name, "Ada"); XCTAssertEqual(file.replaces, "u0"); XCTAssertEqual(file.savedFile, "f1")
        let unreadable = try decoder.decode(ReceivedFile.self, from: Data("""
        {"request_id":"r1","upload_id":"u2","state":"unreadable","name":null,"mime":null,"size":null,"sender":null,
         "replaces":null,"sent_at":null,"stored_at":2,"bytes":10,"received_at":null,"withdrawn_at":null,"saved_file":null}
        """.utf8))
        XCTAssertNil(unreadable.name); XCTAssertNil(unreadable.sender)

        let capacity = try decoder.decode(FileBackupCapacity.self, from: Data(#"{"backup_max_file_bytes":1,"storage_limit":2,"committed":0,"reserved":0,"file_request_limit":10}"#.utf8))
        XCTAssertEqual(capacity.fileRequestLimit, 10)
        let unknown = try decoder.decode(FileBackupCapacity.self, from: Data(#"{"backup_max_file_bytes":null,"storage_limit":null,"committed":0,"reserved":0,"file_request_limit":null}"#.utf8))
        XCTAssertNil(unknown.fileRequestLimit)
        XCTAssertEqual(fileLimit(100 * 1024 * 1024), "100 MiB")
        XCTAssertEqual(fileLimit(1000 * 1024 * 1024), "1000 MiB")
        XCTAssertEqual(fileLimit(2048 * 1024 * 1024), "2 GiB")
    }

    func testFileLinksDecodeAsTheCoreWritesThemAndExplainTheirState() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let made = try decoder.decode(FileLink.self, from: Data(#"""
        {"source":{"type":"file_version"},"share_id":"share","file":"file","version":"version","name":"a.png","size":2048,
         "url":"https://dmsg.net/s/share#key","expires_at":1794000000000,"max_downloads":5,"password":false,
         "created_at":1,"last_checked_at":null,"revoked_at":null,"state":"active","downloads":2}
        """#.utf8))
        XCTAssertEqual(made.id, "share")
        XCTAssertTrue(made.active)
        XCTAssertFalse(made.isSecretSnapshot)
        XCTAssertEqual(made.url, "https://dmsg.net/s/share#key")
        XCTAssertEqual(made.downloadsText, "2 / 5")
        // A link made on another device carries no file, name or URL.
        let elsewhere = try decoder.decode(FileLink.self, from: Data(#"""
        {"source":null,"share_id":"other","file":null,"version":null,"name":null,"size":null,"url":null,
         "expires_at":1,"max_downloads":0,"password":true,"created_at":1,"state":"expired","downloads":9}
        """#.utf8))
        XCTAssertNil(elsewhere.url)
        XCTAssertFalse(elsewhere.active)
        XCTAssertEqual(elsewhere.downloadsText, 9.formatted())
        let snapshot = try decoder.decode(FileLink.self, from: Data(#"""
        {"source":{"type":"vault_snapshot"},"share_id":"secret","expires_at":1,"max_downloads":1,"password":false,"state":"active","downloads":0}
        """#.utf8))
        XCTAssertTrue(snapshot.isSecretSnapshot)
        let titles = ["active", "revoked", "expired", "exhausted"].map { state in
            FileLink(shareId: "s", expiresAt: 1, maxDownloads: 0, password: false, state: state, downloads: 0).stateTitle
        }
        XCTAssertEqual(Set(titles).count, 4)
    }

    func testFileLinkTargetsNameTheirSourceAndTheSizeLimit() {
        func file(size: UInt64) -> LibraryFile {
            LibraryFile(id: "file", name: "a.png", mime: "image/png", size: size, current: "version", sha256: "", status: "local", versions: 1)
        }
        let local = FileLinkTarget(file: file(size: FileLinkTarget.maxBytes), remote: false)
        XCTAssertFalse(local.tooLarge)
        XCTAssertFalse(local.remote)
        XCTAssertEqual(local.id, "version:file:version")
        XCTAssertTrue(FileLinkTarget(file: file(size: FileLinkTarget.maxBytes + 1), remote: true).tooLarge)
        let backup = CloudBackup(uploadId: "upload", name: "b.png", size: 10, bytes: 20, needsRoot: false, local: false)
        let remote = FileLinkTarget(backup: backup)
        XCTAssertTrue(remote.remote)
        XCTAssertEqual(remote.id, "backup:upload")
        XCTAssertFalse(remote.tooLarge)
    }

    func testTransferAtFullBytesRetainsVerificationAndCommitMeaning() {
        XCTAssertNotEqual(FileTransferProgressView.title(stage: "verifying", failed: false), FileTransferProgressView.title(stage: "done", failed: false))
        XCTAssertNotEqual(FileTransferProgressView.title(stage: "committing", failed: false), FileTransferProgressView.title(stage: "done", failed: false))
        XCTAssertNotEqual(FileTransferProgressView.title(stage: "downloading", failed: false), FileTransferProgressView.title(stage: "downloading", failed: true))
    }
}
