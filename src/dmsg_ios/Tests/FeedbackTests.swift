import XCTest
@testable import dMsg

@MainActor final class FeedbackTests: XCTestCase {
    func testOnlyCompleteDMsgScreenshotLinksCanBeOpened() {
        let link = "https://dmsg.net/s/" + String(repeating: "a", count: 64) + "#" + String(repeating: "B", count: 43)
        XCTAssertEqual(feedbackScreenshotURLs("Screenshot 1: \(link)\nScreenshot 2: \(link)").map(\.absoluteString), [link])
        for wrong in [link.replacingOccurrences(of: "https:", with: "javascript:"), link.replacingOccurrences(of: "dmsg.net/", with: "dmsg.net.evil/"), link.replacingOccurrences(of: "#", with: "?key="), link + "more", "file:///private/test.png"] {
            XCTAssertTrue(feedbackScreenshotURLs(wrong).isEmpty, wrong)
        }
    }

    func testCoreReportRestoresPinnedScreenshotAndPendingStatus() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let value = try decoder.decode(FeedbackRecord.self, from: Data("""
        {"id":"original","revision":4,"recipient":"dbrt466u97gecq800000","text":"私密报告","context":"iOS",
        "screenshots":[{"file":"image","version":"pinned-version","name":"screen.png","size":10,"mime":"image/png","operation":"original-share","url":null,"expires_at":null}],
        "state":"pending","error":"EXECUTION_UNKNOWN","editable":false}
        """.utf8))
        XCTAssertEqual(value.text, "私密报告")
        XCTAssertEqual(value.recipient, FeedbackWorkspace.recipient)
        XCTAssertEqual(value.screenshots[0].libraryFile.current, "pinned-version")
        XCTAssertEqual(value.screenshots[0].operation, "original-share")
        XCTAssertEqual(value.state, "pending"); XCTAssertFalse(value.editable)
        XCTAssertNil(value.screenshots[0].url)
    }
}
