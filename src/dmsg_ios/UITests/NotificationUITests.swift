import XCTest

@MainActor final class NotificationUITests: XCTestCase {
    func testReminderSettingsEnglish() { check(language: "en", locale: "en_US", dark: false) }
    func testReminderSettingsDarkChinese() { check(language: "zh-Hans", locale: "zh_CN", dark: true) }
    func testReminderSettingsArabicRTL() { check(language: "ar", locale: "ar_SA", dark: false) }
    func testReminderSettingsGerman() { check(language: "de", locale: "de_DE", dark: false) }

    func testRegistrationRetryChineseDark() { registrationRetry(language: "zh-Hans", locale: "zh_CN", dark: true) }
    func testRegistrationRetryArabic() { registrationRetry(language: "ar", locale: "ar_SA", dark: false) }
    func testRegistrationRetryGerman() { registrationRetry(language: "de", locale: "de_DE", dark: false) }

    private func registrationRetry(language: String, locale: String, dark: Bool) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-push-retry", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        app.openActivity("reminders.open")
        app.buttons["reminders.menu"].tap(); app.buttons["reminders.settings"].tap()
        let retry = app.buttons["notifications.registration"]
        for _ in 0..<8 where !retry.exists || !retry.isHittable { app.swipeUp() }
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        XCTAssertEqual(app.activityIndicators.count, 0, "A completed failure must stop spinning")
        XCTAssertTrue(app.descendants(matching: .any)["notifications.pending"].exists)
        capture(app, "Registration failed · \(language)")
        retry.tap()
        let busy = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == false"), object: retry)
        XCTAssertEqual(XCTWaiter.wait(for: [busy], timeout: 2), .completed)
        XCTAssertFalse((retry.value as? String ?? "").isEmpty, "The action exposes its progress to accessibility")
        let indicators = app.activityIndicators.allElementsBoundByIndex
        XCTAssertFalse(indicators.isEmpty)
        for indicator in indicators {
            XCTAssertTrue(retry.frame.insetBy(dx: -1, dy: -1).contains(indicator.frame), "Progress must stay inside the registration action")
        }
        capture(app, "Registration in progress · \(language)")
        let finished = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: retry)
        XCTAssertEqual(XCTWaiter.wait(for: [finished], timeout: 5), .completed)
        XCTAssertTrue(app.descendants(matching: .any)["notifications.status"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["notifications.pending"].exists)
        XCTAssertEqual(app.activityIndicators.count, 0)
        capture(app, "Registration completed · \(language)")
        app.terminate()
    }

    func testLeavingSettingsFinishesAnInFlightSave() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-notification-slow-save", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        app.openActivity("reminders.open")
        app.buttons["reminders.menu"].tap(); app.buttons["reminders.settings"].tap()
        let enabled = app.switches["notifications.enabled"]
        XCTAssertTrue(enabled.waitForExistence(timeout: 5))
        enabled.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        let saving = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == false"), object: enabled)
        XCTAssertEqual(XCTWaiter.wait(for: [saving], timeout: 3), .completed)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 5))
        // This prompt observes the saved preferences, not the settings draft.
        XCTAssertTrue(app.buttons["notifications.setup.open"].waitForExistence(timeout: 10))
        capture(app, "Leaving settings finishes the notification save")
    }

    func testOtherAccountReminderKeepsCurrentWorkspace() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-notification-setup", "--ui-reminder-other-account", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "040g2082040g2082040g")).firstMatch.exists)
        XCTAssertFalse(app.buttons["reminders.account"].exists)
        XCTAssertFalse(app.buttons["reminders.switch"].exists)
        // Both rows belong to the active account, even though the entry names
        // the other local account. A notification cannot select its workspace.
        XCTAssertTrue(app.staticTexts["Account security"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Approvals"].exists)
        capture(app, "Reminder from another account")
        app.buttons["Done"].tap()
        app.tabBars.buttons["Account"].tap()
        XCTAssertTrue(app.staticTexts["040g2081040g2081040g"].waitForExistence(timeout: 5))
        app.terminate()
    }

    func testBrowserRequestDoesNotOfferAccountSwitch() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-browser-other-account", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "040g2082040g2082040g")).firstMatch.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["Switch to this account"].exists)
        capture(app, "Browser request from another account")
        app.buttons["Done"].tap()
        app.tabBars.buttons["Account"].tap()
        XCTAssertTrue(app.staticTexts["040g2081040g2081040g"].waitForExistence(timeout: 5))
        app.terminate()
    }

    func testSecondAccountConsentEnglish() { consent(language: "en", locale: "en_US", dark: false, confirm: "Turn on for this account") }
    func testSecondAccountConsentArabicDark() { consent(language: "ar", locale: "ar_SA", dark: true, confirm: "تفعيل لهذا الحساب") }

    func testSetupPromptCanBeSetAside() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-notification-setup", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        app.openActivity("reminders.open")
        let later = app.buttons["notifications.setup.later"]
        XCTAssertTrue(later.waitForExistence(timeout: 5))
        capture(app, "Security alert prompt · zh-Hans")
        later.tap()
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: later)
        XCTAssertEqual(XCTWaiter.wait(for: [gone], timeout: 3), .completed)
        app.terminate()
    }

    /// The prompt only opens settings; turning on a second account asks first.
    private func consent(language: String, locale: String, dark: Bool, confirm: String) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-notification-setup", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        app.openActivity("reminders.open")
        let open = app.buttons["notifications.setup.open"]
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        capture(app, "Security alert prompt · \(language)")
        open.tap()
        let account = app.switches["notifications.account"]
        for _ in 0..<8 where !account.exists || !account.isHittable { app.swipeUp() }
        XCTAssertTrue(account.waitForExistence(timeout: 5))
        XCTAssertEqual(account.value as? String, "0")
        account.coordinate(withNormalizedOffset: CGVector(dx: language == "ar" ? 0.08 : 0.92, dy: 0.5)).tap()
        let changed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "1"), object: account)
        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 3), .completed)
        // The change saves by itself, and a second account still needs consent first.
        let accept = app.buttons[confirm]
        XCTAssertTrue(accept.waitForExistence(timeout: 5))
        capture(app, "Second account consent · \(language)")
        accept.tap()
        let pending = app.descendants(matching: .any)["notifications.pending"]
        for _ in 0..<8 where !pending.exists { app.swipeUp() }
        XCTAssertTrue(pending.waitForExistence(timeout: 5))
        XCTAssertEqual(account.value as? String, "1")
        app.terminate()
    }

    private func check(language: String, locale: String, dark: Bool) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        app.openActivity("reminders.open")
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["reminders.account"].exists)
        XCTAssertFalse(app.buttons["reminders.switch"].exists)
        capture(app, "Reminders · \(language)")
        app.buttons["reminders.menu"].tap()
        app.buttons["reminders.settings"].tap()
        let enabled = app.switches["notifications.enabled"]
        XCTAssertTrue(enabled.waitForExistence(timeout: 5))
        XCTAssertEqual(enabled.value as? String, "1")
        // SwiftUI exposes the whole labeled row as the switch's frame. Tap its
        // native thumb, on the leading side in RTL, rather than the label center.
        enabled.coordinate(withNormalizedOffset: CGVector(dx: language == "ar" ? 0.08 : 0.92, dy: 0.5)).tap()
        let changed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "0"), object: enabled)
        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 3), .completed)
        XCTAssertEqual(enabled.value as? String, "0")
        capture(app, "Notification settings · \(language)")
        // There is no Save button to reach: the change saves itself shortly after.
        let pending = app.descendants(matching: .any)["notifications.pending"]
        for _ in 0..<8 where !pending.exists { app.swipeUp() }
        XCTAssertTrue(pending.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["notifications.save"].exists)
        capture(app, "Saved notification settings · \(language)")
        app.terminate()
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
}
