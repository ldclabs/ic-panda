import XCTest

@MainActor
final class AppStoreScreenshotTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    func testEnglishScreenshots() { capture(language: "en", locale: "en_US", account: "Account") }

    func testChineseScreenshots() { capture(language: "zh-Hans", locale: "zh_CN", account: "我的") }

    private func capture(language: String, locale: String, account: String) {
        let app = XCUIApplication()
        func launch(_ arguments: [String]) {
            app.launchArguments = ["--ui-testing", "--ui-store-screenshots", "-AppleLanguages", "(\(language))", "-AppleLocale", locale] + arguments
            app.launch()
        }
        func shot(_ name: String) {
            // Wait for tab and navigation transitions before capturing the final layout.
            Thread.sleep(forTimeInterval: 1)
            let attachment = XCTAttachment(screenshot: app.screenshot())
            attachment.name = "store-\(language)-\(name)"
            attachment.lifetime = .keepAlways
            add(attachment)
        }
        launch([])
        if !app.buttons["vault.scope.all"].waitForExistence(timeout: 2), app.buttons["ToggleSidebar"].exists {
            app.buttons["ToggleSidebar"].tap()
        }
        let entry = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(entry.waitForExistence(timeout: 10))
        shot("01-vault")
        entry.tap()
        XCTAssertTrue(app.staticTexts["GitHub"].firstMatch.waitForExistence(timeout: 5))
        shot("02-secret")
        app.terminate()
        launch(["--ui-inbox"])
        XCTAssertTrue(app.buttons["channel.row.fixture-channel"].waitForExistence(timeout: 10))
        shot("03-messages")
        app.terminate()
        launch([])
        app.buttons[language == "en" ? "Files" : "文件"].firstMatch.tap()
        XCTAssertTrue(app.buttons["files.scope.local"].waitForExistence(timeout: 10))
        shot("04-files")
        app.buttons[account].firstMatch.tap()
        let security = app.descendants(matching: .any).matching(identifier: "account.security").firstMatch
        if !security.isHittable, app.buttons["ToggleSidebar"].exists { app.buttons["ToggleSidebar"].tap() }
        XCTAssertTrue(security.waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["account.lock"].waitForExistence(timeout: 5))
        shot("05-account")
        app.terminate()
    }
}
