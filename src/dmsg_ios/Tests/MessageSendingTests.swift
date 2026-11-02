import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class MessageSendingTests: XCTestCase {
    @MainActor private final class Gate {
        var continuation: CheckedContinuation<MessageItem, Error>?
        func wait() async throws -> MessageItem {
            try await withCheckedThrowingContinuation { continuation = $0 }
        }
        func finish(_ result: Result<MessageItem, Error>) { continuation?.resume(with: result); continuation = nil }
    }

    private func message(_ id: String, _ seq: UInt64, _ text: String = "confirmed") -> MessageItem {
        MessageItem(id: id, sequence: seq, account: "self", text: text, createdAtMs: 1,
            attachmentName: nil, attachmentMime: nil, attachmentSize: nil, unreadable: false,
            digest: id, replySequence: nil, edited: false, retracted: false, attachmentDeleted: false, hidden: false)
    }

    private func item(_ id: String) -> OutgoingMessage {
        OutgoingMessage(id: id, text: id, attachmentName: nil, attempted: false, error: nil)
    }

    private func waitUntil(_ condition: () -> Bool) async {
        for _ in 0..<100 {
            if condition() { return }
            try? await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertTrue(condition())
    }

    func testQueuedSendsPreserveOrderAndAckReplacesPendingWithoutReload() async {
        let sender = MessageSending(), gate = Gate()
        var starts: [String] = []
        sender.start(channel: "channel", item: item("one"), work: { starts.append("one"); return try await gate.wait() })
        sender.start(channel: "channel", item: item("two"), work: { starts.append("two"); return self.message("two", 2) })
        sender.start(channel: "channel", item: item("one"), work: { XCTFail("duplicate publication"); return self.message("one", 1) })
        await waitUntil { gate.continuation != nil }
        XCTAssertEqual(starts, ["one"])
        XCTAssertEqual(sender.items(channel: "channel", stored: []).map(\.id), ["one", "two"])
        XCTAssertTrue(sender.items(channel: "other-channel", stored: []).isEmpty)
        XCTAssertTrue(sender.contains("two"))
        gate.finish(.success(message("one", 1)))
        await waitUntil { !sender.contains("two") }
        XCTAssertEqual(starts, ["one", "two"])
        XCTAssertTrue(sender.items(channel: "channel", stored: [item("one"), item("two")]).isEmpty)
        XCTAssertEqual(sender.messages(channel: "channel", stored: []).map(\.sequence), [1, 2])
    }

    func testFailureRetainsOriginalIdAndRetryDoesNotNeedAView() async {
        let sender = MessageSending()
        var completed = 0
        sender.start(channel: "channel", item: item("same-id"), work: { throw MobileError.Failure(code: "EXECUTION_UNKNOWN") }, finished: { _ in completed += 1 })
        await waitUntil { completed == 1 }
        let pending = sender.items(channel: "channel", stored: [])
        XCTAssertEqual(pending.first?.id, "same-id")
        XCTAssertEqual(pending.first?.error, "EXECUTION_UNKNOWN")
        XCTAssertTrue(sender.messages(channel: "channel", stored: []).isEmpty)
        XCTAssertTrue(sender.items(channel: "channel", stored: pending, received: ["same-id"]).isEmpty,
                      "a verified own message received before the ACK must not also appear unconfirmed")
        sender.start(channel: "channel", item: pending[0], work: { self.message("same-id", 3) })
        await waitUntil { !sender.contains("same-id") }
        XCTAssertTrue(sender.items(channel: "channel", stored: []).isEmpty)
        XCTAssertEqual(sender.messages(channel: "channel", stored: []).first?.sequence, 3)
    }

    func testCompletedOrCancelledCreatorDoesNotOwnDeliveryLifetime() async {
        let sender = MessageSending(), gate = Gate()
        let creator = Task {
            sender.start(channel: "channel", item: item("survives"), work: { try await gate.wait() })
        }
        await creator.value
        creator.cancel()
        await waitUntil { gate.continuation != nil }
        gate.finish(.success(message("survives", 1)))
        await waitUntil { !sender.contains("survives") }
        XCTAssertTrue(sender.active.isEmpty)
        XCTAssertEqual(sender.messages(channel: "channel", stored: []).first?.id, "survives")
    }

    func testSessionResetClearsPlaintextAndRejectsLateCompletion() async {
        let sender = MessageSending(), gate = Gate()
        var completed = 0
        sender.start(channel: "channel", item: item("old"), work: { try await gate.wait() }, finished: { _ in completed += 1 })
        sender.start(channel: "channel", item: item("queued-old"), work: { XCTFail("old queued work ran"); return self.message("queued-old", 2) })
        await waitUntil { gate.continuation != nil }
        sender.reset()
        XCTAssertTrue(sender.active.isEmpty)
        XCTAssertTrue(sender.items(channel: "channel", stored: []).isEmpty)
        gate.finish(.success(message("old", 1)))
        sender.start(channel: "channel", item: item("new"), work: { self.message("new", 3) })
        await waitUntil { !sender.contains("new") }
        XCTAssertEqual(completed, 0)
        XCTAssertEqual(sender.messages(channel: "channel", stored: []).map(\.id), ["new"])
    }

    func testSnapshotStartedBeforeAckCannotReplaceItAndNewerLocalEditsCan() async {
        let sender = MessageSending()
        let oldRevision = sender.revision
        sender.start(channel: "channel", item: item("edit"), work: { self.message("original", 1, "edited") })
        await waitUntil { !sender.contains("edit") }
        sender.loaded(channel: "channel", items: [message("original", 1, "old")], since: oldRevision)
        XCTAssertEqual(sender.messages(channel: "channel", stored: [message("original", 1, "old")]).first?.text, "edited")
        sender.loaded(channel: "channel", items: [message("original", 1, "newer edit")], since: sender.revision)
        XCTAssertEqual(sender.messages(channel: "channel", stored: [message("original", 1, "newer edit")]).first?.text, "newer edit")
    }
}
