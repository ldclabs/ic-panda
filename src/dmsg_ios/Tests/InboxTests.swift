import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class InboxTests: XCTestCase {
    func testSendFailuresNameTheRecipientActionWithoutChangingGenericConflicts() {
        let pending = letterSendFailure(MobileError.Failure(code: "VERSION_CONFLICT"))
        let rekey = letterSendFailure(MobileError.Failure(code: "REKEY_REQUIRED"))
        let stale = letterSendFailure(MobileError.Failure(code: "POLICY_STALE"))
        XCTAssertFalse(pending.contains("VERSION_CONFLICT"))
        XCTAssertFalse(rekey.contains("REKEY_REQUIRED"))
        XCTAssertFalse(stale.contains("POLICY_STALE"))
        XCTAssertNotEqual(pending, rekey)
        XCTAssertNotEqual(stale, rekey)
        XCTAssertEqual(letterSendFailure(MobileError.Failure(code: "EXECUTION_UNKNOWN")), InboxLoading.message(MobileError.Failure(code: "EXECUTION_UNKNOWN")))
    }

    private func letter(_ id: String = "one") -> InboxLetter {
        InboxLetter(orderId: id, sender: "sender", state: "visible", text: "Private letter",
                    createdAt: 1, read: false, archived: false, abortReason: nil)
    }

    func testCoreLetterFieldsDecodeWithoutDependingOnExtraWireFields() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let data = Data("""
        [{"order_id":"one","sender":"sender","state":"visible","text":"来信","created_at":1793534400000,
        "read":false,"archived":false,"signed":"verified-signature","abort_reason":null}]
        """.utf8)
        let rows = try decoder.decode([InboxLetter].self, from: data)
        XCTAssertEqual(rows[0].id, "one"); XCTAssertEqual(rows[0].text, "来信")
        XCTAssertEqual(rows[0].createdAt, 1_793_534_400_000)
    }

    func testMissingKeyFailureKeepsSavedLettersAndRetryClearsIt() async {
        let state = InboxLoading()
        await state.refresh { refresh in
            if refresh { throw MobileError.Failure(code: "RECOVERY_INCOMPLETE") }
            return InboxSnapshot(letters: [self.letter()], complete: false)
        }
        XCTAssertEqual(state.letters.map(\.id), ["one"])
        XCTAssertTrue(state.known); XCTAssertNotNil(state.failure); XCTAssertFalse(state.refreshing)
        XCTAssertNotEqual(state.failure, InboxLoading.message(MobileError.Failure(code: "NETWORK_UNAVAILABLE")))
        await state.refresh { _ in InboxSnapshot(letters: [self.letter(), self.letter("two")], complete: true) }
        XCTAssertEqual(state.letters.count, 2); XCTAssertNil(state.failure)
    }

    func testEmptyObservationAndFailureAreDifferentStates() async {
        let state = InboxLoading()
        await state.cached { _ in InboxSnapshot(letters: [], complete: false) }
        XCTAssertFalse(state.known)
        await state.refresh { refresh in
            if refresh { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
            return InboxSnapshot(letters: [], complete: true)
        }
        XCTAssertNotNil(state.failure)
        await state.refresh { _ in InboxSnapshot(letters: [], complete: true) }
        XCTAssertTrue(state.known); XCTAssertNil(state.failure); XCTAssertTrue(state.letters.isEmpty)
    }

    func testLateCachedReadCannotReplaceFreshLetters() async {
        let state = InboxLoading()
        var resume: CheckedContinuation<InboxSnapshot, Never>?
        let local = Task { await state.cached { _ in await withCheckedContinuation { resume = $0 } } }
        while resume == nil { await Task.yield() }
        await state.refresh { _ in InboxSnapshot(letters: [self.letter()], complete: true) }
        resume?.resume(returning: InboxSnapshot(letters: [], complete: false))
        await local.value
        XCTAssertEqual(state.letters.count, 1); XCTAssertTrue(state.known)
    }

    func testCancellationKeepsTheExistingDisplayWithoutAnError() async {
        let state = InboxLoading()
        await state.cached { _ in InboxSnapshot(letters: [self.letter()], complete: true) }
        await state.refresh { _ in throw CancellationError() }
        XCTAssertEqual(state.letters.count, 1); XCTAssertNil(state.failure); XCTAssertFalse(state.refreshing)
    }

    func testHintsDuringAnInboxRefreshDrainOnceEvenAfterFailure() async {
        let state = InboxLoading()
        var reads = 0
        var resume: CheckedContinuation<Void, Never>?
        let read: InboxLoading.Read = { fresh in
            if fresh {
                reads += 1
                if reads == 1 {
                    await withCheckedContinuation { resume = $0 }
                    throw MobileError.Failure(code: "NETWORK_UNAVAILABLE")
                }
            }
            return InboxSnapshot(letters: [self.letter(fresh ? "new" : "cached")], complete: true)
        }
        let first = Task { await state.refresh(read: read) }
        while resume == nil { await Task.yield() }
        await state.refresh(read: read)
        await state.refresh(read: read)
        resume?.resume()
        await first.value
        XCTAssertEqual(reads, 2)
        XCTAssertEqual(state.letters.map(\.id), ["new"])
        XCTAssertNil(state.failure)
        XCTAssertFalse(state.refreshing)
    }

    func testNewerLocalReadWinsWhenAnOlderSnapshotReturnsFirst() async {
        let state = InboxLoading()
        var older: CheckedContinuation<InboxSnapshot, Never>?
        var newer: CheckedContinuation<InboxSnapshot, Never>?
        let first = Task { await state.cached { _ in await withCheckedContinuation { older = $0 } } }
        while older == nil { await Task.yield() }
        let second = Task { await state.cached { _ in await withCheckedContinuation { newer = $0 } } }
        while newer == nil { await Task.yield() }
        older?.resume(returning: InboxSnapshot(letters: [letter("old")], complete: true))
        await first.value
        newer?.resume(returning: InboxSnapshot(letters: [letter("new")], complete: true))
        await second.value
        XCTAssertEqual(state.letters.map(\.id), ["new"])
    }
}
