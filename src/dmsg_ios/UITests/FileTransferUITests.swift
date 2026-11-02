import XCTest

@MainActor
final class FileTransferUITests: XCTestCase {
    private func inspect(_ language: String, _ locale: String, dark: Bool) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-file-transfers", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        let files = app.tabBars.buttons.element(boundBy: 2)
        XCTAssertTrue(files.waitForExistence(timeout: 8)); files.tap()
        let transfers = app.buttons["files.transfers"]
        XCTAssertTrue(transfers.waitForExistence(timeout: 5)); transfers.tap()
        for id in ["preview-backup", "preview-download"] {
            let stage = app.staticTexts["transfer.stage.\(id)"]
            for _ in 0..<7 where !stage.isHittable { app.swipeUp() }
            XCTAssertTrue(stage.isHittable)
            if language == "en" {
                XCTAssertEqual(stage.label, id == "preview-backup" ? "Confirming cloud backup…" : "Verifying…")
            }
            let shot = XCTAttachment(screenshot: app.screenshot())
            shot.name = "Transfer \(id) \(language) \(dark ? "dark" : "light")"
            shot.lifetime = .keepAlways; add(shot)
        }
        // Navigation must not remove account-owned transfer progress.
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(transfers.waitForExistence(timeout: 5)); transfers.tap()
        let stage = app.staticTexts["transfer.stage.preview-backup"]
        for _ in 0..<7 where !stage.isHittable { app.swipeUp() }
        XCTAssertTrue(stage.isHittable)
        app.terminate()
    }

    func testEnglishLight() { inspect("en", "en_US", dark: false) }
    func testChineseDark() { inspect("zh-Hans", "zh_CN", dark: true) }
    func testFrenchLongCopy() { inspect("fr", "fr_FR", dark: false) }
    func testArabicRTL() { inspect("ar", "ar_SA", dark: true) }
}
