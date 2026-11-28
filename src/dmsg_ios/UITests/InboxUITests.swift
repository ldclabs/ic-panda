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

    func testReplyFailuresExplainRecoveryAndKeepTheOriginalLetter() {
        for (language, failure, dark) in [("zh-Hans", "--ui-letter-policy-stale", true), ("zh-Hans", "--ui-letter-stale-key", false), ("ar", "--ui-letter-policy-stale", false), ("de", "--ui-letter-pending", true)] {
            let stored = failure == "--ui-letter-policy-stale" ? ["--ui-letter-stored-failure"] : []
            let app = open([failure, "-AppleLanguages", "(\(language))"] + (dark ? ["--ui-dark"] : []) + stored)
            let row = app.buttons["inbox.row.fixture-letter"]
            XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
            let reply = app.buttons["inbox.reply"]
            XCTAssertTrue(reply.waitForExistence(timeout: 5)); reply.tap()
            let text = app.textViews["letter.text"].exists ? app.textViews["letter.text"] : app.textFields["letter.text"]
            XCTAssertTrue(text.waitForExistence(timeout: 5)); text.tap(); text.typeText("Keep this original letter")
            app.buttons["letter.send"].tap()
            let notice = app.descendants(matching: .any)["letter.send.failure"].firstMatch
            XCTAssertTrue(notice.waitForExistence(timeout: 5))
            XCTAssertFalse(notice.label.contains("REKEY_REQUIRED"))
            XCTAssertFalse(notice.label.contains("VERSION_CONFLICT"))
            XCTAssertFalse(notice.label.contains("POLICY_STALE"))
            XCTAssertTrue(app.staticTexts["Keep this original letter"].exists)
            app.swipeUp()
            capture(app, "Letter recovery · \(language) · \(failure)")
            let retry = app.buttons["letter.retry"]
            XCTAssertTrue(retry.waitForExistence(timeout: 5)); retry.tap()
            XCTAssertTrue(notice.waitForNonExistence(timeout: 5))
            XCTAssertFalse(retry.exists)
            XCTAssertTrue(app.staticTexts["Keep this original letter"].exists)
            app.terminate()
        }
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
        let more = app.buttons["inbox.more"]
        XCTAssertTrue(more.waitForExistence(timeout: 5)); more.tap()
        let read = app.buttons["inbox.read"]
        XCTAssertTrue(read.waitForExistence(timeout: 5)); XCTAssertEqual(read.label, "标为未读"); read.tap()
        more.tap()
        XCTAssertTrue(read.waitForExistence(timeout: 5)); XCTAssertEqual(read.label, "标记为已读")
        app.buttons["inbox.archive"].tap()
        more.tap()
        XCTAssertTrue(app.buttons["恢复"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.85)).tap()
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.descendants(matching: .any)["inbox.empty"].waitForExistence(timeout: 5))
        app.buttons["letters.folder"].tap()
        app.buttons["已归档"].tap()
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        XCTAssertTrue(more.waitForExistence(timeout: 5)); more.tap()
        app.buttons["inbox.archive"].tap()
        more.tap()
        XCTAssertTrue(app.buttons["归档"].waitForExistence(timeout: 5))
        capture(app, "Inbox restored letter · Chinese dark")
    }

    func testTabletSelectedLetterIsReadableInBothAppearances() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .pad)
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }
        for dark in [false, true] {
            let app = open(["-AppleLanguages", "(en)"] + (dark ? ["--ui-dark"] : []))
            let row = app.buttons["inbox.row.fixture-letter"]
            XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
            XCTAssertTrue(app.buttons["inbox.reply"].waitForExistence(timeout: 5))
            XCTAssertTrue(row.isHittable)
            let shot = XCTAttachment(screenshot: row.screenshot())
            shot.name = dark ? "Selected letter row dark" : "Selected letter row light"
            shot.lifetime = .keepAlways; add(shot)
            capture(app, dark ? "Selected letter dark" : "Selected letter light")
            app.terminate()
        }
    }

    func testReceivingModesRevealOnlyRelevantAccountControls() {
        let app = open(["--ui-inbox-empty", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"])
        app.buttons["Inbox settings"].tap()
        let policy = app.buttons["inbox.policy.mode"], addAccount = app.buttons["Add account"]
        XCTAssertTrue(policy.waitForExistence(timeout: 5))
        XCTAssertFalse(addAccount.exists)
        policy.tap(); app.buttons["Selected accounts"].tap()
        XCTAssertTrue(addAccount.waitForExistence(timeout: 3)); addAccount.tap()
        let account = app.textFields["inbox.account.0"]
        XCTAssertTrue(account.waitForExistence(timeout: 3))
        XCTAssertTrue(app.staticTexts["Enter a valid account ID."].exists)
        policy.tap(); app.buttons["Anyone, free of charge"].tap()
        XCTAssertFalse(app.textFields["inbox.account.0"].exists)
        XCTAssertTrue(app.staticTexts["Anyone you haven’t blocked can write for free."].exists)
        policy.tap(); app.buttons["Selected accounts"].tap()
        XCTAssertTrue(account.waitForExistence(timeout: 3))
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
        XCTAssertTrue(app.buttons["inbox.more"].waitForExistence(timeout: 5))
        capture(app, "Inbox letter detail · Arabic RTL")
    }
}
