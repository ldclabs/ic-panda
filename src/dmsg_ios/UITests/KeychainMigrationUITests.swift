import XCTest

@MainActor
final class KeychainMigrationUITests: XCTestCase {
    func testPreparationFailureCanRetryWithoutOfferingAccountCreation() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-keychain-retry", "--ui-welcome", "-AppleLanguages", "(zh-Hans)"]
        app.launch()
        XCTAssertTrue(app.buttons["keychain.retry"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["login.initialize"].exists)
        XCTAssertFalse(app.buttons["login.start"].exists)
        capture(app, "Keychain preparation failure — Chinese light")
        app.buttons["keychain.retry"].tap()
        XCTAssertTrue(app.buttons["login.start"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["keychain.retry"].exists)
    }

    func testMissingPreparationRemainsBlockedAfterRetryInRTL() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-keychain-missing", "--ui-dark", "-AppleLanguages", "(ar)"]
        app.launch()
        XCTAssertTrue(app.buttons["keychain.retry"].waitForExistence(timeout: 10))
        capture(app, "Skipped preparation — Arabic dark")
        app.buttons["keychain.retry"].tap()
        XCTAssertTrue(app.buttons["keychain.retry"].exists)
        XCTAssertFalse(app.buttons["login.start"].exists)
        XCTAssertFalse(app.buttons["login.initialize"].exists)
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
}
