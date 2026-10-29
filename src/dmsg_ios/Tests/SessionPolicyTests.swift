import XCTest
@testable import dMsg

@MainActor
final class SessionPolicyTests: XCTestCase {
    func testAutomaticUnlockWaitsForALockedReadySession() {
        var policy = AutomaticUnlockPolicy()
        XCTAssertFalse(policy.consume(locked: false, fastUnlock: true, busy: false))
        XCTAssertFalse(policy.consume(locked: true, fastUnlock: false, busy: false))
        XCTAssertFalse(policy.consume(locked: true, fastUnlock: true, busy: true))
        XCTAssertTrue(policy.consume(locked: true, fastUnlock: true, busy: false))
    }

    func testAutomaticUnlockDoesNotRetryCancelledAuthenticationUntilAnotherVisit() {
        var policy = AutomaticUnlockPolicy()
        XCTAssertTrue(policy.consume(locked: true, fastUnlock: true, busy: false))
        // A cancelled prompt leaves the same eligible session. Its inactive /
        // active transition must not present another prompt.
        XCTAssertFalse(policy.consume(locked: true, fastUnlock: true, busy: false))
        policy.rearm()
        XCTAssertTrue(policy.consume(locked: true, fastUnlock: true, busy: false))
        XCTAssertFalse(policy.consume(locked: true, fastUnlock: true, busy: false))
    }

    func testExplicitAuthenticationOrLockSuppressesAutomaticUnlockForThisVisit() {
        var policy = AutomaticUnlockPolicy()
        policy.suppress()
        XCTAssertFalse(policy.consume(locked: true, fastUnlock: true, busy: false))
        policy.rearm()
        XCTAssertTrue(policy.consume(locked: true, fastUnlock: true, busy: false))
    }

    func testBackgroundConcealsWithoutDestroyingAnotherAppsClipboard() async {
        let model = SessionModel()
        model.clipboard.copy("test-secret")
        UIPasteboard.general.string = "another app"
        model.background()
        XCTAssertTrue(model.concealed)
        await model.lock()
        XCTAssertEqual(UIPasteboard.general.string, "another app")
        XCTAssertTrue(model.vault.isEmpty)
        XCTAssertTrue(model.channels.isEmpty)
    }

    func testExplicitLockInvalidatesPriorUIRequests() async {
        let model = SessionModel()
        let original = model.epoch
        await model.lock()
        XCTAssertGreaterThan(model.epoch, original)
        XCTAssertFalse(model.unlocked)
    }

    func testExactDecimalDisplayDoesNotRoundAmounts() throws {
        let value = try JSONDecoder().decode(DisplayValue.self, from: Data("123456789012345678901234567890".utf8))
        XCTAssertEqual(value.text, "123456789012345678901234567890")
    }

    func testInboxDecodesCoreWireNamesWithoutDisplayingSignedEnvelope() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let json = #"{"order_id":"order","sender":"sender","state":"visible","text":"hello","created_at":123,"read":false,"archived":false,"signed":"wire-envelope","abort_reason":null}"#
        let letter = try decoder.decode(InboxLetter.self, from: Data(json.utf8))
        XCTAssertEqual(letter.id, "order")
        XCTAssertEqual(letter.createdAt, 123)
        XCTAssertEqual(letter.text, "hello")
        XCTAssertFalse(letter.read)
    }

    func testStagingPreservesInterruptedImports() throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: base) }
        let paths = try PrivateFiles(base: base)
        let folder = paths.staging.appending(path: "040g2081040g2081040g").appending(path: "dmsg-import-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let pending = folder.appending(path: "pending.txt")
        try Data("pending".utf8).write(to: pending)
        _ = try PrivateFiles(base: base)
        XCTAssertTrue(FileManager.default.fileExists(atPath: pending.path))
        XCTAssertThrowsError(try paths.finishImport(base.appending(path: "outside.txt")))
        try paths.finishImport(pending)
        XCTAssertFalse(FileManager.default.fileExists(atPath: pending.path))
    }

    func testImmediateBackgroundLockPreferenceSurvivesRestart() throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: base) }
        let paths = try PrivateFiles(base: base)
        try LocalPreferences(immediateBackgroundLock: true).write(at: paths.root)
        XCTAssertTrue(try LocalPreferences.read(at: paths.root).immediateBackgroundLock)
        XCTAssertTrue(try LocalPreferences.read(at: paths.root).automaticSyncAccounts.isEmpty)
    }

    func testNotificationSetupDismissalSurvivesRestartAndOlderFilesDecode() throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: base) }
        let paths = try PrivateFiles(base: base)
        try Data(#"{"immediateBackgroundLock":false,"automaticSyncAccounts":[]}"#.utf8).write(to: paths.root.appending(path: "privacy-preferences.json"))
        XCTAssertNil(try LocalPreferences.read(at: paths.root).notificationSetupDismissed)
        try LocalPreferences(notificationSetupDismissed: ["040g2081040g2081040g"]).write(at: paths.root)
        XCTAssertEqual(try LocalPreferences.read(at: paths.root).notificationSetupDismissed, ["040g2081040g2081040g"])
    }

    func testBackgroundInvalidatesRequestsWithoutResettingNavigationOrDrafts() async {
        let model = SessionModel()
        model.drafts["channel"] = "unsent text"
        let view = model.viewGeneration, epoch = model.epoch
        model.background()
        XCTAssertEqual(model.viewGeneration, view)
        XCTAssertGreaterThan(model.epoch, epoch)
        XCTAssertEqual(model.drafts["channel"], "unsent text")
        await model.lock()
        XCTAssertTrue(model.drafts.isEmpty)
        XCTAssertGreaterThan(model.viewGeneration, view)
    }

    func testImmediateBackgroundLockClearsPresentationBeforeReturningAndKeepsCopiedValue() {
        let model = SessionModel()
        model.immediateBackgroundLock = true
        model.drafts["channel"] = "private draft"
        model.clipboard.copy("fixture-copied-value")
        let view = model.viewGeneration
        model.background()
        XCTAssertTrue(model.concealed)
        XCTAssertTrue(model.drafts.isEmpty)
        XCTAssertGreaterThan(model.viewGeneration, view)
        XCTAssertEqual(UIPasteboard.general.string, "fixture-copied-value")
        model.clipboard.clearOwned()
    }

    func testCancelledBusyOperationCannotClearItsReplacementAfterLock() async {
        let model = SessionModel()
        let old = model.beginBusyOperation(synchronizing: true)
        XCTAssertTrue(model.synchronizing)
        model.background()
        XCTAssertTrue(model.finishBusyOperation(old))
        XCTAssertFalse(model.busy)
        XCTAssertFalse(model.synchronizing)
        let locked = model.beginBusyOperation(synchronizing: true)
        await model.lock()
        XCTAssertFalse(model.synchronizing)
        let current = model.beginBusyOperation(synchronizing: true)
        XCTAssertFalse(model.finishBusyOperation(locked))
        XCTAssertTrue(model.busy)
        XCTAssertTrue(model.synchronizing)
        XCTAssertTrue(model.finishBusyOperation(current))
        XCTAssertFalse(model.busy)
        XCTAssertFalse(model.synchronizing)
    }

    func testPublicProfileAndCloudBackupDecodeCoreWireFields() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let profile = try decoder.decode(AccountProfile.self,
            from: Data(#"{"name":"  Studio  ","avatar_url":"https://assets.dmsg.net/avatar.png","updated_at":42}"#.utf8))
        XCTAssertEqual(profile.displayName, "Studio")
        XCTAssertEqual(profile.avatarUrl, "https://assets.dmsg.net/avatar.png")
        let backup = try decoder.decode(CloudBackup.self,
            from: Data(#"{"upload_id":"backup","name":"Notes.pdf","size":12,"bytes":64,"needs_root":false,"local":false}"#.utf8))
        XCTAssertEqual(backup.id, "backup")
        XCTAssertEqual(backup.name, "Notes.pdf")
        XCTAssertFalse(backup.local)
    }

    func testLockClearsCloudBackupMetadata() async {
        let model = SessionModel()
        model.loadFixtures()
        XCTAssertFalse(model.cloudBackups.isEmpty)
        await model.lock()
        XCTAssertTrue(model.cloudBackups.isEmpty)
        XCTAssertFalse(model.loadingCloudBackups)
    }
    func testSendingClearsOnlyTheSubmittedDraft() {
        let model = SessionModel()
        model.drafts["one"] = "new draft"
        model.drafts["two"] = "other channel"
        model.clearQueuedDraft(channel: "one", sent: "submitted")
        XCTAssertEqual(model.drafts["one"], "new draft")
        model.clearQueuedDraft(channel: "one", sent: "new draft")
        XCTAssertEqual(model.drafts["one"], "")
        XCTAssertEqual(model.drafts["two"], "other channel")
    }

    func testChannelBatchesPrioritizeTheVisibleChannelWithoutStarvingOthers() {
        let ids = (0..<8).map(String.init)
        var visited = Set<String>()
        for cursor in [0, 3, 6] {
            let batch = SessionModel.channelBatch(ids, active: "0", cursor: cursor, limit: 4)
            XCTAssertEqual(batch.first, "0")
            XCTAssertEqual(Set(batch).count, batch.count)
            visited.formUnion(batch)
        }
        XCTAssertEqual(visited, Set(ids))
        XCTAssertEqual(SessionModel.channelBatch([], active: nil, cursor: 0, limit: 4), [])
    }

    func testStagingStoresDestinationAndStartupRemovesOnlyPreviewCopies() throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: base) }
        let files = try PrivateFiles(base: base)
        let account = "040g2081040g2081040g"
        let original = base.appending(path: "original.txt")
        try Data("original".utf8).write(to: original)
        let staged = try files.importDocument(original, account: account, folder: "chosen-folder")
        let saved = try XCTUnwrap(files.pending(account: account).first)
        XCTAssertEqual(saved.folder, "chosen-folder")
        XCTAssertTrue(saved.ready)
        let preview = files.workspace.appending(path: "accounts/" + account + "/open/nonce")
        try FileManager.default.createDirectory(at: preview, withIntermediateDirectories: true)
        try Data("preview".utf8).write(to: preview.appending(path: "copy.txt"))
        _ = try PrivateFiles(base: base)
        XCTAssertFalse(FileManager.default.fileExists(atPath: preview.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: staged.path))
        XCTAssertEqual(try files.pending(account: account).count, 1)
        try files.finishImport(staged)
        XCTAssertTrue(try files.pending(account: account).isEmpty)
        XCTAssertTrue(FileManager.default.fileExists(atPath: original.path))
    }

}

extension SessionPolicyTests {
    func testStagingUsesSeparateWorkspaceAndInstallationPreferences() throws {
        let root = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let production = try PrivateFiles(base: root.appending(path: "production"))
        let staged = try PrivateFiles(base: root.appending(path: "staging"), environment: "staging")
        XCTAssertNotEqual(production.workspace, staged.workspace)
        XCTAssertNotEqual(production.installation, staged.installation)
        XCTAssertEqual(staged.workspace.lastPathComponent, "staging")
        XCTAssertThrowsError(try PrivateFiles(base: root, environment: "../production"))
    }
}
