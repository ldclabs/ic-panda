import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class VaultDraftLifecycleTests: XCTestCase {
    private let account = "040g2081040g2081040g"

    private func activate(_ model: SessionModel, account: String = "040g2081040g2081040g") {
        model.installFixtures(SessionSnapshot(generation: 1, sequence: 1, state: "unlocked", account: account,
            deviceId: "fixture-device", pendingSync: 0, conflicts: 0, approvalsWaiting: 0, fastUnlock: false,
            loginExpiresAtMs: nil, loginRenewable: false, passkeyAvailable: false, setupPending: false, lastSyncAtMs: nil, syncError: nil), items: [])
        model.lifecycle = .active
    }

    func testBackgroundRetainsEditorAndLastInputIncludingUncommittedTag() async throws {
        let model = SessionModel(); activate(model)
        await model.vaultEditing.open(VaultEditorTarget(item: nil), model: model)
        let entry = try XCTUnwrap(model.vaultEditing.presented)
        var draft = entry.draft; draft.title = "Unfinished"; draft.tagEntry = "typing tag"
        draft.fields[1].value = "a password still being typed"; draft.fields[1].changed = true
        model.vaultEditing.update(draft, id: entry.id, model: model)
        model.background()
        XCTAssertTrue(model.concealed)
        XCTAssertEqual(model.vaultEditing.presented?.id, entry.id)
        model.lifecycle = .active
        await model.vaultEditing.load(model)
        XCTAssertEqual(model.vaultEditing.presented?.draft, draft)
    }

    func testLockFlushesLastKeystrokeBeforeClearingPlaintextAndRestoresAfterUnlock() async throws {
        let model = SessionModel(); activate(model)
        await model.vaultEditing.open(VaultEditorTarget(item: nil), model: model)
        let entry = try XCTUnwrap(model.vaultEditing.presented)
        var draft = entry.draft; draft.notes = "Last input without waiting for autosave"
        model.vaultEditing.update(draft, id: entry.id, model: model)
        await model.lock()
        XCTAssertNil(model.vaultEditing.presented)
        XCTAssertTrue(model.vaultEditing.pendingInputs.isEmpty)
        activate(model)
        await model.vaultEditing.load(model)
        XCTAssertEqual(model.vaultEditing.presented?.draft, draft)
    }

    func testImmediateBackgroundLockKeepsThePendingCheckpoint() async throws {
        let model = SessionModel(); activate(model)
        model.immediateBackgroundLock = true
        await model.vaultEditing.open(VaultEditorTarget(item: nil), model: model)
        let entry = try XCTUnwrap(model.vaultEditing.presented)
        var draft = entry.draft; draft.title = "Last input before immediate lock"
        model.vaultEditing.update(draft, id: entry.id, model: model)
        model.background()
        XCTAssertFalse(model.unlocked)
        XCTAssertNil(model.vaultEditing.presented)
        activate(model); await model.vaultEditing.load(model)
        XCTAssertEqual(model.vaultEditing.presented?.draft, draft)
    }

    func testAccountChangeDoesNotRestoreAnotherAccountsDraftAndDiscardStaysDiscarded() async throws {
        let model = SessionModel(); activate(model)
        await model.vaultEditing.open(VaultEditorTarget(item: nil), model: model)
        let entry = try XCTUnwrap(model.vaultEditing.presented)
        var draft = entry.draft; draft.title = "Account A"
        model.vaultEditing.update(draft, id: entry.id, model: model)
        await model.lock()
        activate(model, account: "080g2081040g2081040g")
        await model.vaultEditing.load(model)
        XCTAssertNil(model.vaultEditing.presented)
        await model.lock(); activate(model)
        await model.vaultEditing.load(model)
        XCTAssertEqual(model.vaultEditing.presented?.draft.title, "Account A")
        try await model.vaultEditing.discard(entry.id, model: model)
        await model.lock(); activate(model); await model.vaultEditing.load(model)
        XCTAssertNil(model.vaultEditing.presented)
    }

    func testStoredFormRestoresFieldIntentAndOriginalRevision() throws {
        var draft = VaultDraft(target: VaultEditorTarget(item: nil))
        draft.baseRevision = String(repeating: "b", count: 64)
        draft.fields[1].value = ""; draft.fields[1].changed = false
        draft.tagEntry = "unfinished tag"; draft.files = [String(repeating: "c", count: 64)]
        let entry = VaultEditorSession(id: String(repeating: "a", count: 64), itemID: String(repeating: "d", count: 64), draft: draft, initial: draft, revision: 17, active: true)
        let input = try entry.input
        XCTAssertEqual(input.form, try entry.input.form, "Retrying one revision must preserve its exact form bytes")
        let reopened = try VaultEditorSession.Stored(id: input.id, itemId: input.itemId, baseRevision: input.baseRevision, revision: input.revision, form: input.form, active: input.active).session()
        XCTAssertEqual(reopened.draft, draft)
        XCTAssertEqual(reopened.revision, 17)
        XCTAssertFalse(reopened.draft.fields[1].changed, "An unchanged hidden field must not become an empty replacement")
    }

    func testFreshSessionOwnerRestoresTheActiveCheckpointAfterMemoryIsGone() async throws {
        let first = SessionModel(); activate(first)
        await first.vaultEditing.open(VaultEditorTarget(item: nil), model: first)
        let entry = try XCTUnwrap(first.vaultEditing.presented)
        var draft = entry.draft; draft.title = "Continue after restart"; draft.tagEntry = "unfinished"
        draft.fields[1].value = "authored password"; draft.fields[1].changed = true
        first.vaultEditing.update(draft, id: entry.id, model: first)
        await first.vaultEditing.flush(first)
        let checkpoint = try first.vaultEditing.input(id: entry.id)
        first.protectedDataUnavailable()
        XCTAssertNil(first.vaultEditing.presented)

        let restarted = SessionModel(); activate(restarted)
        try restarted.vaultEditing.installFixtureCheckpoint(checkpoint, account: account)
        await restarted.vaultEditing.load(restarted)
        XCTAssertEqual(restarted.vaultEditing.presented?.id, entry.id)
        XCTAssertEqual(restarted.vaultEditing.presented?.draft, draft)
        XCTAssertTrue(restarted.vaultEditing.ready)
    }
}
