import XCTest

@MainActor
final class InboxUITests: XCTestCase {
    private func open(_ arguments: [String] = [], inbox: String = "Inbox") -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-inbox"] + arguments
        app.launch()
        let tab = app.segmentedControls.buttons[inbox]
        XCTAssertTrue(tab.waitForExistence(timeout: 10)); tab.tap()
        return app
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }

    func testMissingKeyRetryLoadsAndReadArchiveRestoreUpdateImmediately() {
        let app = open(["--ui-inbox-failure", "--ui-dark", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"], inbox: "来信")
        let retry = app.buttons["inbox.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["inbox.empty"].exists)
        capture(app, "Inbox missing key · Chinese dark")
        retry.tap()
        let row = app.buttons["inbox.row.fixture-letter"]
        XCTAssertTrue(row.waitForExistence(timeout: 5)); XCTAssertFalse(retry.exists)
        capture(app, "Inbox received letter · Chinese dark")
        row.tap()
        let read = app.buttons["inbox.read"]
        XCTAssertTrue(read.waitForExistence(timeout: 5)); read.tap()
        let readUpdated = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: read)
        XCTAssertEqual(XCTWaiter.wait(for: [readUpdated], timeout: 5), .completed)
        app.buttons["inbox.archive"].tap()
        XCTAssertTrue(app.buttons["恢复"].waitForExistence(timeout: 5))
        app.buttons["BackButton"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["inbox.empty"].waitForExistence(timeout: 5))
        // SwiftUI exposes both the full row and the actual switch. Tapping
        // the row's center hits its empty space, not the control on the edge.
        let archived = app.switches["inbox.archived"].switches.firstMatch
        XCTAssertTrue(archived.exists); archived.tap()
        XCTAssertEqual(archived.value as? String, "1")
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        app.buttons["inbox.archive"].tap()
        XCTAssertTrue(app.buttons["归档"].waitForExistence(timeout: 5))
        capture(app, "Inbox restored letter · Chinese dark")
    }

    func testVerifiedEmptyInboxHasNoFailure() {
        let app = open(["--ui-inbox-empty", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"])
        XCTAssertTrue(app.descendants(matching: .any)["inbox.empty"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["inbox.retry"].exists)
        capture(app, "Inbox verified empty · English light")
    }

    func testSlowInboxShowsLoadingWithoutClaimingEmpty() {
        let app = open(["--ui-inbox-slow", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"])
        XCTAssertTrue(app.descendants(matching: .any)["inbox.loading"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["inbox.empty"].exists)
        XCTAssertFalse(app.buttons["inbox.retry"].exists)
        capture(app, "Inbox loading · English light")
    }

    func testArabicFailureAndRetryKeepTheLetterReadable() {
        let app = open(["--ui-inbox-failure", "-AppleLanguages", "(ar)", "-AppleLocale", "ar_SA"], inbox: "صندوق الوارد")
        let retry = app.buttons["inbox.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        capture(app, "Inbox missing key · Arabic RTL")
        retry.tap()
        let row = app.buttons["inbox.row.fixture-letter"]
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        XCTAssertTrue(app.buttons["inbox.read"].waitForExistence(timeout: 5))
        capture(app, "Inbox letter detail · Arabic RTL")
    }
}
