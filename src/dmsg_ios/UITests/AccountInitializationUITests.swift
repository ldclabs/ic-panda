import XCTest

@MainActor
final class AccountInitializationUITests: XCTestCase {
    func testNewPasskeyLoginOffersWorkspaceCreationBeforeUnlock() {
        for (language, dark) in [("zh-Hans", false), ("zh-Hans", true), ("en", false), ("de", false), ("ar", true)] {
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "--ui-setup", "--ui-passkey", "-AppleLanguages", "(\(language))"]
                + (dark ? ["--ui-dark"] : [])
            app.launch()
            let start = app.buttons["login.initialize"]
            XCTAssertTrue(start.waitForExistence(timeout: 5))
            XCTAssertTrue(start.isHittable)
            if language == "zh-Hans" { XCTAssertEqual(start.label, "开始使用") }
            XCTAssertFalse(app.buttons["login.unlock"].exists, "A renewable passkey login has no workspace to unlock yet")
            XCTAssertFalse(app.buttons["login.start"].exists)
            XCTAssertFalse(app.buttons["login.ii"].exists)
            XCTAssertTrue(app.buttons["login.join"].isHittable)
            XCTAssertTrue(app.staticTexts["login.setup-guidance"].isHittable)
            capture(app, "New passkey workspace · \(language) · \(dark ? "dark" : "light")")
            app.buttons["login.switch-account"].tap()
            XCTAssertTrue(app.buttons["login.start"].waitForExistence(timeout: 3))
            XCTAssertTrue(app.buttons["login.ii"].exists)
            XCTAssertFalse(app.buttons["login.initialize"].exists)
            app.buttons["login.cancel-account"].tap()
            XCTAssertTrue(start.waitForExistence(timeout: 3))
            XCTAssertFalse(app.buttons["login.unlock"].exists)
            app.terminate()
        }
    }

    func testNewPasskeySetupKeepsExistingAccountBindingAvailable() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-setup", "--ui-passkey",
            "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.buttons["login.initialize"].waitForExistence(timeout: 5))
        app.buttons["login.join"].tap()
        XCTAssertTrue(app.textFields["Account ID"].waitForExistence(timeout: 3))
        let pushed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == false"), object: app.buttons["login.initialize"])
        XCTAssertEqual(XCTWaiter.wait(for: [pushed], timeout: 5), .completed)
        XCTAssertFalse(app.buttons["Create binding request"].isEnabled)
        capture(app, "Existing account binding after passkey sign-in")
    }

    func testNewPasskeySetupRemainsReachableInLargeArabic() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-setup", "--ui-passkey", "--ui-dark",
            "-AppleLanguages", "(ar)", "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch()
        let start = app.buttons["login.initialize"]
        XCTAssertTrue(start.waitForExistence(timeout: 5))
        for _ in 0..<6 where !start.isHittable { app.swipeUp() }
        XCTAssertTrue(start.isHittable)
        XCTAssertFalse(app.buttons["login.unlock"].exists)
        capture(app, "New passkey workspace · Arabic large text")
        let join = app.buttons["login.join"]
        for _ in 0..<6 where !join.isHittable { app.swipeUp() }
        XCTAssertTrue(join.isHittable)
    }

    func testUnknownOutcomeRestoresControlsAndRetryContinuesSetup() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-setup", "--ui-passkey", "--ui-setup-failure",
            "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        let initialize = app.buttons["login.initialize"]
        XCTAssertTrue(initialize.waitForExistence(timeout: 5))
        initialize.tap()
        XCTAssertFalse(initialize.isEnabled)
        XCTAssertEqual(initialize.label, "正在准备工作空间…")
        XCTAssertFalse(app.buttons["login.join"].isEnabled)
        XCTAssertFalse(app.buttons["login.switch-account"].isEnabled)
        XCTAssertFalse(app.buttons["login.unlock"].exists)
        capture(app, "Chinese account initialization in progress")

        // Both the initial request and its one automatic reconciliation fail.
        let alert = app.alerts.firstMatch
        XCTAssertTrue(alert.waitForExistence(timeout: 25))
        XCTAssertTrue(alert.staticTexts["工作空间尚未准备完成。请检查网络连接后重试，我们会继续上次的进度。"].exists)
        alert.buttons.firstMatch.tap()
        XCTAssertTrue(initialize.isEnabled)
        XCTAssertTrue(app.buttons["login.switch-account"].isEnabled)
        XCTAssertFalse(app.buttons["login.unlock"].exists)
        XCTAssertFalse(app.buttons["login.start"].exists)
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
        XCTAssertFalse(app.buttons["login.switch-account"].isEnabled)
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
