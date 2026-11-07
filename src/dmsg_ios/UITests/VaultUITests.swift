import XCTest

@MainActor
final class VaultUITests: XCTestCase {
    private func launch(_ extra: [String] = [], language: String = "en") -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-vault-rich", "-AppleLanguages", "(\(language))", "-AppleLocale", language] + extra
        app.launch()
        if !app.buttons["vault.scope.all"].waitForExistence(timeout: 2), app.buttons["ToggleSidebar"].exists {
            app.buttons["ToggleSidebar"].tap()
        }
        XCTAssertTrue(app.buttons["vault.scope.all"].waitForExistence(timeout: 10))
        return app
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let loading = app.activityIndicators["vault.loading"]
        if loading.exists { XCTAssertTrue(loading.waitForNonExistence(timeout: 8)) }
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }

    private func openFieldMenu(_ app: XCUIApplication, _ field: String) {
        let menu = app.descendants(matching: .any).matching(identifier: "vault.field.more.\(field)").firstMatch
        for _ in 0..<6 where !menu.isHittable { app.swipeUp() }
        XCTAssertTrue(menu.isHittable)
        // iOS 18 reports Menu as Button to legacy AX and PopUpButton to modern
        // AX. Tap its observed frame, then verify that the menu actually opened.
        menu.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertTrue(app.buttons["vault.enlarge.\(field)"].waitForExistence(timeout: 5))
    }

    func testSearchAndCategoryPositionsStayStableWhileLoading() {
        let app = launch(["--ui-vault-slow-load"])
        XCTAssertTrue(app.buttons["vault.item.fixture-github"].waitForExistence(timeout: 8))
        let search = app.searchFields.firstMatch
        XCTAssertTrue(search.isHittable)
        let all = app.buttons["vault.scope.all"], favorites = app.buttons["vault.scope.favorites"]
        let originalAll = all.frame, originalFavorites = favorites.frame, originalLabel = favorites.label
        favorites.tap()
        XCTAssertTrue(app.activityIndicators["vault.loading"].waitForExistence(timeout: 1))
        XCTAssertEqual(favorites.label, originalLabel)
        XCTAssertEqual(all.frame.minX, originalAll.minX, accuracy: 0.5)
        XCTAssertEqual(favorites.frame.width, originalFavorites.width, accuracy: 0.5)
        XCTAssertEqual(favorites.frame.minY, originalFavorites.minY, accuracy: 0.5)
        XCTAssertTrue(search.isHittable)
        XCTAssertTrue(app.buttons["vault.item.fixture-github"].waitForExistence(timeout: 8))
        XCTAssertEqual(favorites.frame.minX, originalFavorites.minX, accuracy: 0.5)
        all.tap()
        XCTAssertTrue(app.buttons["vault.item.fixture-github"].waitForExistence(timeout: 8))
        search.tap(); search.typeText("GitHub")
        XCTAssertTrue(app.buttons["vault.item.fixture-github"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.buttons["vault.item.fixture-note"].exists)
        capture(app, "Visible vault search and stable categories")
    }

    func testBrowserSearchInChineseAndArabic() {
        for language in ["zh-Hans", "ar"] {
            let app = launch(language == "ar" ? ["--ui-dark"] : [], language: language)
            XCTAssertTrue(app.buttons["vault.item.fixture-github"].waitForExistence(timeout: 5))
            XCTAssertTrue(app.searchFields.firstMatch.isHittable)
            capture(app, "Vault search \(language)")
            app.buttons["vault.scope.favorites"].tap()
            XCTAssertTrue(app.buttons["vault.item.fixture-github"].waitForExistence(timeout: 5))
            XCTAssertTrue(app.searchFields.firstMatch.isHittable)
            capture(app, "Vault favorites \(language)")
            app.terminate()
        }
    }

    func testFavoritesAndAllTypesAreReachable() {
        let app = launch()
        app.buttons["vault.scope.favorites"].tap()
        let later = app.buttons["vault.item.fixture-rich-5"]
        for _ in 0..<4 where !later.isHittable { app.swipeUp() }
        XCTAssertTrue(later.isHittable)
        app.buttons["vault.scope.note"].tap()
        XCTAssertTrue(app.buttons["vault.item.fixture-note"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["vault.item.fixture-github"].exists)
        capture(app, "Vault notes and complete favorites")
    }

    func testDiscardKeepsDraftUntilExplicitDecision() {
        let app = launch()
        app.buttons["vault.item.fixture-github"].tap()
        app.buttons["vault.edit.open"].tap()
        let title = app.textFields["vault.edit.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5)); title.tap(); title.typeText(" draft")
        app.navigationBars.buttons["Cancel"].tap()
        XCTAssertTrue(app.buttons["Discard"].waitForExistence(timeout: 5))
        capture(app, "Unsaved vault draft requires a decision")
        app.buttons["Discard"].tap()
        XCTAssertTrue(app.staticTexts["vault.detail.title"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["vault.detail.title"].label, "GitHub")
    }

    func testOTPStaysCurrentWithoutAskingAndOtherFieldsCopyDirectly() {
        let app = launch()
        let row = app.buttons["vault.item.fixture-rich-0"]
        for _ in 0..<3 where !row.isHittable { app.swipeUp() }
        row.tap()
        let code = app.buttons["vault.copy.otp"]
        for _ in 0..<3 where !code.isHittable { app.swipeUp() }
        let visible = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "1 2 3 4 5 6"), object: code)
        XCTAssertEqual(XCTWaiter.wait(for: [visible], timeout: 5), .completed)
        XCTAssertFalse(app.buttons["vault.reveal.otp"].exists)
        app.buttons["vault.copy.otp"].tap()
        let copy = app.buttons["vault.copy.username"]
        if !copy.isHittable { app.swipeDown() }
        copy.tap()
        // The fixture's code lasts two seconds, and a new one replaces it.
        sleep(3)
        if !code.isHittable { app.swipeUp() }
        XCTAssertEqual(code.value as? String, "1 2 3 4 5 6")
        capture(app, "OTP stays current and copies without asking")
    }

    func testCopyingASensitiveFieldTakesOneTapAndConfirmsNothing() {
        let app = launch()
        app.buttons["vault.item.fixture-github"].tap()
        let copy = app.buttons["vault.copy.password"]
        XCTAssertTrue(copy.waitForExistence(timeout: 5)); copy.tap()
        XCTAssertTrue(app.staticTexts.matching(identifier: "workspace.notice").firstMatch.waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["vault.field.more.password"].value as? String, "Copied")
        capture(app, "Sensitive copy takes one tap")
        XCTAssertEqual(copy.value as? String, "Hidden value")
        XCTAssertFalse(app.alerts.firstMatch.exists)
        XCTAssertFalse(app.sheets.firstMatch.exists)
    }

    func testListRowCopiesThePasswordInOneTap() {
        let app = launch()
        let row = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.swipeLeft()
        let copy = app.buttons["Copy password"].firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: 5)); copy.tap()
        XCTAssertTrue(app.staticTexts.matching(identifier: "workspace.notice").firstMatch.waitForExistence(timeout: 5))
        capture(app, "A list row copies its password in one tap")
    }

    func testFieldMenuRevealsAndHidesWithoutAnotherConfirmation() {
        let app = launch()
        app.buttons["vault.item.fixture-github"].tap()
        openFieldMenu(app, "password")
        XCTAssertTrue(app.buttons["vault.reveal.password"].waitForExistence(timeout: 5))
        Thread.sleep(forTimeInterval: 0.5)
        capture(app, "Password actions belong to the field menu")
        app.buttons["vault.reveal.password"].tap()
        let value = app.buttons["vault.copy.password"]
        let shown = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "fixture-secret"), object: value)
        XCTAssertEqual(XCTWaiter.wait(for: [shown], timeout: 5), .completed)
        XCTAssertFalse(app.alerts.firstMatch.exists)
        XCTAssertFalse(app.sheets.firstMatch.exists)
        openFieldMenu(app, "password"); app.buttons["vault.reveal.password"].tap()
        XCTAssertEqual(value.value as? String, "Hidden value")
    }

    func testLargeTypeReadsCopiesAndClearsWhenBackgrounded() {
        let app = launch()
        app.buttons["vault.item.fixture-github"].tap()
        openFieldMenu(app, "password"); app.buttons["vault.enlarge.password"].tap()
        let value = app.staticTexts["vault.large.value"]
        XCTAssertTrue(value.waitForExistence(timeout: 5))
        XCTAssertEqual(value.label, "fixture-secret")
        capture(app, "Large type password")
        app.buttons["vault.large.copy"].tap()
        let notice = app.staticTexts.matching(identifier: "workspace.notice").firstMatch
        XCTAssertTrue(notice.waitForExistence(timeout: 5))
        XCTAssertLessThanOrEqual(notice.frame.maxY, app.buttons["vault.large.copy"].frame.minY)
        capture(app, "Large type copy feedback")
        app.buttons["vault.large.done"].tap()
        XCTAssertEqual(app.buttons["vault.copy.password"].value as? String, "Hidden value")
        openFieldMenu(app, "password"); app.buttons["vault.enlarge.password"].tap()
        XCTAssertTrue(value.waitForExistence(timeout: 5))
        XCUIDevice.shared.press(.home)
        app.returnToForeground(showing: app.staticTexts.matching(identifier: "vault.detail.title"))
        XCTAssertFalse(value.exists)
        XCTAssertEqual(app.buttons["vault.copy.password"].value as? String, "Hidden value")
    }

    func testLargeTypeReadFailureCanRetryAndCancelWhileLoading() {
        let app = launch(["--ui-vault-field-slow", "--ui-vault-field-failure"])
        app.buttons["vault.item.fixture-github"].tap()
        openFieldMenu(app, "password"); app.buttons["vault.enlarge.password"].tap()
        XCTAssertTrue(app.activityIndicators["vault.large.loading"].waitForExistence(timeout: 3))
        XCTAssertFalse(app.buttons["vault.large.copy"].isEnabled)
        let retry = app.buttons["vault.large.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5)); capture(app, "Large type read failure")
        retry.tap()
        XCTAssertTrue(app.staticTexts["vault.large.value"].waitForExistence(timeout: 5))
        app.buttons["vault.large.done"].tap()
        openFieldMenu(app, "password"); app.buttons["vault.enlarge.password"].tap()
        XCTAssertTrue(app.activityIndicators["vault.large.loading"].waitForExistence(timeout: 3))
        app.buttons["vault.large.done"].tap()
        XCTAssertFalse(app.staticTexts["vault.large.value"].waitForExistence(timeout: 3))
        XCTAssertEqual(app.buttons["vault.copy.password"].value as? String, "Hidden value")
    }

    func testFavoriteAndWebsiteActionsStayAvailableInTheirMenus() {
        let app = launch(["--ui-vault-detail"])
        app.buttons["vault.item.fixture-api"].tap()
        app.buttons["vault.detail.more"].tap()
        let favorite = app.buttons["vault.detail.favorite"]
        XCTAssertTrue(favorite.waitForExistence(timeout: 5)); favorite.tap()
        app.buttons["vault.detail.more"].tap()
        XCTAssertEqual(favorite.label, "Remove favorite")
        favorite.tap()
        openFieldMenu(app, "url")
        XCTAssertTrue(app.buttons["vault.open.url"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["vault.reference.url"].exists)
        app.buttons["vault.field.copy.url"].tap()
        XCTAssertTrue(app.staticTexts.matching(identifier: "workspace.notice").firstMatch.waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["vault.copy.url"].value as? String, "https://api.example.org/developer/integrations/production?region=ap-southeast-1")
    }

    func testDetailAndLargeTypeAcrossLanguagesAndTextSizes() {
        for (language, dark, large) in [("zh-Hans", false, false), ("zh-Hans", true, false), ("de", false, true), ("ar", true, false)] {
            var arguments = ["--ui-vault-detail"]
            if dark { arguments.append("--ui-dark") }
            if large { arguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
            let app = launch(arguments, language: language)
            let row = app.buttons["vault.item.fixture-api"]
            XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
            let credential = app.buttons["vault.copy.credential"]
            for _ in 0..<5 where !credential.isHittable { app.swipeUp() }
            XCTAssertTrue(credential.isHittable)
            XCTAssertFalse(app.buttons["vault.copy.username"].exists)
            XCTAssertFalse(app.staticTexts["Empty section"].exists)
            capture(app, "Compact detail \(language) dark=\(dark) large=\(large)")
            openFieldMenu(app, "credential")
            Thread.sleep(forTimeInterval: 0.5)
            capture(app, "Field menu \(language)")
            app.buttons["vault.enlarge.credential"].tap()
            let value = app.staticTexts["vault.large.value"]
            XCTAssertTrue(value.waitForExistence(timeout: 5))
            XCTAssertTrue(value.label.contains("Second-line_0O1Il"))
            capture(app, "Large type long key \(language) dark=\(dark) large=\(large)")
            let copy = app.buttons["vault.large.copy"]
            for _ in 0..<12 where value.frame.maxY > copy.frame.minY {
                app.scrollViews["vault.large.scroll"].swipeUp()
            }
            XCTAssertLessThanOrEqual(value.frame.maxY, copy.frame.minY)
            capture(app, "End of large type value \(language)")
            app.buttons["vault.large.done"].tap()
            let region = app.buttons["vault.copy.region"]
            for _ in 0..<6 where !region.isHittable { app.swipeUp() }
            XCTAssertTrue(region.isHittable); region.tap()
            XCTAssertTrue(app.staticTexts.matching(identifier: "workspace.notice").firstMatch.waitForExistence(timeout: 5))
            capture(app, "Last field above the tab bar \(language)")
            app.terminate()
        }
    }

    func testHistoricalOTPExpiresWithoutAnotherFieldOperation() {
        // Leave enough time to observe the value after the system menu closes.
        let app = launch(["--ui-vault-long-code"])
        let row = app.buttons["vault.item.fixture-rich-0"]
        for _ in 0..<3 where !row.isHittable { app.swipeUp() }
        row.tap()
        XCTAssertTrue(app.buttons["vault.detail.more"].waitForExistence(timeout: 5))
        app.buttons["vault.detail.more"].tap(); app.buttons["Version history"].tap()
        let version = app.buttons["vault.version.1"]
        XCTAssertTrue(version.waitForExistence(timeout: 5)); version.tap()
        openFieldMenu(app, "otp"); app.buttons["vault.reveal.otp"].tap()
        let code = app.buttons["vault.copy.otp"]
        let shown = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "1 2 3 4 5 6"), object: code)
        XCTAssertEqual(XCTWaiter.wait(for: [shown], timeout: 3), .completed)
        let expired = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Hidden value"), object: code)
        XCTAssertEqual(XCTWaiter.wait(for: [expired], timeout: 8), .completed)
        capture(app, "Historical code disappears when it expires")
    }

    func testDeletionLeavesNoEditableDetail() {
        let app = launch()
        app.buttons["vault.item.fixture-github"].tap()
        let more = app.buttons["vault.detail.more"]
        XCTAssertTrue(more.waitForExistence(timeout: 5)); more.tap()
        let trash = app.buttons["vault.detail.trash"]
        XCTAssertTrue(trash.waitForExistence(timeout: 5)); trash.tap()
        app.sheets.buttons["Move to Trash"].tap()
        XCTAssertTrue(app.buttons["vault.scope.all"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["vault.item.fixture-github"].exists)
    }

    func testChineseDarkEditorAndGroupedDetail() {
        let app = launch(["--ui-dark"], language: "zh-Hans")
        let row = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        capture(app, "Chinese dark vault browser")
        row.tap()
        XCTAssertTrue(app.buttons["vault.edit.open"].waitForExistence(timeout: 5))
        capture(app, "Chinese dark vault detail")
        app.buttons["vault.edit.open"].tap()
        XCTAssertTrue(app.textFields["vault.edit.title"].waitForExistence(timeout: 5))
        capture(app, "Chinese dark vault editor")
    }

    func testStatusFeedbackDoesNotMoveRows() {
        let app = launch()
        let row = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        let before = row.frame.minY
        let feedback = app.descendants(matching: .any).matching(identifier: "workspace.feedback").firstMatch
        XCTAssertTrue(feedback.waitForNonExistence(timeout: 8))
        XCTAssertEqual(row.frame.minY, before, accuracy: 0.5)
        capture(app, "Status feedback keeps the list position")
    }

    func testSaveFailureRetainsDraftForRetry() {
        let app = launch(["--ui-vault-save-failure"])
        app.buttons["vault.item.fixture-github"].tap(); app.buttons["vault.edit.open"].tap()
        let title = app.textFields["vault.edit.title"]
        title.tap(); title.typeText(" changed")
        app.buttons["vault.edit.save"].tap()
        let failure = app.descendants(matching: .any).matching(identifier: "vault.edit.failure").firstMatch
        XCTAssertTrue(failure.waitForExistence(timeout: 5))
        XCTAssertTrue((title.value as? String)?.contains("changed") == true)
        app.buttons["vault.edit.save"].tap()
        XCTAssertTrue(app.staticTexts["vault.detail.title"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["vault.detail.title"].label.contains("changed"))
    }

    func testCopyFeedbackHasOneVisibleHost() {
        let app = launch()
        app.buttons["vault.item.fixture-github"].tap()
        let copy = app.buttons["vault.copy.username"]
        XCTAssertTrue(copy.waitForExistence(timeout: 5)); copy.tap()
        let notice = app.staticTexts.matching(identifier: "workspace.notice")
        XCTAssertTrue(notice.firstMatch.waitForExistence(timeout: 5))
        XCTAssertEqual(notice.count, 1)
        capture(app, "One copy confirmation in the active column")
    }

    func testKeyboardCanCreateAndSaveAnItem() {
        let app = launch()
        app.typeKey("n", modifierFlags: .command)
        let title = app.textFields["vault.edit.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        title.tap(); title.typeText("Keyboard item")
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(app.buttons["vault.item.fixture-created"].waitForExistence(timeout: 5))
        capture(app, "Keyboard creates and saves a vault item")
    }

    func testVaultControlsHaveAccessibleTargetsAndDescriptions() throws {
        let app = launch()
        app.buttons["vault.item.fixture-github"].tap()
        XCTAssertTrue(app.buttons["vault.field.more.password"].waitForExistence(timeout: 5))
        try app.performAccessibilityAudit(for: [.hitRegion, .trait, .sufficientElementDescription])
    }

    func testIncomingRevisionRefreshesTheOpenDetail() {
        let app = launch(["--ui-vault-refresh"])
        app.buttons["vault.item.fixture-github"].tap()
        let title = app.staticTexts["vault.detail.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        let updated = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", "GitHub updated"), object: title)
        XCTAssertEqual(XCTWaiter.wait(for: [updated], timeout: 12), .completed)
        XCTAssertEqual(app.buttons["vault.copy.password"].value as? String, "Hidden value")
    }

    func testIncomingRevisionDoesNotReplaceAnOpenDraft() {
        let app = launch(["--ui-vault-refresh"])
        app.buttons["vault.item.fixture-github"].tap(); app.buttons["vault.edit.open"].tap()
        let title = app.textFields["vault.edit.title"]
        title.tap(); title.typeText(" local draft")
        // The incoming revision arrives while this modal owns the UI. It must
        // not replace the input; after discarding, verify the new head exists.
        let overwritten = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value CONTAINS %@", "updated"), object: title)
        overwritten.isInverted = true
        XCTAssertEqual(XCTWaiter.wait(for: [overwritten], timeout: 10), .completed)
        XCTAssertEqual(title.value as? String, "GitHub local draft")
        app.navigationBars.buttons["Cancel"].tap(); app.sheets.buttons["Discard"].tap()
        let latest = app.staticTexts["vault.detail.title"]
        let refreshed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", "GitHub updated"), object: latest)
        XCTAssertEqual(XCTWaiter.wait(for: [refreshed], timeout: 5), .completed)
    }

    func testSwipeDismissRequiresADraftDecision() {
        let app = launch()
        app.buttons["vault.item.fixture-github"].tap(); app.buttons["vault.edit.open"].tap()
        let title = app.textFields["vault.edit.title"]
        title.tap(); title.typeText(" draft")
        let start = app.navigationBars["Edit secret"].coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.1))
        start.press(forDuration: 0.1, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.9)))
        let discard = app.sheets.buttons["Discard"]
        XCTAssertTrue(discard.waitForExistence(timeout: 5)); discard.tap()
        XCTAssertTrue(app.staticTexts["vault.detail.title"].waitForExistence(timeout: 5))
    }

    func testCancellingDateSelectionKeepsTheOptionalFieldEmpty() {
        let app = launch()
        app.buttons["vault.add"].tap()
        app.buttons["vault.edit.type"].tap(); app.buttons["API credential"].tap()
        let calendar = app.buttons["vault.edit.calendar.expires"]
        let form = app.collectionViews["vault.edit.form"]
        for _ in 0..<5 where !calendar.isHittable { form.swipeUp() }
        calendar.tap(); app.navigationBars["Date"].buttons["Cancel"].tap()
        let title = app.textFields["vault.edit.title"]
        for _ in 0..<5 where !title.isHittable { form.swipeDown() }
        title.tap(); title.typeText("Date draft")
        app.buttons["vault.edit.save"].tap()
        let row = app.buttons["vault.item.fixture-created"]
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        XCTAssertFalse(app.buttons["vault.copy.expires"].exists)
        app.buttons["vault.edit.open"].tap()
        let calendarAgain = app.buttons["vault.edit.calendar.expires"]
        for _ in 0..<5 where !calendarAgain.isHittable { app.swipeUp() }
        XCTAssertTrue(calendarAgain.isHittable)
        capture(app, "Empty optional fields remain available in the editor")
    }
}
