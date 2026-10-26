import XCTest

@MainActor final class NavigationTests: XCTestCase {
    private func tab(_ app: XCUIApplication, symbol: String, title: String) -> XCUIElement {
        let adaptive = app.descendants(matching: .any).matching(identifier: symbol).firstMatch
        return adaptive.waitForExistence(timeout: 1) ? adaptive : app.tabBars.buttons[title]
    }
    func testCompactVaultAndLockClearContent() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.staticTexts["GitHub"].firstMatch.waitForExistence(timeout: 10))
        tab(app, symbol: "person.crop.circle", title: "Account").tap()
        if !app.buttons["account.lock"].isHittable { app.swipeUp() }
        app.buttons["account.lock"].tap()
        XCTAssertTrue(app.buttons["login.start"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["GitHub"].exists)
    }

    func testSecretFieldsRemainHiddenUntilRequested() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let item = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 10))
        item.tap()
        XCTAssertFalse(app.staticTexts["fixture-secret"].exists)
        XCTAssertTrue(app.buttons["vault.reveal.password"].waitForExistence(timeout: 5))
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "Secret fields remain concealed"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testDraftSurvivesWindowShapeChanges() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        tab(app, symbol: "bubble.left.and.bubble.right", title: "Messages").tap()
        let row = app.buttons["channel.row.fixture-channel"]
        // Regular-width portrait devices can keep the inactive tab's sidebar
        // collapsed. Exercise its system navigation before the draft assertion.
        if !row.waitForExistence(timeout: 2) {
            let sidebar = app.buttons["Show Sidebar"].firstMatch
            if sidebar.waitForExistence(timeout: 2) { sidebar.tap() }
        }
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        let composer = app.descendants(matching: .any).matching(identifier: "message.draft").firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 5))
        composer.tap(); composer.typeText("Keep this draft")
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue((composer.value as? String)?.contains("Keep this draft") == true)
        XCUIDevice.shared.orientation = .portrait
        XCTAssertTrue((composer.value as? String)?.contains("Keep this draft") == true)
    }

    func testLargeArabicLayoutKeepsFieldActionsReachable() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(ar)", "-AppleLocale", "ar",
                               "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch()
        let item = app.buttons["vault.favorite.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 5)); item.tap()
        XCTAssertEqual(app.staticTexts["vault.detail.title"].label, "GitHub")
        let reveal = app.buttons["vault.reveal.password"]
        XCTAssertTrue(reveal.waitForExistence(timeout: 5))
        if !reveal.isHittable { app.swipeUp() }
        XCTAssertTrue(reveal.isHittable)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Arabic accessibility layout"
        shot.lifetime = .keepAlways; add(shot)
    }

    func testSnapshotSelectionIsExplicitAndSurvivesRotation() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let item = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 10)); item.tap()
        let share = app.buttons["vault.share"]
        if !share.isHittable { app.swipeUp() }
        XCTAssertTrue(share.waitForExistence(timeout: 5)); share.tap()
        let password = app.switches["secret.snapshot.field.password"]
        let username = app.switches["secret.snapshot.field.username"]
        XCTAssertTrue(password.waitForExistence(timeout: 5))
        XCTAssertEqual(password.value as? String, "0")
        XCTAssertEqual(username.value as? String, "0")
        // SwiftUI exposes the whole Form row as a switch, but its center can
        // be label whitespace. Hit the actual trailing control in this LTR test.
        password.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        XCTAssertEqual(password.value as? String, "1", "The field must be selected before resizing")
        XCUIDevice.shared.orientation = .landscapeLeft
        XCUIDevice.shared.orientation = .portrait
        XCTAssertEqual(password.value as? String, "1")
        XCTAssertEqual(username.value as? String, "0")
        let prepare = app.buttons["secret.snapshot.prepare"]
        for _ in 0..<4 where !prepare.isHittable { app.swipeUp() }
        XCTAssertTrue(prepare.isEnabled)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Explicit secret snapshot selection"; shot.lifetime = .keepAlways; add(shot)
    }

    func testHardwarePageDoesNotInventAvailabilityWithoutCoreData() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        tab(app, symbol: "person.crop.circle", title: "Account").tap()
        let hardware = app.buttons["account.hardware"]
        XCTAssertTrue(hardware.waitForExistence(timeout: 5)); hardware.tap()
        XCTAssertTrue(app.buttons["Retry loading hardware security"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Review key registration"].exists)
    }

    func testRecoveryWarningKeepsDetailNavigationReachable() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-recovery-pending", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.staticTexts["Account recovery is pending"].waitForExistence(timeout: 5))
        let item = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 5)); item.tap()
        XCTAssertTrue(app.buttons["vault.reveal.password"].waitForExistence(timeout: 5))
        let navigation = [app.navigationBars.buttons["Vault"].firstMatch,
                          app.buttons["Show Sidebar"].firstMatch, app.buttons["Hide Sidebar"].firstMatch,
                          app.buttons["BackButton"].firstMatch]
        XCTAssertTrue(navigation.contains { $0.exists && $0.isHittable })
        XCTAssertTrue(tab(app, symbol: "person.crop.circle", title: "Account").isHittable)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Recovery warning preserves navigation"; shot.lifetime = .keepAlways; add(shot)
    }
}
