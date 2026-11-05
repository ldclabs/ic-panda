import XCTest

@MainActor
final class InboxUITests: XCTestCase {
    private func open(_ arguments: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-inbox"] + arguments
        app.launch()
        let tab = app.buttons["messages.mode.letters"]
        XCTAssertTrue(tab.waitForExistence(timeout: 10)); tab.tap()
        return app
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }

    func testMissingKeyRetryLoadsAndReadArchiveRestoreUpdateImmediately() {
        let app = open(["--ui-inbox-failure", "--ui-dark", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"])
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
        XCTAssertEqual(read.label, "标为未读")
        app.buttons["inbox.archive"].tap()
        XCTAssertTrue(app.buttons["恢复"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.descendants(matching: .any)["inbox.empty"].waitForExistence(timeout: 5))
        app.buttons["letters.folder"].tap()
        app.buttons["已归档"].tap()
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
        let app = open(["--ui-inbox-failure", "-AppleLanguages", "(ar)", "-AppleLocale", "ar_SA"])
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
