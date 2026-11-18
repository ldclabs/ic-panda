import XCTest
@testable import dMsg

final class MessagingTests: XCTestCase {
    func testChannelCreationRetainsItsRequestForTheSameReviewedName() throws {
        let name = ConversationName(" 讨论组 ")
        let first = ChannelCreationAttempt(name: name, previous: nil)
        let restored = try JSONDecoder().decode(ChannelCreationAttempt.self, from: JSONEncoder().encode(first))
        XCTAssertEqual(ChannelCreationAttempt(name: ConversationName("讨论组"), previous: restored).request, first.request)
        XCTAssertNotEqual(ChannelCreationAttempt(name: ConversationName("另一个讨论组"), previous: restored).request, first.request)
        let composed = ChannelCreationAttempt(name: ConversationName("Café"), previous: nil)
        let retry = ChannelCreationAttempt(name: ConversationName("Cafe\u{301}"), previous: composed)
        XCTAssertEqual(retry.request, composed.request)
        XCTAssertEqual(Array(retry.name.utf8), Array(composed.name.utf8))
        XCTAssertTrue(ConversationName("👩‍💻").valid)
        XCTAssertFalse(ConversationName(String(repeating: "👩‍💻", count: 34)).valid)
        XCTAssertFalse(ConversationName("a\nb").valid)
    }

    func testQueuePinsReviewedFileVersionsAndReusesTheOriginalReceiptOnlyForARetry() {
        let file = LibraryFile(id: "one", name: "report.txt", mime: "text/plain", size: 12,
                               current: "version-1", sha256: "", status: "local", versions: 1)
        let update = LibraryFile(id: file.id, name: file.name, mime: file.mime, size: 14,
                                 current: "version-2", sha256: "", status: "local", versions: 2)
        var queue = ComposerQueue()
        let first = queue.prepare(channel: "channel", text: "Reviewed", files: [file], reply: nil)
        XCTAssertEqual(first.attachments.first?.version, "version-1")
        XCTAssertEqual(queue.prepare(channel: "channel", text: "Reviewed", files: [file], reply: nil).stamp, first.stamp)
        let newer = queue.prepare(channel: "channel", text: "Reviewed", files: [update], reply: nil)
        XCTAssertNotEqual(first.stamp, newer.stamp)
        queue.finish(first)
        XCTAssertEqual(queue.prepare(channel: "channel", text: "Reviewed", files: [update], reply: nil).stamp, newer.stamp)
        queue.finish(newer)
        XCTAssertNotEqual(queue.prepare(channel: "channel", text: "Reviewed", files: [update], reply: nil).stamp, newer.stamp)
    }

    @MainActor
    func testPeopleDirectoryBatchesOnlyDistinctRequestedAccounts() async throws {
        let directory = PeopleDirectory()
        let accounts = (0..<205).map { String(format: "%019x0", $0) }
        var batches: [[String]] = []
        directory.request(accounts + accounts + ["invalid"]) { batch in batches.append(batch); return true }
        let deadline = ContinuousClock.now.advanced(by: .seconds(2))
        while batches.count < 3 && ContinuousClock.now < deadline { try await Task.sleep(for: .milliseconds(5)) }
        XCTAssertEqual(batches.map(\.count), [100, 100, 5])
        XCTAssertEqual(Set(batches.joined()), Set(accounts))
    }

    @MainActor
    func testResetWhileNamesAreLoadingKeepsTheNewQueueAndMergesRepeatedRows() async throws {
        let directory = PeopleDirectory()
        let first = "040g2082040g2082040g", second = "040g2081040g2081040g"
        var releaseOld: CheckedContinuation<Bool, Never>?
        var releaseNew: CheckedContinuation<Bool, Never>?
        var batches: [[String]] = []
        directory.request([first]) { _ in await withCheckedContinuation { releaseOld = $0 } }
        while releaseOld == nil { await Task.yield() }
        directory.reset()
        directory.request([second, second]) { batch in
            batches.append(batch)
            if batches.count == 1 { return await withCheckedContinuation { releaseNew = $0 } }
            return true
        }
        while releaseNew == nil { await Task.yield() }
        releaseOld?.resume(returning: false)
        await Task.yield()
        directory.request([first, second]) { _ in XCTFail("The replacement worker still owns the queue"); return false }
        releaseNew?.resume(returning: true)
        let timeout = ContinuousClock.now.advanced(by: .seconds(2))
        while batches.count < 2 && ContinuousClock.now < timeout { try await Task.sleep(for: .milliseconds(5)) }
        XCTAssertEqual(batches, [[second], [first]])
    }

    @MainActor
    func testLateDraftSnapshotCannotReplaceNewTyping() {
        let model = SessionModel(), workspace = LetterWorkspace()
        let original = LetterDraft(recipient: "040g2082040g2082040g", text: "saved")
        workspace.accept(saved: [original], sent: [], startedAt: workspace.revision)
        let readRevision = workspace.revision
        var edited = original; edited.text = "new typing"
        workspace.update(edited, model: model)
        workspace.accept(saved: [original], sent: [], startedAt: readRevision)
        XCTAssertEqual(workspace.drafts[original.id]?.text, "new typing")
        workspace.reset()
        XCTAssertTrue(workspace.drafts.isEmpty)
    }

    func testLetterValidationUsesUtf8BytesAndFullAccountFormat() {
        var draft = LetterDraft(recipient: "040g2082040g2082040g", text: String(repeating: "信", count: 2000))
        XCTAssertTrue(draft.valid)
        draft.text += "信"; XCTAssertFalse(draft.valid)
        draft.text = "hello"; draft.recipient = String(repeating: "z", count: 20); XCTAssertFalse(draft.valid)
        draft.recipient = "040g2082040g2082040g"; draft.invitation = "bad"; XCTAssertFalse(draft.valid)
        let invitation = String(repeating: "a", count: 64)
        draft.invitation = invitation + "\n"; XCTAssertFalse(draft.valid)
        draft.invitation = invitation; XCTAssertTrue(draft.valid)
        draft.text = " \n "; XCTAssertFalse(draft.valid)
    }

    func testChannelLinksOnlyCarryAnInvitationAndNeverAuthorizeJoining() throws {
        let json = """
        {"format":"dmsg-channel-invitation/1","channel":"\(String(repeating: "a", count: 64))","account":"040g2082040g2082040g","name":"A conversation","expires_at":2000000000000}
        """
        let fragment = Data(json.utf8).base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        let input = "dmsg://channel/join#" + fragment
        let link = try XCTUnwrap(ChannelInvitationLink(URL(string: input)!))
        XCTAssertEqual(ChannelInvitationSummary.parse(link.value)?.account, "040g2082040g2082040g")
        let channel = String(repeating: "a", count: 64)
        XCTAssertNil(ChannelInvitationSummary.parse(json.replacingOccurrences(of: channel, with: channel + #"\n"#)))
        for bad in ["dmsg://channel/join?approve=true#" + fragment, "https://channel/join#" + fragment, "dmsg://user@channel/join#" + fragment, "dmsg://channel/join#invalid", "dmsg://channel/other#" + fragment] {
            XCTAssertNil(ChannelInvitationLink(URL(string: bad)!), bad)
        }
    }

    func testHistoryDecodesWithoutInventingAnOldTimestampOrPlaintext() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let row = try decoder.decode(SentLetter.self, from: Data("""
        {"id":"one","recipient":"040g2082040g2082040g","text":null,"created_at":null,"state":"unknown","error":null,"pending":false,"editable":false}
        """.utf8))
        XCTAssertNil(row.text); XCTAssertNil(row.createdAt); XCTAssertFalse(row.editable)
        XCTAssertNotEqual(letterStateTitle("aborted"), String(localized: "Message withdrawn"))
        XCTAssertNotEqual(letterStateTitle("visible"), String(localized: "Mark as read"))
    }
}
