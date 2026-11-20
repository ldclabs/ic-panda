import AppIntents
import AuthenticationServices
import CryptoKit
import Security
import XCTest
@testable import dMsg

final class SystemExportsTests: XCTestCase {
    func testLegacyCredentialAuthenticationFailureKeepsTheSnapshotRetryable() throws {
        try XCTSkipUnless(try KeychainMigration.signingPrefix() == KeychainMigration.originalPrefix,
                          "Requires the LDC team Keychain entitlement")
        let store = try SystemExports(.credentials, root: root())
        let prepared = try store.prepareCredentials([
            CredentialInput(title: "Example", username: "alice", password: "saved-secret", origin: "https://example.com")
        ], account: "migration-test", expires: UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000)
        _ = try activate(prepared, in: store)
        let legacy: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "net.dmsg.export.credentials", kSecAttrAccount as String: prepared.header.id,
            kSecAttrAccessGroup as String: KeychainMigration.originalPrefix + ExportPurpose.credentials.keyGroupSuffix,
            kSecAttrSynchronizable as String: false]
        var destination = legacy
        destination[kSecAttrAccessGroup as String] = ExportPurpose.credentials.group
        defer {
            SecItemDelete(legacy as CFDictionary)
            SecItemDelete(destination as CFDictionary)
        }
        var item = legacy
        item[kSecValueData as String] = prepared.key.withUnsafeBytes { Data($0) }
        // Inject the OS authentication result below; the simulator cannot
        // exercise a real passcode/biometry prompt on a protected item.
        item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        XCTAssertEqual(SecItemAdd(item as CFDictionary, nil), errSecSuccess)

        for (status, expected) in [(errSecUserCanceled, ExportAccessError.authenticationCancelled),
                                   (errSecAuthFailed, .authenticationFailed),
                                   (errSecInteractionNotAllowed, .authenticationFailed)] {
            var attempted = false
            XCTAssertThrowsError(try store.authenticate(migrateItem: { _, _ in
                attempted = true
                return status
            })) { XCTAssertEqual($0 as? ExportAccessError, expected) }
            XCTAssertTrue(attempted)
            XCTAssertEqual(try store.header().id, prepared.header.id)
            XCTAssertEqual(SecItemCopyMatching(legacy as CFDictionary, nil), errSecSuccess)
            XCTAssertEqual(SecItemCopyMatching(destination as CFDictionary, nil), errSecItemNotFound)
        }

        let retried = try store.authenticate()
        let entry = try XCTUnwrap(store.credentialMetadata(retried).first)
        XCTAssertEqual(try store.credential(entry.id, opened: retried), "saved-secret")
        XCTAssertEqual(SecItemCopyMatching(legacy as CFDictionary, nil), errSecItemNotFound)
        XCTAssertEqual(SecItemCopyMatching(destination as CFDictionary, nil), errSecSuccess)
    }

    func testLegacyFileSessionMovesOnceAndLockCannotResurrectIt() throws {
        try XCTSkipUnless(try KeychainMigration.signingPrefix() == KeychainMigration.originalPrefix,
                          "Requires the LDC team Keychain entitlement")
        let directory = try root(), source = directory.appending(path: "sample.bin")
        try Data([1]).write(to: source)
        let store = try SystemExports(.files, root: directory)
        let input = FileExportInput(url: source, name: "sample.bin", sha256: try IncomingShares.digest(source), size: 1)
        let prepared = try store.prepareFiles([input], account: "migration-test", expires: UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000)
        let opened = try activate(prepared, in: store)
        let oldGroup = KeychainMigration.originalPrefix + ExportPurpose.files.keyGroupSuffix
        let service = "net.dmsg.export.files"
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service, kSecAttrAccount as String: "file-session",
            kSecAttrAccessGroup as String: oldGroup, kSecAttrSynchronizable as String: false]
        defer { store.lockFiles(); SecItemDelete(query as CFDictionary) }
        let bytes = prepared.key.withUnsafeBytes { Data($0) }
        let session = SystemExports.FileSession(snapshot: opened.header.id, key: bytes, expires: opened.header.expires)
        var item = query
        item[kSecValueData as String] = try JSONEncoder().encode(session)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        XCTAssertEqual(SecItemAdd(item as CFDictionary, nil), errSecSuccess)

        let migrated = try store.fileSession()
        XCTAssertEqual(migrated.key.withUnsafeBytes { Data($0) }, bytes)
        XCTAssertEqual(SecItemCopyMatching(query as CFDictionary, nil), errSecItemNotFound)
        store.lockFiles()
        XCTAssertThrowsError(try store.fileSession())

        try store.unlockFiles(opened)
        XCTAssertEqual(try store.fileSession().key.withUnsafeBytes { Data($0) }, bytes)
        XCTAssertEqual(SecItemCopyMatching(query as CFDictionary, nil), errSecItemNotFound)
    }

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

    func testFailedPublicationDiscardsOnlyTheUnpublishedFiles() throws {
        let store = try SystemExports(.files, root: root(), keyGroup: "unavailable-test-group")
        let expires = UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000
        let source = store.root.appending(path: "test.txt")
        try Data("sample".utf8).write(to: source)
        let files = [FileExportInput(url: source, name: "test.txt", sha256: try IncomingShares.digest(source), size: 6)]
        let current = try store.prepareFiles(files, account: "fixture", expires: expires)
        let opened = try activate(current, in: store)
        let next = try store.prepareFiles(files, account: "fixture", expires: expires)
        XCTAssertThrowsError(try store.publish(next, biometryOnly: false))
        XCTAssertFalse(FileManager.default.fileExists(atPath: next.directory.path))
        XCTAssertThrowsError(try store.unlockFiles(opened))
        store.discard(current)
        XCTAssertEqual(try store.header().id, current.header.id)
        XCTAssertEqual(try store.fileMetadata(opened).map(\.name), ["test.txt"])
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

    func testRememberedLoginsFollowStableItemsAndStayScopedToAccountAndService() throws {
        let directory = try root()
        let store = try SystemExports(.credentials, root: directory, keyGroup: "test")
        let inputs = ["personal", "work"].map { CredentialInput(title: "Example", username: $0, password: "fixture", origin: "https://example.com", itemID: $0) }
        let first = try activate(store.prepareCredentials(inputs, account: "one", digest: "first"), in: store)
        for id in ["personal", "work", "personal"] { try store.remember(id, service: "app:example", opened: first) }
        let next = try activate(store.prepareCredentials(inputs.reversed(), account: "one", digest: "second"), in: store)
        let account = next.header.accountHash
        XCTAssertEqual(try store.recentLogins(account: account, service: "app:example"), ["personal", "work"])
        XCTAssertEqual(try store.recentLogins(account: account, service: "https://example.com"), [])
        XCTAssertEqual(try store.recentLogins(account: "other", service: "app:example"), [])
        XCTAssertEqual(try store.recentLogins(account: account, service: nil), [])
        XCTAssertThrowsError(try store.remember("work", service: "app:example", opened: first))
        let entries = try store.credentialMetadata(next)
        XCTAssertEqual(CredentialExport.requested(account + ":work", user: "work", in: next.header, entries: entries)?.username, "work")
        XCTAssertNil(CredentialExport.requested(account + ":work", user: "previous-user", in: next.header, entries: entries))
        XCTAssertNil(CredentialExport.requested("other:work", user: "work", in: next.header, entries: entries))
        try store.withdrawCredentials(ifCurrent: first.header)
        XCTAssertEqual(try store.header().id, next.header.id)
        try store.withdrawCredentials(ifCurrent: next.header)
        XCTAssertThrowsError(try store.credentialMetadata(next))
        XCTAssertEqual(try store.recentLogins(account: account, service: "app:example"), ["personal", "work"])
    }

    func testPickerShowsMultipleAccountsAndStillSearchesWithoutAService() throws {
        let entries = ["work", "personal", "other"].map { CredentialExport(id: $0, title: "Login", username: $0, origin: "https://example.com", itemID: $0) }
        let remembered = CredentialExport.sections(entries: entries, services: ["example.com"], recent: ["personal", "work"], query: "")
        XCTAssertEqual(remembered.map { $0.entries.map(\.username) }, [["personal", "work"], ["other"]])
        let unknown = CredentialExport.sections(entries: entries, services: [], recent: [], query: "WORK")
        XCTAssertEqual(unknown.flatMap(\.entries).map(\.username), ["work"])
        let site = ASCredentialServiceIdentifier(identifier: "example.com", type: .domain)
        XCTAssertEqual(site.loginServiceKey, ASCredentialServiceIdentifier(identifier: "https://EXAMPLE.com:443/login", type: .URL).loginServiceKey)
        XCTAssertNotEqual(site.loginServiceKey, ASCredentialServiceIdentifier(identifier: "https://unrelated.example.com", type: .URL).loginServiceKey)
        XCTAssertNil(ASCredentialServiceIdentifier(identifier: "", type: .domain).loginServiceKey)
        XCTAssertNil(ASCredentialServiceIdentifier(identifier: "http://example.com", type: .URL).loginServiceKey)
        if #available(iOS 26.2, *) {
            XCTAssertEqual(ASCredentialServiceIdentifier(identifier: "TEAM.sample", type: .app).loginServiceKey, "app:TEAM.sample")
        }
    }

    func testPreparingCatalogThroughAnAliasedDirectory() throws {
        let parent = try root(), target = parent.appending(path: "group", directoryHint: .isDirectory)
        let alias = parent.appending(path: "alias", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: target, withIntermediateDirectories: false)
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: target)
        let store = try SystemExports(.credentials, root: alias, keyGroup: "test")
        let empty = try store.prepareCredentials([], account: "one", digest: "empty")
        let opened = try activate(empty, in: store)
        XCTAssertTrue(try store.credentialMetadata(opened).isEmpty)
        XCTAssertEqual(try store.header().count, 0)
        let escaped = ExportHeader(id: UUID().uuidString, purpose: .credentials, accountHash: "one", expires: UInt64.max)
        try FileManager.default.createSymbolicLink(at: store.root.appending(path: escaped.id), withDestinationURL: parent)
        try JSONEncoder().encode(escaped).write(to: store.root.appending(path: "current.json"))
        XCTAssertThrowsError(try store.header())
    }

    func testAnOldSystemSuggestionFallsBackToCurrentCredentialsAfterRenewal() throws {
        let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
        let expires = UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000
        let original = try store.prepareCredentials([
            CredentialInput(title: "Example", username: "alice", password: "old-secret", origin: "https://example.com")
        ], account: "one", expires: expires)
        let old = try activate(original, in: store), oldEntries = try store.credentialMetadata(old)
        let suggestion = old.header.id + ":" + oldEntries[0].id
        XCTAssertEqual(CredentialExport.requested(suggestion, user: "alice", in: old.header, entries: oldEntries)?.id, oldEntries[0].id)
        let replacement = try store.prepareCredentials([
            CredentialInput(title: "Example", username: "alice", password: "current-secret", origin: "https://example.com")
        ], account: "one", expires: expires)
        let current = try activate(replacement, in: store), entries = try store.credentialMetadata(current)
        XCTAssertNil(CredentialExport.requested(suggestion, user: "alice", in: current.header, entries: entries))
        XCTAssertEqual(entries.count, 1, "The current picker remains available instead of reporting expiry")
        XCTAssertEqual(try store.credential(entries[0].id, opened: current), "current-secret")
        XCTAssertThrowsError(try store.credential(oldEntries[0].id, opened: old))
    }

    @MainActor
    func testFillOnceAfterAMinuteUsesTheFirstConfirmationExactlyOnce() async throws {
        let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
        let input = CredentialInput(title: "Example", username: "work", password: "work-password", origin: "", itemID: "work")
        let opened = try activate(store.prepareCredentials([input], account: "vault", digest: "1"), in: store)
        var time = ContinuousClock.now
        var unlocks = 0
        var fills: [ASPasswordCredential] = []
        let picker = CredentialPickerSession(store: { store }, authenticate: { store in
            unlocks += 1
            return (opened, try store.credentialMetadata(opened))
        }, now: { time }) { credential, identity in
            XCTAssertNil(identity)
            fills.append(credential)
        }

        picker.start(services: [])
        await picker.operation?.value
        picker.query = "work"
        let selected = try XCTUnwrap(picker.sections.first?.entries.first)
        picker.choose(selected)
        picker.back()
        XCTAssertEqual(picker.query, "work")
        picker.choose(selected)
        XCTAssertFalse(picker.canRemember(selected))
        time += .seconds(61)
        picker.confirm(remember: false)
        picker.confirm(remember: false)
        await picker.operation?.value
        picker.choose(selected)
        picker.confirm(remember: false)
        XCTAssertEqual(unlocks, 2)
        XCTAssertEqual(fills.map(\.user), ["work"])
        XCTAssertEqual(fills.first?.password, "work-password")
        guard case .finished = picker.page else { return XCTFail("A delivered request cannot reopen the picker") }
    }

    @MainActor
    func testConfirmedAccountSurvivesPasswordRotationAndRemembersBothAccounts() async throws {
        let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
        let service = ASCredentialServiceIdentifier(identifier: "example.org", type: .domain)
        let key = try XCTUnwrap(service.loginServiceKey)
        let input = { (user: String, password: String) in
            CredentialInput(title: "Example", username: user, password: password, origin: "https://example.org", itemID: user)
        }
        let personal = input("personal", "my-password")
        var opened = try activate(store.prepareCredentials([personal, input("work", "before")], account: "vault", digest: "1"), in: store)
        try store.remember("personal", service: key, opened: opened)
        var fills: [ASPasswordCredential] = []
        var identity: ASPasswordCredentialIdentity?
        let picker = CredentialPickerSession(store: { store }, authenticate: { store in
            (opened, try store.credentialMetadata(opened))
        }) { credential, suggestion in fills.append(credential); identity = suggestion }
        picker.start(services: [service])
        await picker.operation?.value
        picker.query = "work"
        picker.choose(try XCTUnwrap(picker.sections.first?.entries.first))
        opened = try activate(store.prepareCredentials([personal, input("work", "rotated")], account: "vault", digest: "2"), in: store)
        picker.confirm(remember: true)
        await picker.operation?.value
        XCTAssertEqual(fills.map(\.password), ["rotated"])
        XCTAssertEqual(try store.recentLogins(account: opened.header.accountHash, service: key), ["work", "personal"])

        let suggestion = try XCTUnwrap(identity)
        picker.start(services: [service], requested: suggestion)
        await picker.operation?.value
        XCTAssertEqual(fills.map(\.user), ["work", "work"])
        picker.start(services: [service])
        await picker.operation?.value
        XCTAssertEqual(picker.sections.first?.entries.map(\.username), ["work", "personal"])
        picker.choose(try XCTUnwrap(picker.sections.first?.entries.last))
        await picker.operation?.value
        XCTAssertEqual(fills.last?.password, "my-password")
    }

    @MainActor
    func testReplacementCannotRedirectAConfirmedLogin() async throws {
        for (account, username, origin, itemID) in [("other", "work", "https://example.org", "1"),
            ("vault", "personal", "https://example.org", "1"), ("vault", "work", "https://other.org", "1"),
            ("vault", "work", "https://example.org", "2")] {
            let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
            let original = CredentialInput(title: "Example", username: "work", password: "first", origin: "https://example.org", itemID: "1")
            var opened = try activate(store.prepareCredentials([original], account: "vault", digest: "1"), in: store)
            let picker = CredentialPickerSession(store: { store }, authenticate: { store in
                (opened, try store.credentialMetadata(opened))
            }) { _, _ in XCTFail("A changed account, item or displayed login requires a new choice") }
            picker.start(services: [])
            await picker.operation?.value
            picker.choose(try XCTUnwrap(picker.sections.first?.entries.first))
            let replacement = CredentialInput(title: "Example", username: username, password: "second", origin: origin, itemID: itemID)
            opened = try activate(store.prepareCredentials([replacement], account: account, digest: "2"), in: store)
            picker.confirm(remember: false)
            await picker.operation?.value
            guard case .list = picker.page else { return XCTFail("The refreshed picker should be visible") }
        }
    }

    @MainActor
    func testDismissalDuringReauthenticationDropsTheConfirmedLogin() async throws {
        let store = try SystemExports(.credentials, root: root(), keyGroup: "test")
        let input = CredentialInput(title: "Example", username: "work", password: "unused", origin: "", itemID: "work")
        let opened = try activate(store.prepareCredentials([input], account: "vault", digest: "1"), in: store)
        var time = ContinuousClock.now
        let start = time
        let started = expectation(description: "Reauthentication requested")
        var resume: CheckedContinuation<Void, Never>?
        let picker = CredentialPickerSession(store: { store }, authenticate: { store in
            if time > start {
                await withCheckedContinuation { resume = $0; started.fulfill() }
            }
            return (opened, try store.credentialMetadata(opened))
        }, now: { time }) { _, _ in XCTFail("A cancelled request must never return a password") }
        picker.start(services: [])
        await picker.operation?.value
        picker.choose(try XCTUnwrap(picker.sections.first?.entries.first))
        time += .seconds(61)
        picker.confirm(remember: false)
        let operation = try XCTUnwrap(picker.operation)
        await fulfillment(of: [started], timeout: 3)
        picker.close()
        resume?.resume()
        await operation.value
        guard case .finished = picker.page else { return XCTFail("Dismissal is final") }
    }
}

extension SystemExportsTests {
    @MainActor
    func testWorkspaceIntentsRouteAndStageWithoutPickingAnAccount() async throws {
        let navigation = IntentNavigation.shared, previous = navigation.destination
        defer { navigation.destination = previous }
        let open = OpenWorkspaceIntent()
        open.destination = .approvals
        _ = try await open.perform()
        XCTAssertEqual(navigation.destination, .approvals)
        let url = try root().appending(path: UUID().uuidString + ".txt")
        try Data("shortcut test".utf8).write(to: url)
        let store = try IncomingShares()
        let intent = ImportIntoDMsgIntent()
        intent.files = [IntentFile(fileURL: url)]
        _ = try await intent.perform()
        let staged = try XCTUnwrap(store.list().first { $0.name == url.lastPathComponent })
        defer { try? store.remove(staged) }
        XCTAssertNil(staged.accountHash)
        XCTAssertEqual(navigation.destination, .imports)
        XCTAssertEqual(try String(contentsOf: store.file(staged), encoding: .utf8), "shortcut test")
    }

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
