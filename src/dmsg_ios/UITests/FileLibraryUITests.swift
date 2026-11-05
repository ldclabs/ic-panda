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

    /// Whether the screen shows one flat color where the element sits.
    private func isBlank(_ element: XCUIElement) -> Bool {
        guard let image = element.screenshot().image.cgImage else { return false }
        var pixels = [UInt8](repeating: 0, count: image.width * image.height * 4)
        pixels.withUnsafeMutableBytes { buffer in
            CGContext(data: buffer.baseAddress, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: image.width * 4,
                      space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)?
                .draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        }
        return stride(from: 0, to: pixels.count, by: 4).allSatisfy { pixel in
            (0..<3).allSatisfy { abs(Int(pixels[pixel + $0]) - Int(pixels[$0])) < 16 }
        }
    }

    func testWorkspaceRootShowsNoTitleButPushedScreensDo() {
        let app = launch()
        // The expanded layout's floating tab bar counts as part of the navigation bars.
        // iOS 18 keeps the hidden title in the accessibility tree for VoiceOver, and
        // reports it as hittable too, so only the screen shows whether it is drawn.
        let title = app.navigationBars.staticTexts["Files"]
        if !app.buttons["ToggleSidebar"].exists && title.exists { XCTAssertTrue(isBlank(title)) }
        tap(app.buttons["files.transfers"], in: app)
        XCTAssertTrue(app.navigationBars.staticTexts["File transfers"].waitForExistence(timeout: 5))
        capture(app, "Pushed screen keeps a compact title")
    }

    func testLargeTextScopeMenuSwitchesToCloud() {
        let app = launch(["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"])
        chooseCloud(app)
        XCTAssertTrue(app.buttons["backup.item.fixture-local-backup"].waitForExistence(timeout: 5))
        capture(app, "Scope menu at maximum Dynamic Type")
    }

    func testImportPhotosOpensThePickerFromTheAddMenu() {
        let app = launch()
        app.buttons["files.add"].tap(); app.buttons["Import photos or videos"].tap()
        let cancel = app.buttons["Cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 8))
        capture(app, "Photo picker opened from the add menu")
        cancel.tap()
        XCTAssertTrue(cancel.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 5))
    }

    func testCameraEntryOnlyAppearsWithACamera() {
        let app = launch()
        app.buttons["files.add"].tap()
        XCTAssertTrue(app.buttons["New folder"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Take photo or video"].exists)
    }

    func testCameraCaptureIsListedInTheOpenFolder() {
        let app = launch(["--ui-camera"])
        tap(app.buttons["folder.item.fixture-folder"], in: app)
        XCTAssertTrue(app.buttons["file.item.fixture-nested"].waitForExistence(timeout: 5))
        app.buttons["files.add"].tap(); app.buttons["Take photo or video"].tap()
        let captured = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "IMG_")).firstMatch
        XCTAssertTrue(captured.waitForExistence(timeout: 5))
        XCTAssertTrue(captured.label.contains(".jpg"))
        XCTAssertTrue(app.descendants(matching: .any)["workspace.notice"].waitForExistence(timeout: 5))
        capture(app, "Camera capture listed in the open folder")
        app.buttons["files.parent"].tap()
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 5))
        XCTAssertFalse(captured.exists)
    }

    func testCameraDeniedOffersSettings() {
        let app = launch(["--ui-camera-denied"])
        app.buttons["files.add"].tap(); app.buttons["Take photo or video"].tap()
        let alert = app.alerts.firstMatch
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        XCTAssertTrue(alert.buttons["Open Settings"].exists)
        capture(app, "Camera access denied")
        alert.buttons["Cancel"].tap()
        XCTAssertTrue(alert.waitForNonExistence(timeout: 5))
    }

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
        XCTAssertTrue(app.staticTexts["Moved"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Connection unavailable. Your saved content stays on this device."].exists)
        tap(app.buttons["Choose folder"], in: app)
        tap(app.buttons["files.folder.choose"], in: app)
        let moved = app.staticTexts.matching(NSPredicate(format: "label == %@", "Moved"))
        expectation(for: NSPredicate(format: "count == 2"), evaluatedWith: moved)
        waitForExpectations(timeout: 5)
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
        XCTAssertTrue(app.navigationBars.buttons["Cancel"].exists)
        app.buttons["Choose folder"].tap()
        tap(app.buttons["files.folder.fixture-folder"], in: app)
        XCTAssertEqual(app.buttons["files.folder.choose"].label, "Move here")
        tap(app.buttons["files.folder.choose"], in: app)
        XCTAssertTrue(app.staticTexts["Moved"].waitForExistence(timeout: 5))
        capture(app, "File move completed in selected folder")
        app.buttons["Done"].tap(); back(app); back(app)
        tap(app.buttons["folder.item.fixture-folder"], in: app)
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 5))
    }

    func testContextMenuMoveAfterCancellingAndReopeningFolderPicker() {
        let app = launch(["--ui-files-slow-move"])
        app.buttons["file.item.fixture-local"].press(forDuration: 1)
        tap(app.buttons["Move to another folder"], in: app)
        tap(app.buttons["Choose folder"], in: app)
        app.navigationBars["Files"].buttons["Cancel"].tap()
        XCTAssertFalse(app.staticTexts["Moved"].exists)
        tap(app.buttons["Choose folder"], in: app)
        tap(app.buttons["files.folder.fixture-folder"], in: app)
        tap(app.buttons["files.folder.choose"], in: app)
        XCTAssertTrue(app.staticTexts["Moving…"].waitForExistence(timeout: 1))
        XCTAssertFalse(app.buttons["Choose folder"].isEnabled)
        XCTAssertTrue(app.staticTexts["Moved"].waitForExistence(timeout: 8))
        capture(app, "Context menu move after reopening destination picker")
        app.navigationBars["Move to another folder"].buttons["Done"].tap()
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForNonExistence(timeout: 5))
        tap(app.buttons["folder.item.fixture-folder"], in: app)
        XCTAssertTrue(app.buttons["file.item.fixture-local"].waitForExistence(timeout: 5))
    }

    func testFailedMoveRetriesTheChosenDestination() {
        let app = launch(["--ui-files-move-failure"])
        app.buttons["file.item.fixture-video"].press(forDuration: 1)
        tap(app.buttons["Move to another folder"], in: app)
        tap(app.buttons["Choose folder"], in: app)
        tap(app.buttons["files.folder.fixture-folder"], in: app)
        tap(app.buttons["files.folder.choose"], in: app)
        XCTAssertTrue(app.staticTexts["Connection unavailable. Your saved content stays on this device."].waitForExistence(timeout: 5))
        app.buttons["files.move.retry"].tap()
        XCTAssertTrue(app.staticTexts["Moved"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["files.move.retry"].exists)
        app.navigationBars["Move to another folder"].buttons["Done"].tap()
        XCTAssertTrue(app.buttons["file.item.fixture-video"].waitForNonExistence(timeout: 5))
        tap(app.buttons["folder.item.fixture-folder"], in: app)
        XCTAssertTrue(app.buttons["file.item.fixture-video"].waitForExistence(timeout: 5))
    }

    func testLinkFromALocalFileWaitsThenHandsOverTheLink() {
        let app = launch(["--ui-files-link-slow"])
        app.buttons["file.item.fixture-local"].press(forDuration: 1)
        tap(app.buttons["Share by link"], in: app)
        let create = app.buttons["file.link.create"]
        XCTAssertTrue(create.waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["file.link.remote"].exists)
        capture(app, "Link terms for a local file")
        create.tap()
        XCTAssertTrue(app.descendants(matching: .any)["file.link.working"].waitForExistence(timeout: 2))
        capture(app, "Link waits for its new encrypted copy")
        XCTAssertTrue(app.staticTexts["file.link.ready"].waitForExistence(timeout: 6))
        XCTAssertTrue(app.descendants(matching: .any)["file.link.share"].exists)
        tap(app.buttons["file.link.copy"], in: app)
        capture(app, "Finished link")
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["file.link.copy"].waitForNonExistence(timeout: 5))
    }

    func testLinkFromACloudBackupExplainsTheDownloadAndCanRetry() {
        let app = launch(["--ui-files-link-failure"])
        chooseCloud(app)
        tap(app.buttons["backup.item.fixture-old-backup"], in: app)
        tap(app.buttons["backup.link.fixture-old-backup"], in: app)
        XCTAssertTrue(app.staticTexts["file.link.remote"].waitForExistence(timeout: 5))
        capture(app, "Link terms for a backup that is not on this device")
        tap(app.buttons["file.link.create"], in: app)
        XCTAssertTrue(app.staticTexts["Connection unavailable. Your saved content stays on this device."].waitForExistence(timeout: 5))
        capture(app, "Link failure keeps its terms")
        tap(app.buttons["file.link.create"], in: app)
        XCTAssertTrue(app.staticTexts["file.link.ready"].waitForExistence(timeout: 5))
    }

    func testOversizedFilesCannotStartALink() {
        let app = launch()
        tap(app.buttons["file.item.fixture-video"], in: app)
        let link = app.buttons["file.link"]
        for _ in 0..<6 where !link.exists { app.swipeUp() }
        XCTAssertTrue(link.waitForExistence(timeout: 5))
        XCTAssertFalse(link.isEnabled)
        XCTAssertTrue(app.staticTexts["Files over 100 MiB cannot be shared by link."].exists)
        capture(app, "Oversized file explains the link limit")
    }

    func testLinksListShowsStateAndRevokes() {
        let app = launch(["--ui-files-links", "--ui-files-links-failure"])
        tap(app.buttons["files.tools"], in: app)
        tap(app.buttons["Share links"], in: app)
        let active = app.buttons["file.link.item.fixture-link-active"]
        XCTAssertTrue(active.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Share links could not be refreshed. Showing saved links."].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["file.link.item.fixture-link-guarded"].exists)
        capture(app, "Links keep the saved list when a refresh fails")
        // A link made on another device can be revoked but not shared from here.
        app.buttons["file.link.item.fixture-link-other-device"].tap()
        XCTAssertTrue(app.staticTexts["The full link is not available on this device."].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["file.link.copy"].exists)
        back(app)
        tap(active, in: app)
        XCTAssertTrue(app.buttons["file.link.copy"].waitForExistence(timeout: 5))
        tap(app.buttons["file.link.revoke"], in: app)
        capture(app, "Revoke asks first")
        let confirm = app.sheets.buttons["Revoke"].exists ? app.sheets.buttons["Revoke"] : app.buttons["Revoke"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: 5)); confirm.tap()
        let revoked = app.staticTexts.matching(NSPredicate(format: "identifier == 'file.link.state' AND label CONTAINS 'Revoked'")).firstMatch
        XCTAssertTrue(revoked.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["file.link.copy"].exists)
        XCTAssertFalse(app.buttons["file.link.revoke"].exists)
        capture(app, "Revoked link")
    }

    func testLinkScreensInLanguagesAndLargeText() {
        for (language, locale, title) in [("zh-Hans", "zh_CN", "通过链接分享"), ("ar", "ar_SA", nil as String?), ("de", "de_DE", "Per Link teilen")] {
            let app = launch(language == "de" ? [] : ["--ui-dark"], language: language, locale: locale)
            app.buttons["file.item.fixture-local"].press(forDuration: 1)
            let menu = title.map { app.buttons[$0] } ?? app.buttons.matching(NSPredicate(format: "label CONTAINS 'رابط'")).firstMatch
            tap(menu, in: app)
            XCTAssertTrue(app.buttons["file.link.create"].waitForExistence(timeout: 5))
            capture(app, "Link terms \(language)")
            app.terminate()
        }
        let app = launch(["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"])
        chooseCloud(app)
        tap(app.buttons["backup.item.fixture-old-backup"], in: app)
        tap(app.buttons["backup.link.fixture-old-backup"], in: app)
        XCTAssertTrue(app.staticTexts["file.link.remote"].waitForExistence(timeout: 5))
        capture(app, "Link terms at maximum Dynamic Type")
        let create = app.buttons["file.link.create"]
        for _ in 0..<8 where !create.isHittable { app.swipeUp() }
        XCTAssertTrue(create.isHittable)
        capture(app, "Create link reachable at maximum Dynamic Type")
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
