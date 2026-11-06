import XCTest

@MainActor
final class AccountInitializationUITests: XCTestCase {
    func testUnknownOutcomeRestoresControlsAndRetryContinuesSetup() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-setup", "--ui-setup-failure",
            "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        let initialize = app.buttons["login.initialize"]
        XCTAssertTrue(initialize.waitForExistence(timeout: 5))
        initialize.tap()
        XCTAssertFalse(initialize.isEnabled)
        XCTAssertEqual(initialize.label, "正在初始化账户…")
        XCTAssertFalse(app.buttons["login.join"].isEnabled)
        XCTAssertFalse(app.buttons["login.start"].isEnabled)
        capture(app, "Chinese account initialization in progress")

        // Both the initial request and its one automatic reconciliation fail.
        let alert = app.alerts.firstMatch
        XCTAssertTrue(alert.waitForExistence(timeout: 25))
        XCTAssertTrue(alert.staticTexts["尚未确认账户初始化结果。请检查网络连接后重试，应用会继续已保存的初始化进度。"].exists)
        alert.buttons.firstMatch.tap()
        XCTAssertTrue(initialize.isEnabled)
        XCTAssertTrue(app.buttons["login.start"].isEnabled)
        XCTAssertFalse(app.buttons["login.join"].exists, "The account already exists; only its initial root remains pending")
        initialize.tap()
        XCTAssertFalse(initialize.isEnabled)
        XCTAssertTrue(app.buttons["我的"].waitForExistence(timeout: 15))
        XCTAssertFalse(app.alerts.firstMatch.exists)
    }

    func testSlowNetworkShowsWaitingFeedbackAndReconcilesInDarkArabic() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-setup", "--ui-setup-slow", "--ui-dark",
            "-AppleLanguages", "(ar)", "-AppleLocale", "ar_SA"]
        app.launch()
        let initialize = app.buttons["login.initialize"]
        XCTAssertTrue(initialize.waitForExistence(timeout: 5))
        initialize.tap()
        XCTAssertFalse(initialize.isEnabled)
        XCTAssertFalse(app.buttons["login.join"].isEnabled)
        XCTAssertFalse(app.buttons["login.start"].isEnabled)
        capture(app, "Dark Arabic account initialization in progress")
        let waiting = app.staticTexts["تستغرق الشبكة وقتًا أطول من المعتاد. ما زلنا ننتظر التأكيد؛ تم حفظ تقدم الإعداد."]
        for _ in 0..<3 where !waiting.isHittable { app.swipeUp() }
        XCTAssertTrue(waiting.waitForExistence(timeout: 20))
        capture(app, "Dark Arabic slow network feedback")
        XCTAssertTrue(app.buttons["الحساب"].waitForExistence(timeout: 20))
        XCTAssertFalse(app.alerts.firstMatch.exists, "One lost response is reconciled without an error alert")
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways
        add(attachment)
    }
}
