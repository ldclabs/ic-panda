import CryptoKit
import XCTest
@testable import dMsg

final class SystemExportsTests: XCTestCase {
    func testALoginMatchesItsSiteAndSubdomainsButNotALookalike() {
        let login = CredentialExport(id: "1", title: "GitHub", username: "lin", origin: "https://www.github.com/login")
        for service in ["github.com", "https://github.com", "https://gist.github.com/x", "www.github.com"] {
            XCTAssertTrue(login.matches(service: service), service)
        }
        for service in ["notgithub.com", "github.com.example.org", "", "example.org"] {
            XCTAssertFalse(login.matches(service: service), service)
        }
        XCTAssertTrue(login.contains("hub")); XCTAssertFalse(login.contains("gitlab"))
    }


    private func root() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }
    private func activate(_ prepared: PreparedExport, in store: SystemExports) throws -> OpenExport {
        try JSONEncoder().encode(prepared.header).write(to: store.root.appending(path: "current.json"))
        return OpenExport(header: prepared.header, key: prepared.key)
    }
    func testCredentialCopiesBindSnapshotAccountAndEntry() throws {
        let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
        let prepared = try store.prepareCredentials([CredentialInput(title: "Example", username: "alice", password: "selected-secret", origin: "https://example.com")], account: "account-one", expires: UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000)
        let opened = try activate(prepared, in: store), entry = try XCTUnwrap(store.credentialMetadata(opened).first)
        XCTAssertEqual(try store.credential(entry.id, opened: opened), "selected-secret")
        let encrypted = try Data(contentsOf: prepared.directory.appending(path: entry.id))
        XCTAssertNil(String(data: encrypted, encoding: .utf8)?.range(of: "selected-secret"))
        let changed = ExportHeader(id: opened.header.id, purpose: .credentials, accountHash: IncomingShares.accountHash("account-two"), expires: opened.header.expires)
        XCTAssertThrowsError(try store.credential(entry.id, opened: OpenExport(header: changed, key: opened.key)))
        XCTAssertThrowsError(try store.credential("../manifest", opened: opened))
        let newer = try store.prepareCredentials([CredentialInput(title: "Other", username: "bob", password: "new-secret", origin: "")], account: "account-two", expires: opened.header.expires)
        _ = try activate(newer, in: store)
        XCTAssertThrowsError(try store.credential(entry.id, opened: opened))
    }
    func testFileCopyVerifiesDigestAndCleansPartialOutput() throws {
        let directory = try root(), source = directory.appending(path: "source")
        let bytes = Data(repeating: 42, count: 1_500_000); try bytes.write(to: source)
        let store = try SystemExports(.files, root: directory, keyGroup: "test")
        let input = FileExportInput(url: source, name: "example.bin", sha256: try IncomingShares.digest(source), size: UInt64(bytes.count))
        let prepared = try store.prepareFiles([input], account: "one", expires: UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000)
        let opened = try activate(prepared, in: store), file = try XCTUnwrap(store.fileMetadata(opened).first)
        let output = directory.appending(path: "output")
        try store.materialize(file, opened: opened, output: output, cancelled: { false })
        XCTAssertEqual(try Data(contentsOf: output), bytes)
        try FileManager.default.removeItem(at: output)
        XCTAssertThrowsError(try store.materialize(file, opened: opened, output: output, cancelled: { true }))
        XCTAssertFalse(FileManager.default.fileExists(atPath: output.path))
        try Data("tampered".utf8).write(to: prepared.directory.appending(path: file.id + ".1"))
        XCTAssertThrowsError(try store.materialize(file, opened: opened, output: output, cancelled: { false }))
        XCTAssertFalse(FileManager.default.fileExists(atPath: output.path))
    }

    func testRemovingCredentialSnapshotRevokesAnAlreadyOpenedCopy() throws {
        let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
        let prepared = try store.prepareCredentials([
            CredentialInput(title: "Example", username: "alice", password: "selected-secret", origin: "https://example.com")
        ], account: "account-one", expires: UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000)
        let opened = try activate(prepared, in: store)
        let entry = try XCTUnwrap(store.credentialMetadata(opened).first)
        XCTAssertEqual(try store.credential(entry.id, opened: opened), "selected-secret")

        try store.invalidate()

        XCTAssertThrowsError(try store.header())
        XCTAssertThrowsError(try store.credential(entry.id, opened: opened))
        XCTAssertFalse(FileManager.default.fileExists(atPath: prepared.directory.path))
    }
    func testExpiredAndEscapedCopiesAreRejected() throws {
        let directory = try root(), store = try SystemExports(.credentials, root: directory, keyGroup: "test")
        let expired = ExportHeader(id: UUID().uuidString, purpose: .credentials, accountHash: "one", expires: 1)
        try JSONEncoder().encode(expired).write(to: store.root.appending(path: "current.json"))
        XCTAssertThrowsError(try store.header())
        let shares = try IncomingShares(root: directory), escaped = IncomingShare(id: UUID().uuidString, name: "file", size: 1, sha256: "")
        let outside = directory.appending(path: "outside"); try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: shares.root.appending(path: escaped.id), withDestinationURL: outside)
        XCTAssertThrowsError(try shares.file(escaped))
        XCTAssertThrowsError(try store.prepareFiles([FileExportInput(url: outside, name: "large", sha256: "", size: UInt64.max)], account: "one", expires: 1))
    }

    func testMissingAndExpiredCredentialsHaveDifferentRecoveryStates() throws {
        let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
        XCTAssertThrowsError(try store.header()) { XCTAssertEqual($0 as? ExportAccessError, .notConfigured) }
        let expired = ExportHeader(id: UUID().uuidString, purpose: .credentials, accountHash: IncomingShares.accountHash("one"), expires: 1)
        try JSONEncoder().encode(expired).write(to: store.root.appending(path: "current.json"))
        XCTAssertThrowsError(try store.header()) { XCTAssertEqual($0 as? ExportAccessError, .expired) }
        XCTAssertEqual(try store.header(checkExpiry: false).id, expired.id)
    }

    func testAutoFillRebuildsMissingCopiesAndRenewsWithoutChangingAccounts() throws {
        let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
        let now = UInt64(Date.now.timeIntervalSince1970 * 1000)
        let missing = try XCTUnwrap(store.credentialRefresh(account: "one", now: now))
        XCTAssertNil(missing.snapshot)
        XCTAssertTrue(missing.force, "A previously acknowledged core selection must be requested again")
        let prepared = try store.prepareCredentials([
            CredentialInput(title: "Example", username: "alice", password: "secret", origin: "https://example.com")
        ], account: "one", expires: now + 86_400_000)
        _ = try activate(prepared, in: store)
        XCTAssertEqual(try store.credentialRefresh(account: "one", now: now), .init(snapshot: prepared.header.id, force: false))
        XCTAssertTrue(try XCTUnwrap(store.credentialRefresh(account: "one", now: now + 12 * 3_600_000)).force)
        XCTAssertTrue(try XCTUnwrap(store.credentialRefresh(account: "one", now: now + 86_400_001)).force)
        XCTAssertNil(try store.credentialRefresh(account: "another", now: now), "Never republish another account's copy")
        try store.invalidate()
        XCTAssertTrue(try XCTUnwrap(store.credentialRefresh(account: "one", now: now)).force)
    }

    func testAnOldSystemSuggestionFallsBackToCurrentCredentialsAfterRenewal() throws {
        let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
        let expires = UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000
        let original = try store.prepareCredentials([
            CredentialInput(title: "Example", username: "alice", password: "old-secret", origin: "https://example.com")
        ], account: "one", expires: expires)
        let old = try activate(original, in: store), oldEntries = try store.credentialMetadata(old)
        let suggestion = old.header.id + ":" + oldEntries[0].id
        XCTAssertEqual(CredentialExport.requested(suggestion, in: old.header, entries: oldEntries)?.id, oldEntries[0].id)
        let replacement = try store.prepareCredentials([
            CredentialInput(title: "Example", username: "alice", password: "current-secret", origin: "https://example.com")
        ], account: "one", expires: expires)
        let current = try activate(replacement, in: store), entries = try store.credentialMetadata(current)
        XCTAssertNil(CredentialExport.requested(suggestion, in: current.header, entries: entries))
        XCTAssertEqual(entries.count, 1, "The current picker remains available instead of reporting expiry")
        XCTAssertEqual(try store.credential(entries[0].id, opened: current), "current-secret")
        XCTAssertThrowsError(try store.credential(oldEntries[0].id, opened: old))
    }
}

extension SystemExportsTests {
    func testIncomingTextIsStagedAndInterruptedCopyRemainsDiscardable() throws {
        let directory = try root(), store = try IncomingShares(root: directory)
        let record = try store.stageText("sample text")
        XCTAssertEqual(try String(contentsOf: store.file(record), encoding: .utf8), "sample text")
        XCTAssertEqual(try IncomingShares.digest(store.file(record)), record.sha256)
        let interrupted = IncomingShare(id: UUID().uuidString, name: "unfinished.txt", size: 100, sha256: "")
        try store.write(interrupted)
        XCTAssertEqual(try store.list().count, 2)
        try store.remove(interrupted); XCTAssertEqual(try store.list().count, 1)
        try store.remove(record); XCTAssertTrue(try store.list().isEmpty)
    }
}
