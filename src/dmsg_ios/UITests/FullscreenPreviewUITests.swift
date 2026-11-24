import XCTest

@MainActor
final class FullscreenPreviewUITests: XCTestCase {
    private struct Look {
        let language: String
        let locale: String
        let dark: Bool
        let large: Bool

        var name: String { "\(language)-\(dark ? "dark" : "light")\(large ? "-ax" : "")" }
        var arguments: [String] {
            ["--ui-testing", "--ui-files-rich", "--ui-files-preview",
             "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
                + (dark ? ["--ui-dark"] : [])
                + (large ? ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityL"] : [])
        }
    }

    override func setUp() { continueAfterFailure = false }

    func testFullscreenPreviewFilterMarkdownAndFallback() {
        let app = launch(Look(language: "en", locale: "en_US", dark: false, large: false))
        tapRow(app.buttons["file.item.fixture-local"], in: app)
        XCTAssertTrue(app.buttons["preview.close"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["preview.filter"].exists)
        XCTAssertTrue(app.buttons["file.details"].exists)
        XCTAssertTrue(app.buttons["preview.actions"].exists)

        // Filter to Documents -> jumps to Markdown document and enables Reading/Source toggle.
        app.buttons["preview.filter"].tap()
        let documents = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Documents")).firstMatch
        XCTAssertTrue(documents.waitForExistence(timeout: 5))
        documents.tap()
        XCTAssertTrue(app.buttons["preview.markdown.mode"].waitForExistence(timeout: 5))
        app.buttons["preview.markdown.mode"].tap()
        app.buttons["preview.markdown.mode"].tap()

        // Close preview and open unsupported video -> renders fallback metadata card.
        app.buttons["preview.close"].tap()
        XCTAssertTrue(app.buttons["file.item.fixture-video"].waitForExistence(timeout: 5))
        tapRow(app.buttons["file.item.fixture-video"], in: app)
        XCTAssertTrue(app.descendants(matching: .any)["preview.fallback"].waitForExistence(timeout: 5))
        app.buttons["file.details"].tap()
        XCTAssertTrue(app.buttons["Preview file"].waitForExistence(timeout: 5), "The original system preview remains available for video.")
    }

    func testFullscreenPreviewShowsItsOwnNotices() {
        let app = launch(Look(language: "en", locale: "en_US", dark: false, large: false))
        tapRow(app.buttons["file.item.fixture-local"], in: app)
        XCTAssertTrue(app.buttons["preview.actions"].waitForExistence(timeout: 5))
        app.buttons["preview.actions"].tap()
        let copy = app.buttons["Copy"].firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: 5))
        copy.tap()
        // The cover is the notice host while it is shown, so the notice is above it.
        let notice = app.descendants(matching: .any)["workspace.notice"].firstMatch
        XCTAssertTrue(notice.waitForExistence(timeout: 5))
        XCTAssertTrue(notice.isHittable)
        XCTAssertTrue(app.buttons["preview.close"].exists)
        shot(app, "ios-preview-notice-en-light")
    }

    func testFullscreenPreviewVisualMatrix() {
        let looks = [
            Look(language: "en", locale: "en_US", dark: false, large: false),
            Look(language: "en", locale: "en_US", dark: true, large: false),
            Look(language: "zh-Hans", locale: "zh_CN", dark: false, large: false),
            Look(language: "zh-Hans", locale: "zh_CN", dark: true, large: false),
            Look(language: "ar", locale: "ar_SA", dark: false, large: false),
            Look(language: "en", locale: "en_US", dark: false, large: true),
        ]
        for look in looks {
            let app = launch(look)
            tapRow(app.buttons["file.item.fixture-local"], in: app)
            XCTAssertTrue(app.buttons["preview.close"].waitForExistence(timeout: 5), look.name)
            shot(app, "ios-preview-image-\(look.name)")
            app.buttons["preview.close"].tap()

            tapRow(app.buttons["file.item.fixture-note"], in: app)
            XCTAssertTrue(app.buttons["preview.markdown.mode"].waitForExistence(timeout: 5), look.name)
            shot(app, "ios-preview-markdown-\(look.name)")
            app.buttons["preview.close"].tap()

            if !look.dark || look.language == "zh-Hans" {
                tapRow(app.buttons["file.item.fixture-video"], in: app)
                XCTAssertTrue(app.descendants(matching: .any)["preview.fallback"].waitForExistence(timeout: 5), look.name)
                shot(app, "ios-preview-fallback-\(look.name)")
                app.buttons["preview.close"].tap()
            }
            app.terminate()
        }
    }

    private func launch(_ look: Look) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = look.arguments
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["files.scope.local"].firstMatch.waitForExistence(timeout: 10), look.name)
        return app
    }

    private func tapRow(_ element: XCUIElement, in app: XCUIApplication) {
        for _ in 0..<8 where !element.isHittable { app.swipeUp() }
        XCTAssertTrue(element.waitForExistence(timeout: 5))
        XCTAssertTrue(element.isHittable)
        element.tap()
    }

    private func shot(_ app: XCUIApplication, _ name: String) {
        Thread.sleep(forTimeInterval: 0.6)
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
