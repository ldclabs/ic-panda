import XCTest
@testable import dMsg

final class MessagingTests: XCTestCase {
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
        draft.invitation = String(repeating: "a", count: 64); XCTAssertTrue(draft.valid)
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
