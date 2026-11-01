import XCTest

@MainActor
final class ConversationTests: XCTestCase {
    private func open(_ arguments: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-channel"] + arguments
        app.launch()
        let row = app.buttons["channel.row.fixture-channel"]
        XCTAssertTrue(row.waitForExistence(timeout: 10)); row.tap()
        return app
    }

    func testVerifiedMessagesAppearBeforeSlowRefreshCompletes() {
        let app = open(["--ui-channel-slow", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"])
        XCTAssertTrue(app.staticTexts["Hello channel · 频道消息已显示"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.otherElements["conversation.loading"].exists || app.activityIndicators.firstMatch.exists)
        XCTAssertTrue(app.textFields["message.draft"].exists || app.textViews["message.draft"].exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Channel content during slow sync"; shot.lifetime = .keepAlways; add(shot)
    }

    func testFailedInitialRefreshOffersRetryAndLoadsContent() {
        let app = open(["--ui-channel-failure", "--ui-dark", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"])
        let retry = app.buttons["conversation.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5)); retry.tap()
        XCTAssertTrue(app.staticTexts["Hello channel · 频道消息已显示"].waitForExistence(timeout: 5))
        XCTAssertFalse(retry.exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Channel retry · dark Chinese"; shot.lifetime = .keepAlways; add(shot)
    }

    func testEmptyChannelShowsAnExplicitState() {
        let app = open(["--ui-channel-empty"])
        XCTAssertTrue(app.descendants(matching: .any)["conversation.empty"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["conversation.retry"].exists)
    }
}
