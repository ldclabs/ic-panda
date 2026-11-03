import XCTest

@MainActor
final class FileLibraryUITests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    private func launch(_ extra: [String] = [], language: String = "en", locale: String = "en_US") -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-files-rich", "-AppleLanguages", "(\(language))", "-AppleLocale", locale] + extra
        app.launch()
        XCTAssertTrue(app.buttons["files.scope.local"].waitForExistence(timeout: 8))
        return app
    }

    private func tap(_ element: XCUIElement, in app: XCUIApplication) {
        for _ in 0..<6 where !element.isHittable { app.swipeUp() }
        XCTAssertTrue(element.waitForExistence(timeout: 5)); XCTAssertTrue(element.isHittable); element.tap()
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let feedback = app.buttons.matching(identifier: "workspace.feedback").firstMatch
        if feedback.exists { _ = feedback.waitForNonExistence(timeout: 6) }
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }

    private func chooseCloud(_ app: XCUIApplication, language: String = "en") {
        let cloud = app.buttons["files.scope.cloud"]
        if cloud.exists && cloud.isHittable { cloud.tap() }
        else {
            app.buttons["files.scope.local"].tap()
            let title = ["en": "Cloud backups", "zh-Hans": "云端备份", "fr": "Sauvegardes en ligne", "ar": "النسخ السحابية"][language]!
            app.buttons[title].tap()
        }
    }

    private func back(_ app: XCUIApplication) { app.navigationBars.buttons.element(boundBy: 0).tap() }

    func testCloudRefreshFailureStaysWithItsRetryControl() {
        let app = launch(["--ui-files-cloud-failure"])
        chooseCloud(app)
        XCTAssertTrue(app.staticTexts["Backup list could not be refreshed"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["backups.refresh"].isHittable)
        XCTAssertFalse(app.buttons["Sync now"].exists)
        capture(app, "Cloud catalog failure and refresh")
        app.buttons["backups.refresh"].tap()
        XCTAssertTrue(app.staticTexts["Backup list could not be refreshed"].waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.buttons["backup.item.fixture-backup"].exists)
    }

    func testSearchChangesScopeAndClearsOldResults() {
        let app = launch(["--ui-files-slow"])
        chooseCloud(app)
        app.swipeDown()
        let search = app.searchFields.firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: 5)); search.tap(); search.typeText("no-matching-file")
        XCTAssertTrue(app.buttons["backup.item.fixture-backup"].waitForNonExistence(timeout: 5))
        app.buttons["Clear search"].tap()
        XCTAssertTrue(app.buttons["backup.item.fixture-backup"].waitForExistence(timeout: 5))
        if app.buttons["Cancel"].isHittable { app.buttons["Cancel"].tap() }
        app.buttons["files.scope.local"].tap()
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["backup.item.fixture-backup"].exists)
    }

    func testCloudHistoricalVersionCanBeRestoredWithoutOpeningTheCurrentVersion() {
        let app = launch()
        tap(app.buttons["file.item.fixture-local"], in: app)
        tap(app.buttons["file.versions"], in: app)
        tap(app.buttons["file.version.v1"], in: app)
        XCTAssertFalse(app.buttons["Preview file"].isEnabled)
        tap(app.buttons["file.restore"], in: app)
        let preview = app.buttons["Preview file"]
        expectation(for: NSPredicate(format: "enabled == true"), evaluatedWith: preview)
        waitForExpectations(timeout: 5)
        capture(app, "Restored historical file version")
    }

    func testRenameFailureKeepsDraftAndCanRetry() {
        let app = launch(["--ui-files-save-failure"])
        tap(app.buttons["file.item.fixture-local"], in: app)
        tap(app.buttons["file.manage"], in: app)
        tap(app.buttons["Rename"], in: app)
        let field = app.textFields["files.name"]
        XCTAssertTrue(field.waitForExistence(timeout: 5)); field.tap()
        let old = field.value as? String ?? ""
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: old.count) + "Renamed image.png")
        field.typeText("\n")
        XCTAssertTrue(app.staticTexts["files.action.failure"].waitForExistence(timeout: 5))
        XCTAssertEqual(field.value as? String, "Renamed image.png")
        capture(app, "Rename failure retains draft")
        app.buttons["files.name.save"].tap()
        XCTAssertTrue(field.waitForNonExistence(timeout: 5))
        back(app)
        XCTAssertTrue(app.staticTexts["Renamed image.png"].waitForExistence(timeout: 5))
    }

    func testBatchBackupShowsExistingAndOversizedOutcomes() {
        let app = launch()
        app.buttons["files.tools"].tap(); app.buttons["Select files"].tap()
        tap(app.buttons["file.item.fixture-local"], in: app)
        tap(app.buttons["file.item.fixture-video"], in: app)
        app.buttons["files.selection.actions"].tap(); app.buttons["Back up to cloud"].tap()
        tap(app.buttons["files.backup.submit"], in: app)
        XCTAssertTrue(app.staticTexts["This version is already backed up."].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["This file exceeds your current cloud backup file limit."].exists)
        XCTAssertFalse(app.staticTexts["Transfer queued. Follow its progress in Files."].exists)
        capture(app, "Per-file backup outcomes")
    }

    func testRemoveLocalCopyKeepsRestorableRecord() {
        let app = launch()
        tap(app.buttons["file.item.fixture-local"], in: app)
        tap(app.buttons["file.evict"], in: app)
        app.sheets.buttons["Remove local copy"].tap()
        XCTAssertTrue(app.buttons["file.restore"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Preview file"].isEnabled)
        XCTAssertTrue(app.staticTexts["Local image.png"].exists)
        capture(app, "Local copy removed with restore available")
    }

    func testSidebarSelectionReplacesTheFileDetail() throws {
        let app = launch()
        tap(app.buttons["file.item.fixture-local"], in: app)
        XCTAssertTrue(app.buttons["file.evict"].waitForExistence(timeout: 5))
        let next = app.buttons["file.item.fixture-video"]
        guard next.isHittable else { throw XCTSkip("Requires the expanded file sidebar") }
        next.tap()
        let title = app.descendants(matching: .any)["file.detail.title"].firstMatch
        expectation(for: NSPredicate(format: "label CONTAINS %@", "Product walkthrough.mp4"), evaluatedWith: title)
        waitForExpectations(timeout: 5)
        XCTAssertFalse(app.buttons["file.evict"].exists)
        capture(app, "Sidebar selection shows the selected file")
    }

    func testChangingDestinationAfterPartialMoveMovesTheWholeSelection() {
        let app = launch(["--ui-files-move-failure"])
        app.buttons["files.tools"].tap(); app.buttons["Select files"].tap()
        tap(app.buttons["file.item.fixture-local"], in: app)
        tap(app.buttons["file.item.fixture-video"], in: app)
        app.buttons["files.selection.actions"].tap(); app.buttons["Move to another folder"].tap()
        tap(app.buttons["Choose folder"], in: app)
        tap(app.buttons["files.folder.fixture-folder"], in: app)
        tap(app.buttons["files.folder.choose"], in: app)
        tap(app.buttons["files.move.save"], in: app)
        XCTAssertTrue(app.staticTexts["Moved"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Connection unavailable. Your saved content stays on this device."].exists)
        tap(app.buttons["Choose folder"], in: app)
        tap(app.buttons["files.folder.choose"], in: app)
        tap(app.buttons["files.move.save"], in: app)
        XCTAssertTrue(app.buttons["files.move.save"].waitForNonExistence(timeout: 5))
        app.navigationBars["Move to another folder"].buttons["Done"].firstMatch.tap()
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["file.item.fixture-video"].exists)
        capture(app, "Both files follow the changed move destination")
    }

    func testFolderCreationNavigationAndTrashRestore() {
        let app = launch()
        app.buttons["files.add"].tap(); app.buttons["New folder"].tap()
        let field = app.textFields["files.name"]
        XCTAssertTrue(field.waitForExistence(timeout: 5)); field.tap(); field.typeText("Audit folder")
        app.buttons["files.name.save"].tap()
        XCTAssertTrue(field.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Audit folder")).firstMatch.waitForExistence(timeout: 5))
        tap(app.buttons["folder.item.fixture-folder"], in: app)
        XCTAssertTrue(app.buttons["file.item.fixture-nested"].waitForExistence(timeout: 5))
        app.buttons["files.parent"].tap()
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 5))
        app.buttons["files.tools"].tap(); app.buttons["Select files"].tap()
        tap(app.buttons["file.item.fixture-local"], in: app)
        app.buttons["files.selection.actions"].tap(); app.buttons["Move to Trash"].tap()
        app.sheets.buttons["Move to Trash"].tap()
        XCTAssertTrue(app.staticTexts["Moved to Trash"].waitForExistence(timeout: 5))
        app.buttons["Done"].tap()
        app.buttons["files.tools"].tap(); app.buttons["Trash"].tap()
        tap(app.buttons["trash.restore.fixture-local"], in: app)
        XCTAssertTrue(app.buttons["trash.restore.fixture-local"].waitForNonExistence(timeout: 5))
        capture(app, "Trash after restoring selected file")
    }

    func testLayoutsInFourLanguages() {
        for (language, locale, dark) in [("en","en_US",false),("zh-Hans","zh_CN",true),("fr","fr_FR",false),("ar","ar_SA",true)] {
            let app = launch(dark ? ["--ui-dark"] : [], language: language, locale: locale)
            XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 5))
            capture(app, "Files local \(language)")
            chooseCloud(app, language: language)
            XCTAssertTrue(app.buttons["backup.item.fixture-backup"].waitForExistence(timeout: 5))
            capture(app, "Files cloud \(language)")
            app.terminate()
        }
    }

    func testMoveRequiresDestinationAndKeepsTheFileReachable() {
        let app = launch()
        tap(app.buttons["file.item.fixture-local"], in: app)
        tap(app.buttons["file.manage"], in: app)
        tap(app.buttons["Move to another folder"], in: app)
        XCTAssertFalse(app.buttons["files.move.save"].isEnabled)
        app.buttons["Choose folder"].tap()
        tap(app.buttons["files.folder.fixture-folder"], in: app)
        tap(app.buttons["files.folder.choose"], in: app)
        tap(app.buttons["files.move.save"], in: app)
        XCTAssertTrue(app.staticTexts["Moved"].waitForExistence(timeout: 5))
        capture(app, "File move completed in selected folder")
        app.buttons["Done"].tap(); back(app); back(app)
        tap(app.buttons["folder.item.fixture-folder"], in: app)
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 5))
    }

    func testAccessibilityAndLargeText() throws {
        let app = launch(["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"])
        capture(app, "Files maximum Dynamic Type header")
        let longName = app.buttons["file.item.fixture-note"]
        let list = app.collectionViews.firstMatch
        for _ in 0..<10 where !longName.isHittable {
            // The list extends behind the tab bar; start inside its visible body.
            list.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.55)).press(forDuration: 0.05,
                thenDragTo: list.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)))
        }
        XCTAssertTrue(longName.waitForExistence(timeout: 5)); XCTAssertTrue(longName.isHittable)
        XCTAssertTrue(longName.label.contains("A very long research document with a visible extension.md"))
        capture(app, "Files maximum Dynamic Type")
        try app.performAccessibilityAudit(for: [.sufficientElementDescription, .hitRegion])
    }
}
