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

    func testIncomingMessageMenuMatchesDesktopActions() {
        let app = open(["-AppleLanguages", "(en)", "-AppleLocale", "en_US"])
        let message = app.staticTexts["Hello channel · 频道消息已显示"]
        XCTAssertTrue(message.waitForExistence(timeout: 5))
        message.press(forDuration: 1)
        for action in ["Reply", "Copy", "Hide on this device"] {
            XCTAssertTrue(app.buttons[action].waitForExistence(timeout: 3), action)
        }
        XCTAssertFalse(app.buttons["Edit message"].exists)
        XCTAssertFalse(app.buttons["Report content"].exists)
        XCTAssertFalse(app.buttons["Block this account"].exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Message menu without reporting or blocking"; shot.lifetime = .keepAlways; add(shot)
    }

    func testHistoryStopsAtTheBeginningWithoutPullToRefresh() {
        let app = open(["--ui-channel-history", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"])
        let earlier = app.buttons["conversation.earlier"]
        XCTAssertTrue(earlier.waitForExistence(timeout: 5))
        let scroll = app.scrollViews.firstMatch
        scroll.swipeDown()
        earlier.tap()
        scroll.swipeDown()
        XCTAssertTrue(app.staticTexts["History message 1"].waitForExistence(timeout: 5))
        XCTAssertFalse(earlier.exists)
        scroll.swipeDown()
        XCTAssertFalse(app.buttons["conversation.retry"].waitForExistence(timeout: 2))
        XCTAssertFalse(app.descendants(matching: .any)["conversation.loading"].exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "History beginning · no redundant refresh"; shot.lifetime = .keepAlways; add(shot)
    }

    private func composer(_ app: XCUIApplication) -> XCUIElement {
        let text = app.textViews["message.draft"]
        return text.exists ? text : app.textFields["message.draft"]
    }

    func testComposerAcceptsNextMessageAndSendingSurvivesTabChange() {
        let app = open(["--ui-channel-send", "--ui-channel-send-held", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"])
        let draft = composer(app), send = app.buttons["message.send"]
        draft.tap(); draft.typeText("First queued message")
        send.tap()
        let pendingText = app.staticTexts["First queued message"]
        XCTAssertTrue(pendingText.waitForExistence(timeout: 5))
        XCTAssertGreaterThan(pendingText.frame.midX, app.frame.midX, "The pending message belongs on the outgoing side")
        let pendingShot = XCTAttachment(screenshot: app.screenshot())
        pendingShot.name = "Outgoing message stays on the right before confirmation"; pendingShot.lifetime = .keepAlways; add(pendingShot)
        draft.tap(); draft.typeText("Second queued message")
        XCTAssertTrue(send.isEnabled, "waiting for a send must not block the next draft")
        send.tap()
        // Leave the detail first: the software keyboard covers the tab bar.
        // Tapping an offscreen tab can hit a keyboard key without navigating.
        app.navigationBars.buttons.firstMatch.tap()
        let ready = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"), object: app.tabBars.buttons["Vault"])
        XCTAssertEqual(XCTWaiter.wait(for: [ready], timeout: 5), .completed)
        app.tabBars.buttons["Vault"].tap()
        app.tabBars.buttons["Messages"].tap()
        let channel = app.buttons["channel.row.fixture-channel"]
        XCTAssertTrue(channel.waitForExistence(timeout: 5)); channel.tap()
        XCTAssertTrue(app.descendants(matching: .any)["message.row.1"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.descendants(matching: .any)["message.row.2"].waitForExistence(timeout: 20))
        XCTAssertGreaterThan(app.staticTexts["First queued message"].frame.midX, app.frame.midX)
        XCTAssertEqual(app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@", "message.pending.")).count, 0)
        XCTAssertFalse(app.alerts.firstMatch.exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Queued sends survive tab navigation"; shot.lifetime = .keepAlways; add(shot)
    }

    func testUnconfirmedSendStaysInlineAndKeepsTheNextDraft() {
        let app = open(["--ui-channel-send", "--ui-channel-send-failure", "--ui-dark", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"])
        let draft = composer(app)
        draft.tap(); draft.typeText("Message awaiting confirmation")
        app.buttons["message.send"].tap()
        draft.tap(); draft.typeText("Keep my next draft")
        let nextDraft = draft.value as? String
        XCTAssertEqual(nextDraft?.trimmingCharacters(in: .whitespacesAndNewlines), "Keep my next draft")
        let retry = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "message.retry.")).firstMatch
        XCTAssertTrue(retry.waitForExistence(timeout: 20))
        XCTAssertFalse(app.alerts.firstMatch.exists)
        XCTAssertEqual(draft.value as? String, nextDraft)
        XCTAssertTrue(app.buttons["message.send"].isEnabled)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Unconfirmed message · inline · draft preserved"; shot.lifetime = .keepAlways; add(shot)
    }
}
