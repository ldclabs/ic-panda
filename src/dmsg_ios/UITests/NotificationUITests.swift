import XCTest

@MainActor final class NotificationUITests: XCTestCase {
    func testSystemNotificationTapKeepsAppAlive() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launchArguments.append("--ui-reminder-list")
        app.launch()
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 10))
        app.buttons["reminders.menu"].tap(); app.buttons["reminders.settings"].tap()
        let test = app.buttons["notifications.test"]
        XCTAssertTrue(test.waitForExistence(timeout: 5))
        test.tap()
        let system = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let allow = system.alerts.buttons.matching(NSPredicate(format: "label IN %@", ["Allow", "允许"])).firstMatch
        if allow.waitForExistence(timeout: 2) { allow.tap() }
        let banner = system.descendants(matching: .any)["NotificationShortLookView"]
        // Even a notification with no route must finish on main. Repeated
        // taps exercise the actual Objective-C delegate completion bridge.
        for _ in 0..<2 {
            test.tap()
            XCTAssertTrue(banner.waitForExistence(timeout: 5))
            capture(system, "Foreground notification")
            banner.tap()
            XCTAssertTrue(test.waitForExistence(timeout: 5), "The notification must return to the same screen without restarting dMsg")
            XCTAssertEqual(app.state, .runningForeground)
            XCTAssertTrue(test.isHittable)
        }
        capture(app, "Notification taps preserve the running workspace")
        app.terminate()
    }

    func testReadRemindersChineseDark() { readReminders(language: "zh-Hans", locale: "zh_CN", dark: true) }
    func testReadRemindersArabic() { readReminders(language: "ar", locale: "ar_SA", dark: false) }
    func testReadRemindersGerman() { readReminders(language: "de", locale: "de_DE", dark: false) }

    private func readReminders(language: String, locale: String, dark: Bool) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-reminder-services", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launchArguments.append("--ui-reminder-list"); app.launch(); XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 10))
        let menu = app.buttons["reminders.menu"]
        XCTAssertTrue(menu.waitForExistence(timeout: 5))
        let unreadValue = app.buttons.matching(identifier: "reminders.item").element(boundBy: 0).value as? String
        XCTAssertFalse((unreadValue ?? "").isEmpty)
        XCTAssertFalse(app.buttons["reminders.read-all"].exists, "Bulk housekeeping does not occupy the list")
        XCTAssertEqual(app.buttons.matching(identifier: "reminders.item").count, 2)
        capture(app, "Compact service reminders · \(language)")
        let network = app.buttons.matching(identifier: "reminders.item").element(boundBy: 0)
        if language == "ar" { network.swipeRight() } else { network.swipeLeft() }
        app.buttons["reminders.read.network"].tap()
        let readValue = network.value as? String
        XCTAssertNotEqual(readValue, unreadValue)
        network.tap()
        XCTAssertTrue(app.buttons["account.sync"].waitForExistence(timeout: 5))
        capture(app, "Network reminder opens synchronization · \(language)")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(menu.waitForExistence(timeout: 5))
        app.buttons.matching(identifier: "reminders.item").element(boundBy: 1).tap()
        let registration = app.buttons["notifications.registration"]
        XCTAssertTrue(registration.waitForExistence(timeout: 5))
        XCTAssertTrue(registration.isHittable, "The relevant recovery action is above settings")
        capture(app, "Registration reminder offers retry first · \(language)")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(menu.waitForExistence(timeout: 5))
        let read = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", readValue ?? ""), object: app.buttons.matching(identifier: "reminders.item").element(boundBy: 1))
        XCTAssertEqual(XCTWaiter.wait(for: [read], timeout: 5), .completed)
        menu.tap(); app.buttons["reminders.read-all"].tap()
        app.buttons["reminders.confirm-read-all"].firstMatch.tap()
        XCTAssertEqual(app.buttons.matching(identifier: "reminders.item").element(boundBy: 1).value as? String, readValue)
        XCTAssertEqual(app.buttons.matching(identifier: "reminders.item").count, 2, "Read is not business completion")
        capture(app, "Read reminders remain actionable · \(language)")
        app.terminate()
    }

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
        app.launchArguments.append("--ui-reminder-list")
        app.launch()
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 10))
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
        app.launchArguments.append("--ui-reminder-list")
        app.launch()
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 10))
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
        app.launchArguments = ["--ui-testing", "--ui-reminder-services", "--ui-reminder-other-account", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let choose = app.buttons["reminders.choose-account"]
        XCTAssertTrue(choose.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["reminders.unlock"].exists)
        capture(app, "Other account reminder needs an explicit choice")
        choose.tap()
        app.buttons["Cancel"].tap()
        XCTAssertTrue(choose.waitForExistence(timeout: 5), "Cancelling keeps the original account")
        choose.tap()
        app.buttons["accounts.040g2082040g2082040g"].tap()
        let unlock = app.buttons["reminders.unlock"]
        XCTAssertTrue(unlock.waitForExistence(timeout: 5))
        capture(app, "Chosen account still requires unlock")
        unlock.tap()
        XCTAssertTrue(app.buttons["notifications.registration"].waitForExistence(timeout: 5), "Continue the original reminder after account selection and unlock")
        app.terminate()
    }

    func testEmptyReminderListHasDistinctScopes() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-reminder-empty", "-AppleLanguages", "(en)"]
        app.launch()
        XCTAssertTrue(app.staticTexts["Nothing needs your attention"].waitForExistence(timeout: 10))
        capture(app, "No actions pending")
        app.segmentedControls["reminders.scope"].buttons.element(boundBy: 1).tap()
        XCTAssertTrue(app.staticTexts["No reminders yet"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons.matching(identifier: "reminders.item").count, 0)
        app.terminate()
    }

    func testReminderListGermanLargeText() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-reminder-services", "--ui-reminder-list", "-AppleLanguages", "(de)", "-AppleLocale", "de_DE", "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch()
        let rows = app.buttons.matching(identifier: "reminders.item")
        XCTAssertTrue(rows.firstMatch.waitForExistence(timeout: 10))
        capture(app, "Reminder list with accessibility XXXL · de")
        let second = rows.element(boundBy: 1)
        for _ in 0..<8 where !second.isHittable { app.swipeUp() }
        XCTAssertTrue(second.isHittable)
        capture(app, "Long reminder remains reachable · de")
        second.tap()
        XCTAssertTrue(app.buttons["notifications.registration"].waitForExistence(timeout: 5))
        app.terminate()
    }

    func testLockedReminderContinuesAfterUnlockChinese() { lockedReminder(language: "zh-Hans", locale: "zh_CN", dark: false) }
    func testLockedReminderArabicDark() { lockedReminder(language: "ar", locale: "ar_SA", dark: true) }
    func testLockedReminderGermanLargeText() { lockedReminder(language: "de", locale: "de_DE", dark: false, large: true) }

    private func lockedReminder(language: String, locale: String, dark: Bool, large: Bool = false) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-reminder-services", "--ui-reminder-locked", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        if large { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch()
        let unlock = app.buttons["reminders.unlock"]
        XCTAssertTrue(unlock.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["reminders.choose-account"].exists)
        capture(app, "Locked reminder · \(language) · large \(large)")
        for _ in 0..<5 where !unlock.isHittable { app.swipeUp() }
        XCTAssertTrue(unlock.isHittable)
        unlock.tap()
        XCTAssertTrue(app.buttons["notifications.registration"].waitForExistence(timeout: 5))
        capture(app, "Unlock continues original reminder · \(language)")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        let menu = app.buttons["reminders.menu"]
        XCTAssertTrue(menu.waitForExistence(timeout: 5))
        let unreadValue = app.buttons.matching(identifier: "reminders.item").element(boundBy: 0).value as? String ?? ""
        let read = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value != %@", unreadValue), object: app.buttons.matching(identifier: "reminders.item").element(boundBy: 1))
        XCTAssertEqual(XCTWaiter.wait(for: [read], timeout: 5), .completed)
        app.terminate()
    }

    func testCancelledUnlockDoesNotReadReminder() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-reminder-services", "--ui-reminder-locked", "--ui-reminder-cancel-unlock", "-AppleLanguages", "(en)"]
        app.launch()
        let unlock = app.buttons["reminders.unlock"]
        XCTAssertTrue(unlock.waitForExistence(timeout: 10)); unlock.tap()
        XCTAssertTrue(unlock.isHittable)
        app.buttons["reminders.return"].tap()
        let rows = app.buttons.matching(identifier: "reminders.item")
        XCTAssertTrue(rows.element(boundBy: 0).waitForExistence(timeout: 5))
        XCTAssertEqual(rows.element(boundBy: 0).value as? String, rows.element(boundBy: 1).value as? String)
        app.terminate()
    }

    func testFailedReminderKeepsUnreadAndOffersRetry() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-reminder-services", "--ui-reminder-failed", "-AppleLanguages", "(zh-Hans)"]
        app.launch()
        let retry = app.buttons["reminders.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 10)); retry.tap()
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        capture(app, "Failed reminder has retry and return")
        app.buttons["reminders.return"].tap()
        let rows = app.buttons.matching(identifier: "reminders.item")
        XCTAssertTrue(rows.element(boundBy: 0).waitForExistence(timeout: 5))
        XCTAssertEqual(rows.element(boundBy: 0).value as? String, rows.element(boundBy: 1).value as? String)
        app.terminate()
    }

    func testExpiredAndUnavailableRemindersHaveScopedResults() {
        for flag in ["--ui-reminder-expired", "--ui-reminder-ended"] {
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "--ui-reminder-services", flag, "-AppleLanguages", "(en)"]
            app.launch()
            XCTAssertTrue(app.buttons["reminders.return"].waitForExistence(timeout: 10))
            capture(app, "Scoped reminder result \(flag)")
            app.buttons["reminders.return"].tap()
            XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 5))
            XCTAssertFalse(app.staticTexts["This request has expired"].exists)
            XCTAssertFalse(app.staticTexts["Reminder unavailable"].exists)
            app.terminate()
        }
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
        app.buttons["account.profile"].tap()
        XCTAssertTrue(app.staticTexts["040g2081040g2081040g"].waitForExistence(timeout: 5))
        app.terminate()
    }

    func testSecondAccountConsentEnglish() { consent(language: "en", locale: "en_US", dark: false, confirm: "Turn on for this account") }
    func testSecondAccountConsentArabicDark() { consent(language: "ar", locale: "ar_SA", dark: true, confirm: "تفعيل لهذا الحساب") }

    func testSetupPromptCanBeSetAside() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-notification-setup", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launchArguments.append("--ui-reminder-list")
        app.launch()
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 10))
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
        app.launchArguments.append("--ui-reminder-list")
        app.launch()
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 10))
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
        app.launchArguments.append("--ui-reminder-list")
        app.launch()
        XCTAssertTrue(app.segmentedControls["reminders.scope"].waitForExistence(timeout: 10))
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
