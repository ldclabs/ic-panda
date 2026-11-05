import XCTest

@MainActor
final class SyncUITests: XCTestCase {
    func testPausedSyncDoesNotInterruptBrowsing() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-files-rich", "--ui-channel", "--ui-sync-paused", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "workspace.feedback").firstMatch.exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Files without an automatic sync-paused notice"; shot.lifetime = .keepAlways; add(shot)
        app.tabBars.buttons.element(boundBy: 1).tap()
        XCTAssertTrue(app.buttons["messages.mode.conversations"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["channel.row.fixture-channel"].exists)
        XCTAssertFalse(app.otherElements["sync.status"].exists)
        XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "workspace.feedback").firstMatch.exists)
        let messages = XCTAttachment(screenshot: app.screenshot())
        messages.name = "Messages without an inline sync status"; messages.lifetime = .keepAlways; add(messages)
        app.buttons["workspace.status"].tap()
        XCTAssertTrue(app.otherElements["sync.status"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["同步已暂停"].exists)
        XCTAssertFalse(app.buttons["sync.retry"].isEnabled)
    }

    private func inspect(_ language: String, _ locale: String, dark: Bool, paused: Bool = false) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", paused ? "--ui-sync-paused" : "--ui-first-sync",
                               "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        let status = app.buttons["workspace.status"].firstMatch
        XCTAssertTrue(status.waitForExistence(timeout: 8)); status.tap()
        XCTAssertTrue(app.otherElements["sync.status"].waitForExistence(timeout: 8))
        if paused { XCTAssertTrue(app.buttons["sync.retry"].exists) }
        else { XCTAssertTrue(app.staticTexts["sync.entries"].exists) }
        let vault = XCTAttachment(screenshot: app.screenshot())
        vault.name = "First sync \(language) \(dark ? "dark" : "light") \(paused ? "paused" : "running")"
        vault.lifetime = .keepAlways; add(vault)
        app.buttons["workspace.status.done"].tap()
        // Browsing keeps sync details in the toolbar instead of adding list rows.
        let tabs = app.tabBars.buttons
        XCTAssertEqual(tabs.count, 4)
        tabs.element(boundBy: 1).tap()
        XCTAssertTrue(app.buttons["workspace.status"].exists)
        XCTAssertFalse(app.otherElements["sync.status"].exists)
        tabs.element(boundBy: 3).tap()
        let settings = app.buttons["account.sync-settings"]
        for _ in 0..<4 where !settings.isHittable { app.swipeUp() }
        XCTAssertTrue(settings.waitForExistence(timeout: 5)); settings.tap()
        let sync = app.buttons["account.sync"]
        for _ in 0..<7 where !sync.isHittable { app.swipeUp() }
        XCTAssertTrue(sync.isHittable)
        if paused { XCTAssertFalse(sync.isEnabled) }
        let account = XCTAttachment(screenshot: app.screenshot())
        account.name = "Account sync scope \(language)"; account.lifetime = .keepAlways; add(account)
        app.terminate()
    }

    func testServerCooldownExpiresWithoutAnotherStatusChange() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-sync-paused", "--ui-sync-short-pause", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let status = app.buttons["workspace.status"].firstMatch
        XCTAssertTrue(status.waitForExistence(timeout: 5)); status.tap()
        let retry = app.buttons["sync.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        XCTAssertFalse(retry.isEnabled)
        let available = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: retry)
        XCTAssertEqual(XCTWaiter.wait(for: [available], timeout: 15), .completed)
        app.terminate()
    }

    func testFirstSyncEnglishLight() { inspect("en", "en_US", dark: false) }

    func testFirstSyncChineseDark() { inspect("zh-Hans", "zh_CN", dark: true) }

    func testFirstSyncFrench() { inspect("fr", "fr_FR", dark: false) }

    func testFirstSyncArabicRTL() { inspect("ar", "ar_SA", dark: false) }

    func testRetryDeadlineChinese() { inspect("zh-Hans", "zh_CN", dark: false, paused: true) }
}
