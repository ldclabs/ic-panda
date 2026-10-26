import XCTest
@testable import dMsg

@MainActor
final class SessionPolicyTests: XCTestCase {
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
        let old = model.beginBusyOperation()
        model.background()
        XCTAssertTrue(model.finishBusyOperation(old))
        XCTAssertFalse(model.busy)
        let locked = model.beginBusyOperation()
        await model.lock()
        let current = model.beginBusyOperation()
        XCTAssertFalse(model.finishBusyOperation(locked))
        XCTAssertTrue(model.busy)
        XCTAssertTrue(model.finishBusyOperation(current))
        XCTAssertFalse(model.busy)
    }
}
