import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class InboxTests: XCTestCase {
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
}
