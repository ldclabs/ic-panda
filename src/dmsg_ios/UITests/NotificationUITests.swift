import XCTest

@MainActor final class NotificationUITests: XCTestCase {
    func testReminderSettingsEnglish() { check(language: "en", locale: "en_US", dark: false) }
    func testReminderSettingsDarkChinese() { check(language: "zh-Hans", locale: "zh_CN", dark: true) }
    func testReminderSettingsArabicRTL() { check(language: "ar", locale: "ar_SA", dark: false) }

    func testSecondAccountConsentEnglish() { consent(language: "en", locale: "en_US", dark: false, confirm: "Turn on for this account") }
    func testSecondAccountConsentArabicDark() { consent(language: "ar", locale: "ar_SA", dark: true, confirm: "تفعيل لهذا الحساب") }

    func testSetupPromptCanBeSetAside() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-notification-setup", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        let reminders = app.buttons["reminders.open"].firstMatch
        XCTAssertTrue(reminders.waitForExistence(timeout: 10)); reminders.tap()
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
        let reminders = app.buttons["reminders.open"].firstMatch
        XCTAssertTrue(reminders.waitForExistence(timeout: 10)); reminders.tap()
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
        let save = app.buttons["notifications.save"]
        for _ in 0..<8 where !save.isHittable { app.swipeUp() }
        XCTAssertTrue(save.isEnabled); save.tap()
        let accept = app.buttons[confirm]
        XCTAssertTrue(accept.waitForExistence(timeout: 5))
        capture(app, "Second account consent · \(language)")
        accept.tap()
        for _ in 0..<8 where !save.exists || !save.isHittable { app.swipeUp() }
        let saved = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == false"), object: save)
        XCTAssertEqual(XCTWaiter.wait(for: [saved], timeout: 5), .completed)
        app.terminate()
    }

    private func check(language: String, locale: String, dark: Bool) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        let reminders = app.buttons["reminders.open"].firstMatch
        XCTAssertTrue(reminders.waitForExistence(timeout: 10)); reminders.tap()
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 5))
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
        let save = app.buttons["notifications.save"]
        for _ in 0..<8 where !save.isHittable { app.swipeUp() }
        XCTAssertTrue(save.isHittable); XCTAssertTrue(save.isEnabled); save.tap()
        // The pending-sync explanation inserts rows above Save, which can move
        // it off screen with longer translations. Find it in the new layout.
        for _ in 0..<8 where !save.exists || !save.isHittable { app.swipeUp() }
        XCTAssertTrue(save.waitForExistence(timeout: 3))
        XCTAssertFalse(save.isEnabled)
        capture(app, "Saved notification settings · \(language)")
        app.terminate()
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
}
