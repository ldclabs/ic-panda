import XCTest

@MainActor final class NavigationTests: XCTestCase {
    private func tab(_ app: XCUIApplication, symbol: String, title: String) -> XCUIElement {
        // The system can move these buttons outside TabBar into a side rail.
        let named = app.buttons[title].firstMatch
        if named.waitForExistence(timeout: 10) { return named }
        return app.descendants(matching: .any).matching(identifier: symbol).firstMatch
    }

    private func back(_ app: XCUIApplication) {
        let rail = app.buttons["BackButton"].firstMatch
        if rail.exists { rail.tap() }
        else { app.navigationBars.buttons.element(boundBy: 0).tap() }
    }

    func testProductionStartupAllowsPasskeyLoginWithoutStagingConfiguration() {
        let app = XCUIApplication()
        // Exercise normal startup and the real bridge, not presentation fixtures.
        app.launchArguments = ["-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let login = app.buttons["login.start"]
        XCTAssertTrue(login.waitForExistence(timeout: 10))
        let ready = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: login)
        XCTAssertEqual(XCTWaiter.wait(for: [ready], timeout: 10), .completed)
        XCTAssertFalse(app.alerts.firstMatch.exists)
        XCTAssertTrue(app.staticTexts["On your first visit, sign in and follow the steps to create your encrypted workspace."].exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Production startup with passkey self-registration"; shot.lifetime = .keepAlways; add(shot)
    }

    func testLanguageSettingsAvailableBeforeLoginAndInAccount() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-access", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let language = app.buttons["settings.language"]
        XCTAssertTrue(language.waitForExistence(timeout: 5))
        for _ in 0..<4 where !language.isHittable { app.swipeUp() }
        XCTAssertTrue(language.isHittable)
        XCTAssertEqual(language.label, "Language")
        let access = XCTAttachment(screenshot: app.screenshot())
        access.name = "Language entry before login"; access.lifetime = .keepAlways; add(access)

        app.terminate()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        tab(app, symbol: "Glyph-account", title: "我的").tap()
        for _ in 0..<8 where !language.isHittable { app.swipeUp() }
        XCTAssertTrue(language.isHittable)
        XCTAssertEqual(language.label, "语言")
        let account = XCTAttachment(screenshot: app.screenshot())
        account.name = "Language entry in account"; account.lifetime = .keepAlways; add(account)
    }

    func testPlanRemainsReadableWithoutPurchaseEntrypoints() {
        assertReadOnlyPlan(language: "en", locale: "en_US", accountTitle: "Account", dark: false)
    }

    func testReadOnlyPlanInDarkChinese() {
        assertReadOnlyPlan(language: "zh-Hans", locale: "zh_CN", accountTitle: "我的", dark: true)
    }

    private func assertReadOnlyPlan(language: String, locale: String, accountTitle: String, dark: Bool) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        tab(app, symbol: "Glyph-account", title: accountTitle).tap()
        let plan = app.buttons["account.plan"]
        for _ in 0..<6 where !plan.isHittable { app.swipeUp() }
        XCTAssertTrue(plan.isHittable); plan.tap()
        XCTAssertTrue(app.staticTexts["plan.purchases-disabled"].waitForExistence(timeout: 5))
        // LabeledContent exposes its localized label and value as one element.
        let currentPlan = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", "Free")).firstMatch
        XCTAssertTrue(currentPlan.exists)
        XCTAssertFalse(app.buttons["plan.restore"].exists)
        XCTAssertFalse(app.buttons["plan.manage"].exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Read-only plan · \(language)"; shot.lifetime = .keepAlways; add(shot)
    }

    func testCompactVaultAndLockClearContent() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.staticTexts["GitHub"].firstMatch.waitForExistence(timeout: 10))
        let row = app.buttons["vault.item.fixture-github"]
        XCTAssertGreaterThanOrEqual(row.frame.height, 44)
        XCTAssertLessThanOrEqual(row.frame.height, 60)
        tab(app, symbol: "Glyph-account", title: "Account").tap()
        if !app.buttons["account.lock"].isHittable { app.swipeUp() }
        app.buttons["account.lock"].tap()
        XCTAssertTrue(app.buttons["login.start"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["GitHub"].exists)
    }

    func testRepeatedManagedSecretAndFileNavigation() {
        checkDetailNavigation(language: "en", locale: "en_US", dark: false, repetitions: 3)
    }

    func testDetailNavigationInDarkChinese() {
        checkDetailNavigation(language: "zh-Hans", locale: "zh_CN", dark: true, repetitions: 1)
    }

    private func checkDetailNavigation(language: String, locale: String, dark: Bool, repetitions: Int) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-detail-navigation", "--ui-slow-vault", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        for iteration in 0..<repetitions {
            for (id, title) in [("fixture-managed", "dMsg controller #1"), ("fixture-note", "Project notes")] {
                let row = app.buttons["vault.item.\(id)"]
                XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
                let detail = app.staticTexts["vault.detail.title"]
                guard detail.waitForExistence(timeout: 5) else { XCTFail("Secret detail did not finish loading: \(id)"); return }
                XCTAssertEqual(detail.label, title)
                if id == "fixture-managed" {
                    XCTAssertFalse(app.buttons["vault.reveal.key"].isEnabled)
                    if iteration == 0 { capture(app, name: "Managed secret · \(language)") }
                }
                back(app)
            }
            tab(app, symbol: "Glyph-folder", title: language == "en" ? "Files" : "文件").tap()
            let file = app.buttons.containing(.staticText, identifier: "Local image.png").firstMatch
            XCTAssertTrue(file.waitForExistence(timeout: 5)); file.tap()
            XCTAssertTrue(app.buttons[language == "en" ? "Preview file" : "预览文件"].waitForExistence(timeout: 5))
            if iteration == 0 { capture(app, name: "File detail after secret navigation · \(language)") }
            back(app)
            tab(app, symbol: "Glyph-vault", title: language == "en" ? "Vault" : "保险库").tap()
        }
    }

    private func capture(_ app: XCUIApplication, name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }

    func testLeavingPendingSecretDoesNotReplaceNextDetail() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-detail-navigation", "--ui-slow-vault", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let managed = app.buttons["vault.item.fixture-managed"]
        XCTAssertTrue(managed.waitForExistence(timeout: 5)); managed.tap()
        back(app)
        let note = app.buttons["vault.item.fixture-note"]
        XCTAssertTrue(note.waitForExistence(timeout: 5)); note.tap()
        let detail = app.staticTexts["vault.detail.title"]
        XCTAssertTrue(detail.waitForExistence(timeout: 5))
        XCTAssertEqual(detail.label, "Project notes")
        XCTAssertFalse(app.alerts.firstMatch.exists)
    }

    func testRegularWidthSelectionReplacesPendingRead() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-detail-navigation", "--ui-slow-vault", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        guard app.frame.width > 700 else { throw XCTSkip("Requires a regular-width iPad") }
        let managed = app.buttons["vault.item.fixture-managed"]
        XCTAssertTrue(managed.waitForExistence(timeout: 5)); managed.tap()
        app.buttons["vault.item.fixture-note"].tap()
        let detail = app.staticTexts["vault.detail.title"]
        XCTAssertTrue(detail.waitForExistence(timeout: 5))
        XCTAssertEqual(detail.label, "Project notes")
        app.buttons["vault.favorite.fixture-github"].tap()
        let github = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", "GitHub"), object: detail)
        XCTAssertEqual(XCTWaiter.wait(for: [github], timeout: 5), .completed)
        capture(app, name: "iPad selection and favorites")
    }

    func testCancelledSecretReadOffersRetry() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-detail-navigation", "--ui-vault-load-failure", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let row = app.buttons["vault.item.fixture-managed"]
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        let retry = app.buttons["vault.detail.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "vault.detail.loading").firstMatch.exists)
        capture(app, name: "Interrupted secret read offers retry")
        retry.tap()
        let detail = app.staticTexts["vault.detail.title"]
        XCTAssertTrue(detail.waitForExistence(timeout: 5))
        XCTAssertEqual(detail.label, "dMsg controller #1")
    }

    func testSecretLoadingResumesAfterBackground() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-detail-navigation", "--ui-slow-vault", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let row = app.buttons["vault.item.fixture-managed"]
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        XCUIDevice.shared.press(.home)
        app.activate()
        let detail = app.staticTexts["vault.detail.title"]
        guard detail.waitForExistence(timeout: 8) else { XCTFail("Secret did not resume after background suspension"); return }
        XCTAssertEqual(detail.label, "dMsg controller #1")
        XCTAssertFalse(app.alerts.firstMatch.exists)
        capture(app, name: "Secret load recovered after background")
    }

    func testSecretFieldsRemainHiddenUntilRequested() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let item = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 10))
        item.tap()
        XCTAssertFalse(app.staticTexts["fixture-secret"].exists)
        XCTAssertTrue(app.buttons["vault.reveal.password"].waitForExistence(timeout: 5))
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "Secret fields remain concealed"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testDraftSurvivesWindowShapeChanges() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        tab(app, symbol: "Glyph-channels", title: "Messages").tap()
        let row = app.buttons["channel.row.fixture-channel"]
        // Regular-width portrait devices can keep the inactive tab's sidebar
        // collapsed. Exercise its system navigation before the draft assertion.
        if !row.waitForExistence(timeout: 2) {
            let sidebar = app.buttons["Show Sidebar"].firstMatch
            if sidebar.waitForExistence(timeout: 2) { sidebar.tap() }
        }
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        let composer = app.descendants(matching: .any).matching(identifier: "message.draft").firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 5))
        composer.tap(); composer.typeText("Keep this draft")
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue((composer.value as? String)?.contains("Keep this draft") == true)
        XCUIDevice.shared.orientation = .portrait
        XCTAssertTrue((composer.value as? String)?.contains("Keep this draft") == true)
    }

    func testLargeArabicLayoutKeepsFieldActionsReachable() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(ar)", "-AppleLocale", "ar",
                               "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch()
        let item = app.buttons["vault.favorite.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 5)); item.tap()
        XCTAssertEqual(app.staticTexts["vault.detail.title"].label, "GitHub")
        let reveal = app.buttons["vault.reveal.password"]
        XCTAssertTrue(reveal.waitForExistence(timeout: 5))
        if !reveal.isHittable { app.swipeUp() }
        XCTAssertTrue(reveal.isHittable)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Arabic accessibility layout"
        shot.lifetime = .keepAlways; add(shot)
    }

    func testSnapshotSelectionIsExplicitAndSurvivesRotation() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let item = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 10)); item.tap()
        let share = app.buttons["vault.share"]
        if !share.isHittable { app.swipeUp() }
        XCTAssertTrue(share.waitForExistence(timeout: 5)); share.tap()
        let password = app.switches["secret.snapshot.field.password"]
        let username = app.switches["secret.snapshot.field.username"]
        XCTAssertTrue(password.waitForExistence(timeout: 5))
        XCTAssertEqual(password.value as? String, "0")
        XCTAssertEqual(username.value as? String, "0")
        // SwiftUI exposes the whole Form row as a switch, but its center can
        // be label whitespace. Hit the actual trailing control in this LTR test.
        password.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        XCTAssertEqual(password.value as? String, "1", "The field must be selected before resizing")
        XCUIDevice.shared.orientation = .landscapeLeft
        XCUIDevice.shared.orientation = .portrait
        XCTAssertEqual(password.value as? String, "1")
        XCTAssertEqual(username.value as? String, "0")
        let prepare = app.buttons["secret.snapshot.prepare"]
        for _ in 0..<4 where !prepare.isHittable { app.swipeUp() }
        XCTAssertTrue(prepare.isEnabled)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Explicit secret snapshot selection"; shot.lifetime = .keepAlways; add(shot)
    }

    func testHardwarePageDoesNotInventAvailabilityWithoutCoreData() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        tab(app, symbol: "Glyph-account", title: "Account").tap()
        let hardware = app.buttons["account.hardware"]
        for _ in 0..<4 where !hardware.isHittable { app.swipeUp() }
        XCTAssertTrue(hardware.waitForExistence(timeout: 5)); hardware.tap()
        XCTAssertTrue(app.buttons["Retry loading hardware security"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Review key registration"].exists)
    }

    func testRecoveryWarningKeepsDetailNavigationReachable() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-recovery-pending", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.staticTexts["Account recovery is pending"].waitForExistence(timeout: 5))
        let item = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 5)); item.tap()
        XCTAssertTrue(app.buttons["vault.reveal.password"].waitForExistence(timeout: 5))
        let navigation = [app.navigationBars.buttons["Vault"].firstMatch,
                          app.buttons["Show Sidebar"].firstMatch, app.buttons["Hide Sidebar"].firstMatch,
                          app.buttons["BackButton"].firstMatch]
        XCTAssertTrue(navigation.contains { $0.exists && $0.isHittable })
        XCTAssertTrue(tab(app, symbol: "Glyph-account", title: "Account").isHittable)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Recovery warning preserves navigation"; shot.lifetime = .keepAlways; add(shot)
    }

    func testAccountShowsPublicNameAvatarAndSyncProgress() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-syncing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        tab(app, symbol: "Glyph-account", title: "Account").tap()
        XCTAssertTrue(app.staticTexts["account.name"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["account.name"].label, "Studio")
        XCTAssertTrue(app.images["account.avatar"].exists)
        let header = XCTAttachment(screenshot: app.screenshot())
        header.name = "Account public profile"; header.lifetime = .keepAlways; add(header)
        let sync = app.buttons["account.sync"]
        for _ in 0..<4 where !sync.isHittable { app.swipeUp() }
        XCTAssertTrue(sync.exists)
        XCTAssertEqual(sync.label, "Syncing…")
        XCTAssertFalse(sync.isEnabled)
        let progress = XCTAttachment(screenshot: app.screenshot())
        progress.name = "Manual sync progress"; progress.lifetime = .keepAlways; add(progress)
    }

    func testAccessActionsAreAlignedWithoutLegacyLogin() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-access", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        let unlock = app.buttons["login.unlock"], authorize = app.buttons["login.authorize"], login = app.buttons["login.start"]
        XCTAssertTrue(unlock.waitForExistence(timeout: 5))
        XCTAssertEqual(unlock.frame.width, authorize.frame.width, accuracy: 2)
        XCTAssertEqual(unlock.frame.width, login.frame.width, accuracy: 2)
        XCTAssertTrue(unlock.isHittable && authorize.isHittable && login.isHittable)
        XCTAssertEqual(app.switches.count, 0)
        XCTAssertTrue(app.staticTexts["要在这台 iPhone 上使用已有账号，请在一台已授权设备上批准此设备。"].exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Chinese access actions"; shot.lifetime = .keepAlways; add(shot)
    }

    func testAccessActionsRemainReachableInDarkArabic() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-access", "--ui-dark", "-AppleLanguages", "(ar)", "-AppleLocale", "ar",
                               "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch()
        for identifier in ["login.unlock", "login.authorize", "login.start"] {
            let button = app.buttons[identifier]
            XCTAssertTrue(button.waitForExistence(timeout: 5))
            for _ in 0..<6 where !button.isHittable { app.swipeUp() }
            XCTAssertTrue(button.isHittable)
        }
        XCTAssertEqual(app.switches.count, 0)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Dark Arabic large-text access actions"; shot.lifetime = .keepAlways; add(shot)
        for identifier in ["settings.language", "access.demo", "access.privacy"] {
            let link = app.buttons[identifier]
            for _ in 0..<6 where !link.isHittable { app.swipeUp() }
            XCTAssertTrue(link.isHittable)
        }
        let footer = XCTAttachment(screenshot: app.screenshot())
        footer.name = "Dark Arabic large-text access footer"; footer.lifetime = .keepAlways; add(footer)
    }

    func testFilesShowCloudBackupsWithoutOpeningImportMenu() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        tab(app, symbol: "Glyph-folder", title: "Files").tap()
        XCTAssertTrue(app.staticTexts["Project brief.pdf"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["backup.restore.fixture-backup"].isHittable)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Files shows another device's cloud backup"; shot.lifetime = .keepAlways; add(shot)
    }

    func testAccountHeaderWrapsLongNameInDarkChinese() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-dark", "--ui-long-profile", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        tab(app, symbol: "Glyph-account", title: "我的").tap()
        let name = app.staticTexts["account.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        XCTAssertEqual(name.label, "Studio Research · Design, Security and Collaboration")
        XCTAssertTrue(name.isHittable)
        XCTAssertLessThanOrEqual(name.frame.maxX, app.frame.maxX - 16)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Dark Chinese account with a long public name"; shot.lifetime = .keepAlways; add(shot)
    }
    func testPublicDemoAndOfflinePrivacyNeverEnterARealWorkspace() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-access", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let demo = app.buttons["access.demo"]
        XCTAssertTrue(demo.waitForExistence(timeout: 5))
        for _ in 0..<4 { if demo.isHittable { break }; app.swipeUp() }
        demo.tap()
        XCTAssertTrue(app.staticTexts["Local demo · Fictitious data · Resets when closed"].waitForExistence(timeout: 5))
        app.buttons["demo.login"].firstMatch.tap()
        XCTAssertTrue(app.secureTextFields["Sample password"].waitForExistence(timeout: 5))
        let sample = XCTAttachment(screenshot: app.screenshot())
        sample.name = "Public local demo with fictional credentials"; sample.lifetime = .keepAlways; add(sample)
        back(app)
        back(app)
        XCTAssertTrue(app.buttons["login.start"].exists)
        let privacy = app.buttons["access.privacy"]
        for _ in 0..<4 { if privacy.isHittable { break }; app.swipeUp() }
        privacy.tap()
        XCTAssertTrue(app.staticTexts["dMsg Privacy Policy"].waitForExistence(timeout: 5))
        let policy = XCTAttachment(screenshot: app.screenshot())
        policy.name = "Privacy available before login"; policy.lifetime = .keepAlways; add(policy)
    }

}
