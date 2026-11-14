import XCTest

@MainActor
final class FeedbackUITests: XCTestCase {
    private func open(language: String = "en", dark: Bool = false, failure: Bool = false) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(\(language))"] + (dark ? ["--ui-dark"] : []) + (failure ? ["--ui-feedback-failure"] : [])
        app.launch()
        let account = ["zh-Hans": "我的", "ar": "الحساب", "de": "Konto"][language] ?? "Account"
        XCTAssertTrue(app.buttons[account].waitForExistence(timeout: 10)); app.buttons[account].tap()
        tap(app.buttons["account.feedback"], in: app)
        XCTAssertTrue(field(app).waitForExistence(timeout: 5))
        return app
    }

    private func field(_ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: "feedback.text").firstMatch
    }

    private func tap(_ element: XCUIElement, in app: XCUIApplication) {
        _ = element.waitForExistence(timeout: 3)
        for _ in 0..<12 where !element.isHittable {
            let top = element.exists && element.frame.midY < 110
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: top ? 0.35 : 0.7))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: top ? 0.7 : 0.35))
            start.press(forDuration: 0.05, thenDragTo: end)
        }
        XCTAssertTrue(element.isHittable); element.tap()
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }

    func testDraftNavigationScreenshotAndOriginalReportRetry() {
        let app = open(failure: true)
        let text = field(app)
        text.tap(); text.typeText("Keep this report after leaving the page")
        app.buttons["feedback.keyboard-done"].tap()
        app.navigationBars.buttons.firstMatch.tap()
        tap(app.buttons["account.feedback"], in: app)
        XCTAssertEqual(field(app).value as? String, "Keep this report after leaving the page")
        tap(app.buttons["feedback.fixture-image"], in: app)
        XCTAssertTrue(app.descendants(matching: .any)["feedback.screenshot"].firstMatch.waitForExistence(timeout: 5))
        tap(app.buttons["feedback.send"], in: app)
        XCTAssertTrue(app.descendants(matching: .any)["feedback.failure"].firstMatch.waitForExistence(timeout: 5))
        capture(app, "Feedback failure retains report and screenshot")
        for _ in 0..<6 where !field(app).exists || !field(app).isHittable { app.swipeDown() }
        XCTAssertFalse(field(app).isEnabled)
        tap(app.buttons["feedback.send"], in: app)
        XCTAssertTrue(app.buttons["feedback.new"].waitForExistence(timeout: 5))
        capture(app, "Feedback confirmed delivery")
        tap(app.buttons["feedback.new"], in: app)
        for _ in 0..<6 where !field(app).isHittable { app.swipeDown() }
        XCTAssertEqual(field(app).value as? String, "")
    }

    func testScreenshotFeedbackInChineseGermanArabicAndBothThemes() {
        for language in ["zh-Hans", "de", "ar"] {
            for dark in [false, true] {
                let app = open(language: language, dark: dark)
                capture(app, "Feedback empty · \(language) · \(dark ? "dark" : "light")")
                let text = field(app)
                text.tap(); text.typeText("The action failed. I expected the file to open. This description remains available for retry.")
                app.buttons["feedback.keyboard-done"].tap()
                tap(app.buttons["feedback.fixture-image"], in: app)
                capture(app, "Feedback screenshot · \(language) · \(dark ? "dark" : "light")")
                tap(app.buttons["feedback.send"], in: app)
                XCTAssertTrue(app.buttons["feedback.new"].waitForExistence(timeout: 5))
                capture(app, "Feedback delivered · \(language) · \(dark ? "dark" : "light")")
                app.terminate()
            }
        }
    }
}
