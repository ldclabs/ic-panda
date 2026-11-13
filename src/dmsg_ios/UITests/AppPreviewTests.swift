import XCTest

/// Drives the App Store preview takes, one scene per test, on fictitious
/// household data. `scripts/ios/record-app-preview.sh` records the screen while
/// each test runs; `PREVIEW-MARK` lines let the edit find each action in the take.
/// The language comes from `TEST_RUNNER_PREVIEW_LANG` (`zh` or `en`); without it
/// the takes are skipped, so the regular UI test run stays fast.
@MainActor
final class AppPreviewTests: XCTestCase {
    private var english: Bool { ProcessInfo.processInfo.environment["PREVIEW_LANG"] == "en" }

    override func setUpWithError() throws {
        guard ProcessInfo.processInfo.environment["PREVIEW_LANG"] != nil else { throw XCTSkip("Recorded by scripts/ios/record-app-preview.sh") }
        continueAfterFailure = false
    }

    func testScene1Keep() {
        let app = launch(["--ui-access"])
        let unlock = app.buttons["login.unlock"]
        XCTAssertTrue(unlock.waitForExistence(timeout: 15))
        hold(2.5)
        mark("s1.unlock")
        unlock.tap()
        XCTAssertTrue(app.buttons["vault.item.preview-03-mail"].waitForExistence(timeout: 30))
        mark("s1.vault")
        hold(5)
    }

    func testScene2Devices() {
        let app = launch([])
        let mail = app.buttons["vault.item.preview-03-mail"]
        XCTAssertTrue(mail.waitForExistence(timeout: 15))
        hold(2.5)
        mark("s2.item")
        mail.tap()
        hold(3.5)
        app.buttons[text("Account", "我的")].firstMatch.tap()
        let devices = app.descendants(matching: .any).matching(identifier: "account.devices").firstMatch
        XCTAssertTrue(devices.waitForExistence(timeout: 10))
        hold(0.8)
        devices.tap()
        XCTAssertTrue(app.staticTexts[text("This iPhone", "这台 iPhone")].waitForExistence(timeout: 10))
        mark("s2.devices")
        hold(5)
    }

    func testScene3Send() {
        let app = launch(["--ui-preview-files"])
        let file = app.descendants(matching: .any).matching(identifier: "file.item.preview-passport").firstMatch
        XCTAssertTrue(file.waitForExistence(timeout: 15))
        hold(2.5)
        mark("s3.file")
        file.tap()
        let share = app.buttons[text("Share by link", "通过链接分享")].firstMatch
        XCTAssertTrue(share.waitForExistence(timeout: 10))
        if !share.isHittable { app.swipeUp() }
        hold(1.2)
        mark("s3.share")
        share.tap()
        let downloads = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", text("Downloads allowed", "允许的下载次数"))).firstMatch
        XCTAssertTrue(downloads.waitForExistence(timeout: 10))
        hold(1.8)
        downloads.tap()
        let five = app.buttons["5"].firstMatch
        XCTAssertTrue(five.waitForExistence(timeout: 5))
        hold(0.6)
        five.tap()
        hold(1.8)
        mark("s3.create")
        app.buttons[text("Create link", "生成链接")].firstMatch.tap()
        XCTAssertTrue(app.buttons[text("Copy link", "复制链接")].firstMatch.waitForExistence(timeout: 10))
        mark("s3.ready")
        hold(5)
    }

    func testScene4aRequest() {
        let app = launch(["--ui-preview-files"])
        openCollectFiles(app)
        let new = app.buttons["file.request.new"]
        XCTAssertTrue(new.waitForExistence(timeout: 10))
        hold(1)
        new.tap()
        let create = app.buttons["file.request.create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10))
        mark("s4.form")
        hold(3)
        mark("s4.create")
        create.tap()
        XCTAssertTrue(app.descendants(matching: .any).matching(identifier: "file.request.ready").firstMatch.waitForExistence(timeout: 10))
        mark("s4.ready")
        hold(4)
    }

    func testScene4bArrived() {
        let app = launch(["--ui-preview-files", "--ui-preview-arrived"])
        openCollectFiles(app)
        let request = app.descendants(matching: .any).matching(identifier: "file.request.item.preview-request").firstMatch
        XCTAssertTrue(request.waitForExistence(timeout: 10))
        mark("s4.list")
        hold(3)
        mark("s4.open")
        request.tap()
        let save = app.buttons["file.request.save"].firstMatch
        XCTAssertTrue(save.waitForExistence(timeout: 10))
        if !save.isHittable { app.swipeUp(velocity: .slow) }
        hold(2)
        mark("s4.save")
        save.tap()
        hold(4)
    }

    private func launch(_ arguments: [String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-store-preview", "-AppleLanguages", english ? "(en)" : "(zh-Hans)",
                               "-AppleLocale", english ? "en_US" : "zh_CN"] + arguments
        app.launch()
        return app
    }

    private func openCollectFiles(_ app: XCUIApplication) {
        let tools = app.buttons["files.tools"]
        XCTAssertTrue(tools.waitForExistence(timeout: 15))
        tools.tap()
        let collect = app.buttons[text("Collect files", "收取文件")].firstMatch
        XCTAssertTrue(collect.waitForExistence(timeout: 5))
        collect.tap()
    }

    private func text(_ english: String, _ chinese: String) -> String { self.english ? english : chinese }

    private func hold(_ seconds: TimeInterval) { Thread.sleep(forTimeInterval: seconds) }

    /// Wall-clock time; the recording script subtracts the moment its recording started.
    private func mark(_ name: String) { print("PREVIEW-MARK \(name) \(String(format: "%.3f", Date().timeIntervalSince1970))") }
}
