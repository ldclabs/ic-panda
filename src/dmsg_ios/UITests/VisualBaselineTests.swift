import XCTest

/// Before-and-after screenshots for visual changes (DESIGN.md, "Checking a change"):
/// the main screens in light and dark, English and Simplified Chinese, plus one
/// accessibility text size and Arabic. `scripts/ios/export-screenshots.sh` copies the
/// attachments out of the result bundle.
@MainActor
final class VisualBaselineTests: XCTestCase {
    private struct Look {
        let language: String, locale: String, dark: Bool, large: Bool
        var name: String { "\(language)-\(dark ? "dark" : "light")\(large ? "-ax" : "")" }
        var arguments: [String] {
            ["--ui-testing", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
                + (dark ? ["--ui-dark"] : [])
                + (large ? ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityL"] : [])
        }
    }

    private let looks = [
        Look(language: "en", locale: "en_US", dark: false, large: false),
        Look(language: "en", locale: "en_US", dark: true, large: false),
        Look(language: "zh-Hans", locale: "zh_CN", dark: false, large: false),
        Look(language: "zh-Hans", locale: "zh_CN", dark: true, large: false),
        Look(language: "en", locale: "en_US", dark: false, large: true),
        Look(language: "ar", locale: "ar_SA", dark: false, large: false),
    ]

    override func setUp() { continueAfterFailure = false }

    func testVaultAndAccount() {
        for look in looks {
            let app = launch(look)
            let item = app.buttons["vault.item.fixture-github"]
            XCTAssertTrue(item.waitForExistence(timeout: 10), look.name)
            shot(app, look, "vault")
            item.tap()
            XCTAssertTrue(app.staticTexts["vault.detail.title"].waitForExistence(timeout: 5), look.name)
            shot(app, look, "vault-detail")
            app.tabBars.buttons.element(boundBy: 3).tap()
            XCTAssertTrue(app.buttons["account.profile"].waitForExistence(timeout: 5), look.name)
            shot(app, look, "account")
            app.terminate()
        }
    }

    func testMessages() {
        for look in looks {
            let app = launch(look, ["--ui-channel", "--ui-channel-thread"])
            let channel = app.buttons["channel.row.fixture-channel"]
            XCTAssertTrue(channel.waitForExistence(timeout: 10), look.name)
            shot(app, look, "messages")
            channel.tap()
            XCTAssertTrue(app.buttons["conversation.title"].waitForExistence(timeout: 5), look.name)
            shot(app, look, "conversation")
            app.terminate()
        }
    }

    func testReturningAccount() {
        for look in looks {
            let app = launch(look, ["--ui-reminder-services", "--ui-locked"])
            XCTAssertTrue(app.buttons["login.unlock"].waitForExistence(timeout: 10), look.name)
            shot(app, look, "returning")
            app.terminate()
        }
    }

    private func launch(_ look: Look, _ extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = look.arguments + extra
        app.launch()
        return app
    }

    private func shot(_ app: XCUIApplication, _ look: Look, _ screen: String) {
        // Let navigation and tab transitions settle before capturing the final layout.
        Thread.sleep(forTimeInterval: 1)
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "\(screen)-\(look.name)"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
