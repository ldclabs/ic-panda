import XCTest

@MainActor final class AccountSettingsUITests: XCTestCase {
    private func open(_ extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"] + extra
        app.launch()
        XCTAssertTrue(app.buttons["Account"].waitForExistence(timeout: 8))
        app.buttons["Account"].tap()
        return app
    }

    private func tap(_ label: String, in app: XCUIApplication) {
        let button = app.buttons[label].firstMatch
        _ = button.waitForExistence(timeout: 1)
        for _ in 0..<12 where !button.isHittable { app.swipeUp() }
        XCTAssertTrue(button.isHittable, label); button.tap()
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }

    func testAccountSwitcherShowsNamesAndIDs() {
        checkAccountNames(language: "en", locale: "en_US", tab: "Account", personal: "Personal", dark: false)
    }

    func testAccountSwitcherShowsNamesInDarkChinese() {
        checkAccountNames(language: "zh-Hans", locale: "zh_CN", tab: "我的", personal: "个人账户", dark: true)
    }

    private func checkAccountNames(language: String, locale: String, tab: String, personal: String, dark: Bool) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(\(language))", "-AppleLocale", locale] + (dark ? ["--ui-dark"] : [])
        app.launch()
        XCTAssertTrue(app.buttons[tab].waitForExistence(timeout: 8)); app.buttons[tab].tap()
        tap("account.switch", in: app)
        let current = app.descendants(matching: .any).matching(identifier: "accounts.040g2081040g2081040g").firstMatch
        let other = app.buttons["accounts.040g2082040g2082040g"]
        let unnamed = app.buttons["accounts.040g2083040g2083040g"]
        XCTAssertTrue(current.waitForExistence(timeout: 5))
        XCTAssertTrue(current.label.contains("Studio")); XCTAssertTrue(current.label.contains("040g2081040g2081040g"))
        XCTAssertTrue(other.label.contains(personal)); XCTAssertTrue(other.label.contains("040g2082040g2082040g"))
        XCTAssertTrue(unnamed.label.contains("040g2083040g2083040g"))
        XCTAssertFalse(app.buttons["accounts.040g2081040g2081040g"].exists); XCTAssertTrue(other.isEnabled)
        capture(app, "Named account switcher · \(language)")
    }

    func testMemberDeviceCanDisputeRecoveryWithoutEditingPolicy() {
        let app = open(["--ui-member-security"])
        tap("account.security", in: app)
        let toggle = app.switches["On-chain recovery"]
        for _ in 0..<10 where !toggle.isHittable { app.swipeUp() }
        XCTAssertTrue(toggle.exists); XCTAssertFalse(toggle.isEnabled)
        let dispute = app.buttons["Dispute pending recovery"]
        for _ in 0..<10 where !dispute.isHittable { app.swipeUp() }
        XCTAssertTrue(dispute.isHittable); XCTAssertTrue(dispute.isEnabled)
        capture(app, "Member can dispute recovery while policy stays read-only")
    }

    func testExpiredRecoveryOffersANewRequestInsteadOfCompletingTheExpiredOne() {
        let app = open(["--ui-recovery-expired"])
        tap("account.security", in: app)
        tap("Account recovery", in: app)
        XCTAssertTrue(app.buttons["recovery.request"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["recovery.request"].isEnabled)
        XCTAssertFalse(app.buttons["recovery.complete"].exists)
        XCTAssertTrue(app.staticTexts["Expired"].exists)
        capture(app, "Expired recovery can be requested again")
    }

    func testGlobalNotificationSettingsAreReadableBeforeLogin() {
        let app = XCUIApplication()
        app.launchArguments = ["-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.buttons["login.start"].waitForExistence(timeout: 10))
        tap("reminders.open", in: app)
        tap("reminders.menu", in: app)
        tap("reminders.settings", in: app)
        XCTAssertTrue(app.switches["notifications.enabled"].waitForExistence(timeout: 10))
        capture(app, "Global notification settings before login")
    }

    func testAccountMenuHasNoDemoOrSafetySupportAndKeepsAccountManagement() {
        let app = open()
        XCTAssertFalse(app.buttons["Try the local demo"].exists)
        XCTAssertFalse(app.buttons["Safety and support"].exists)
        tap("account.security", in: app)
        tap("account.local-data", in: app)
        XCTAssertTrue(app.buttons["account.remove-local"].waitForExistence(timeout: 5))
        // An unknown local inventory cannot authorize removing data.
        XCTAssertFalse(app.buttons["account.remove-local"].isEnabled)
        capture(app, "Independent local removal requires a known inventory")
    }

    func testPublicProfileKeepsDraftWhenLeavingAndLabelsRemainVisible() {
        let app = open()
        tap("account.profile", in: app)
        let name = app.textFields["profile.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        for _ in 0..<8 where !name.isHittable { app.swipeUp() }
        name.tap(); name.typeText(" Draft")
        XCTAssertTrue(app.staticTexts["Name"].exists)
        XCUIDevice.shared.press(.home)
        app.activate()
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        XCTAssertTrue((name.value as? String ?? "").contains("Draft"))
        app.buttons["Back"].firstMatch.tap()
        tap("Keep draft and leave", in: app)
        tap("account.profile", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        XCTAssertTrue((name.value as? String ?? "").contains("Draft"))
        capture(app, "Public profile keeps the unsaved draft")
    }

    func testNotificationDraftSurvivesReturningToTheForeground() {
        let app = open()
        tap("Notifications", in: app)
        let enabled = app.switches["notifications.enabled"]
        XCTAssertTrue(enabled.waitForExistence(timeout: 5))
        let original = enabled.value as? String
        enabled.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        XCTAssertNotEqual(enabled.value as? String, original)
        XCUIDevice.shared.press(.home)
        app.activate()
        XCTAssertTrue(enabled.waitForExistence(timeout: 5))
        XCTAssertNotEqual(enabled.value as? String, original)
        capture(app, "Notification draft survives a foreground refresh")
    }

    func testRetryingPublicationPreservesTheUnsubmittedDraftAndItsVersionConflict() {
        let app = open(["--ui-profile-pending"])
        tap("account.profile", in: app)
        let name = app.textFields["profile.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        XCTAssertEqual(name.value as? String, "Local draft")
        tap("Retry publication", in: app)
        XCTAssertEqual(name.value as? String, "Local draft")
        XCTAssertTrue(app.staticTexts["Saved profile published. Your draft has not been submitted."].exists)
        XCTAssertTrue(app.buttons["Reload saved profile"].exists)
        XCTAssertFalse(app.buttons["profile.publish"].isEnabled)
        tap("Reload saved profile", in: app)
        XCTAssertEqual(name.value as? String, "Local draft")
        XCTAssertTrue(app.buttons["profile.publish"].isEnabled)
        capture(app, "Republishing saved data retains the unsubmitted draft")
    }

    func testAccountIDCopyDoesNotOpenProfile() {
        let app = open()
        tap("account.copy-id", in: app)
        XCTAssertTrue(app.staticTexts["Copied"].waitForExistence(timeout: 3))
        XCTAssertFalse(app.textFields["profile.name"].exists)
        XCTAssertTrue(app.buttons["account.profile"].exists)
        capture(app, "Account ID has an independent copy action")
        tap("account.profile", in: app)
        XCTAssertTrue(app.textFields["profile.name"].waitForExistence(timeout: 5))
        tap("profile.copy-id", in: app)
        XCTAssertTrue(app.staticTexts["Copied"].waitForExistence(timeout: 3))
        XCTAssertTrue(app.textFields["profile.name"].exists)
    }

    func testAutoFillSelectionSurvivesSearchAndReopening() {
        let app = open()
        tap("System integrations", in: app)
        tap("Select login items", in: app)
        let github = app.switches["GitHub"].firstMatch
        XCTAssertTrue(github.waitForExistence(timeout: 5))
        github.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        XCTAssertEqual(github.value as? String, "1")
        let search = app.searchFields.firstMatch
        search.tap(); search.typeText("Work")
        let mail = app.switches["Work email"].firstMatch
        XCTAssertTrue(mail.waitForExistence(timeout: 5))
        mail.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        XCTAssertEqual(mail.value as? String, "1")
        tap("autofill.done", in: app)
        tap("Select login items", in: app)
        XCTAssertTrue(github.waitForExistence(timeout: 5))
        XCTAssertEqual(github.value as? String, "1")
        XCTAssertEqual(mail.value as? String, "1")
        capture(app, "AutoFill retains selection across search and reopening")
    }

    func testSystemExportBiometryChoiceSurvivesReturningToTheForeground() {
        let app = open()
        tap("System integrations", in: app)
        let biometry = app.switches["Biometrics only for exported copies"]
        for _ in 0..<8 where !biometry.isHittable { app.swipeUp() }
        XCTAssertTrue(biometry.isHittable)
        biometry.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        XCTAssertEqual(biometry.value as? String, "1")
        XCUIDevice.shared.press(.home)
        app.activate()
        XCTAssertTrue(biometry.waitForExistence(timeout: 5))
        let relaxed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "0"), object: biometry)
        relaxed.isInverted = true
        XCTAssertEqual(XCTWaiter.wait(for: [relaxed], timeout: 2), .completed)
        capture(app, "Biometrics-only export choice survives a foreground policy refresh")
    }

    func testMessagingOwnsBlockingAndReportHistory() {
        let app = open()
        app.buttons["Messages"].tap()
        tap("messages.settings", in: app)
        tap("messages.blocked", in: app)
        XCTAssertTrue(app.staticTexts["No blocked accounts"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        tap("messages.reports", in: app)
        XCTAssertTrue(app.staticTexts["No reports"].waitForExistence(timeout: 5))
        capture(app, "Report history remains accessible in message settings")
    }

    func testUsageShowsConsumptionSeparatelyFromLimits() {
        let app = open()
        tap("account.plan", in: app)
        XCTAssertTrue(app.staticTexts["Storage limit"].waitForExistence(timeout: 5))
        for _ in 0..<5 where !app.staticTexts["Used"].isHittable { app.swipeUp() }
        XCTAssertTrue(app.staticTexts["Used"].exists)
        XCTAssertTrue(app.staticTexts["Reserved for uploads"].exists)
        XCTAssertTrue(app.staticTexts["Remaining"].firstMatch.exists)
        capture(app, "Current usage with reserved and remaining capacity")
    }
}
