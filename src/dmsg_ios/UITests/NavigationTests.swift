import XCTest

@MainActor class NavigationTestCase: XCTestCase {
    fileprivate func tab(_ app: XCUIApplication, symbol: String, title: String) -> XCUIElement {
        // The system can move these buttons outside TabBar into a side rail.
        let named = app.buttons[title].firstMatch
        if named.waitForExistence(timeout: 10) { return named }
        return app.descendants(matching: .any).matching(identifier: symbol).firstMatch
    }

    /// SwiftUI reports a `Link` as a button on iOS 18 and as a link on iOS 27.
    fileprivate func linkOrButton(_ app: XCUIApplication, _ identifier: String) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    fileprivate func back(_ app: XCUIApplication) {
        let rail = app.buttons["BackButton"].firstMatch
        if ProcessInfo.processInfo.operatingSystemVersion.majorVersion >= 26 {
            XCTAssertTrue(rail.waitForExistence(timeout: 3)); rail.tap()
        }
        else if rail.exists { rail.tap() }
        else { app.navigationBars.buttons.element(boundBy: 0).tap() }
    }

    fileprivate func capture(_ app: XCUIApplication, name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}

final class NavigationTests: NavigationTestCase {
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
        let language = linkOrButton(app, "settings.language")
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

    func testExpandedLanguageAccessScreens() {
        // One language per script and text width: `localizations.py` checks every catalog.
        let cases = [
            ("zh-Hant", "zh_TW", "語言"), ("hi", "hi_IN", "भाषा"),
            ("de", "de_DE", "Sprache"), ("ar", "ar_SA", "اللغة"),
            ("th", "th_TH", "ภาษา"), ("ur", "ur_PK", "زبان"),
        ]
        let app = XCUIApplication()
        for (index, item) in cases.enumerated() {
            let (language, locale, label) = item
            app.launchArguments = ["--ui-testing", "--ui-access", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
            if index.isMultiple(of: 2) { app.launchArguments.append("--ui-dark") }
            app.launch()
            let button = linkOrButton(app, "settings.language")
            XCTAssertTrue(button.waitForExistence(timeout: 10), language)
            for _ in 0..<4 where !button.isHittable { app.swipeUp() }
            XCTAssertTrue(button.isHittable, language)
            XCTAssertEqual(button.label, label, language)
            XCTAssertFalse(app.staticTexts["Welcome back."].exists, language)
            XCTAssertFalse(app.buttons["login.start"].label.contains("Continue with Internet Identity"), language)
            XCTAssertFalse(app.buttons["login.unlock"].label.contains("Unlock with system authentication"), language)
            let screenshot = XCTAttachment(screenshot: app.screenshot())
            screenshot.name = "Access language · \(language)"; screenshot.lifetime = .keepAlways; add(screenshot)
            app.terminate()
        }
    }

    func testPlanRemainsReadableWithoutPurchaseEntrypoints() {
        assertReadOnlyPlan(language: "en", locale: "en_US", accountTitle: "Account", dark: false)
    }

    func testReadOnlyPlanInDarkChinese() {
        assertReadOnlyPlan(language: "zh-Hans", locale: "zh_CN", accountTitle: "我的", dark: true)
    }

    func testReadOnlyPlanInDarkArabic() {
        assertReadOnlyPlan(language: "ar", locale: "ar_SA", accountTitle: "الحساب", dark: true)
    }

    func testForeignAppleSubscriptionOffersAccountSwitchInsteadOfPurchase() {
        for (language, title, dark) in [("en", "Account", false), ("zh-Hans", "我的", true), ("de", "Konto", false), ("ar", "الحساب", true)] {
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "--ui-foreign-subscription", "-AppleLanguages", "(\(language))"] + (dark ? ["--ui-dark"] : [])
            app.launch()
            tab(app, symbol: "Glyph-account", title: title).tap()
            let plan = app.buttons["account.plan"]
            for _ in 0..<6 where !plan.isHittable { app.swipeUp() }
            XCTAssertTrue(plan.isHittable); plan.tap()
            let notice = app.descendants(matching: .any).matching(identifier: "plan.subscription-account").firstMatch
            let switchAccount = app.buttons["plan.switch-account"]
            for _ in 0..<8 where !switchAccount.isHittable { app.swipeUp() }
            XCTAssertTrue(notice.exists)
            XCTAssertEqual(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "plan.purchase.")).count, 0)
            capture(app, name: "Apple subscription belongs to another account · \(language)")
            XCTAssertTrue(switchAccount.isHittable); switchAccount.tap()
            XCTAssertTrue(app.buttons["accounts.040g2082040g2082040g"].waitForExistence(timeout: 5))
            app.terminate()
        }
    }

    func testUnavailableProductsOfferRetryWithoutHidingRestore() {
        for (language, title, dark) in [("zh-Hans", "我的", false), ("zh-Hans", "我的", true), ("de", "Konto", false), ("ar", "الحساب", true)] {
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "--ui-purchases", "-AppleLanguages", "(\(language))"] + (dark ? ["--ui-dark"] : [])
            app.launch()
            tab(app, symbol: "Glyph-account", title: title).tap()
            let plan = app.buttons["account.plan"]
            for _ in 0..<6 where !plan.isHittable { app.swipeUp() }
            XCTAssertTrue(plan.isHittable); plan.tap()
            let retry = app.buttons["plan.products-retry"]
            for _ in 0..<8 where !retry.isHittable { app.swipeUp() }
            XCTAssertTrue(retry.isHittable); XCTAssertTrue(retry.isEnabled)
            XCTAssertTrue(app.staticTexts["plan.products-unavailable"].exists)
            XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "plan.purchase-progress").firstMatch.exists)
            XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "plan.purchase-result").firstMatch.exists)
            retry.tap()
            let restore = app.buttons["plan.restore"]
            for _ in 0..<4 where !restore.isHittable { app.swipeUp() }
            XCTAssertTrue(restore.isEnabled)
            XCTAssertTrue(app.buttons["plan.manage"].exists)
            capture(app, name: "Unavailable products retry · \(language) · \(dark ? "dark" : "light")")
            app.terminate()
        }
    }

    func testLoadingProductsDoNotReportUnavailable() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-purchases", "--ui-products-loading", "-AppleLanguages", "(en)"]
        app.launch()
        tab(app, symbol: "Glyph-account", title: "Account").tap()
        let plan = app.buttons["account.plan"]
        for _ in 0..<6 where !plan.isHittable { app.swipeUp() }
        XCTAssertTrue(plan.isHittable); plan.tap()
        let progress = app.descendants(matching: .any).matching(identifier: "plan.purchase-progress").firstMatch
        for _ in 0..<8 where !progress.isHittable { app.swipeUp() }
        XCTAssertTrue(progress.isHittable)
        XCTAssertFalse(app.staticTexts["plan.products-unavailable"].exists)
        XCTAssertFalse(app.buttons["plan.products-retry"].exists)
        capture(app, name: "Product loading is not unavailable")
    }

    func testSubscriptionPageRecoversAfterReturningFromBackground() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-subscription-status-unavailable", "-AppleLanguages", "(zh-Hans)"]
        app.launch()
        tab(app, symbol: "Glyph-account", title: "我的").tap()
        let plan = app.buttons["account.plan"]
        for _ in 0..<6 where !plan.isHittable { app.swipeUp() }
        XCTAssertTrue(plan.isHittable); plan.tap()
        let retry = app.buttons["plan.products-retry"]
        for _ in 0..<8 where !retry.isHittable { app.swipeUp() }
        XCTAssertTrue(retry.isEnabled)
        XCUIDevice.shared.press(.home)
        app.activate()
        let visible = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"), object: retry)
        XCTAssertEqual(XCTWaiter.wait(for: [visible], timeout: 10), .completed)
        XCTAssertTrue(retry.isEnabled)
        XCTAssertTrue(app.buttons["plan.restore"].isEnabled)
        XCTAssertFalse(app.alerts.firstMatch.exists)
        capture(app, name: "Subscription recovery after returning · Chinese")
    }

    func testSubscriptionStatusReadFailureStaysInlineAndOffersRetry() {
        for (language, title, dark) in [("zh-Hans", "我的", false), ("zh-Hans", "我的", true), ("ar", "الحساب", true)] {
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "--ui-subscription-status-unavailable", "-AppleLanguages", "(\(language))"] + (dark ? ["--ui-dark"] : [])
            app.launch()
            tab(app, symbol: "Glyph-account", title: title).tap()
            let plan = app.buttons["account.plan"]
            for _ in 0..<6 where !plan.isHittable { app.swipeUp() }
            XCTAssertTrue(plan.isHittable); plan.tap()
            let retry = app.buttons["plan.products-retry"]
            for _ in 0..<8 where !retry.isHittable { app.swipeUp() }
            XCTAssertTrue(app.descendants(matching: .any).matching(identifier: "plan.store-error").firstMatch.exists)
            XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "plan.subscription-account").firstMatch.exists)
            XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "plan.purchase-result").firstMatch.exists)
            XCTAssertFalse(app.alerts.firstMatch.exists)
            XCTAssertTrue(retry.isEnabled); retry.tap()
            XCTAssertFalse(app.alerts.firstMatch.exists)
            let restore = app.buttons["plan.restore"]
            for _ in 0..<4 where !restore.isHittable { app.swipeUp() }
            XCTAssertTrue(restore.isEnabled)
            XCTAssertTrue(app.buttons["plan.manage"].exists)
            capture(app, name: "Subscription status retry · \(language) · \(dark ? "dark" : "light")")
            app.terminate()
        }
    }

    func testScheduledAppleDowngradeKeepsCurrentPlan() {
        for (language, title, dark) in [("en", "Account", false), ("zh-Hans", "我的", true), ("de", "Konto", false), ("ar", "الحساب", true)] {
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "--ui-scheduled-subscription", "-AppleLanguages", "(\(language))"] + (dark ? ["--ui-dark"] : [])
            app.launch()
            tab(app, symbol: "Glyph-account", title: title).tap()
            let plan = app.buttons["account.plan"]
            for _ in 0..<6 where !plan.isHittable { app.swipeUp() }
            XCTAssertTrue(plan.isHittable); plan.tap()
            let scheduled = app.descendants(matching: .any).matching(identifier: "plan.next-renewal").firstMatch
            for _ in 0..<8 where !scheduled.isHittable { app.swipeUp() }
            XCTAssertTrue(scheduled.isHittable)
            XCTAssertTrue(app.descendants(matching: .any).matching(identifier: "plan.next-renewal")
                .matching(NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", "Plus", "Plus")).firstMatch.exists)
            XCTAssertTrue(app.descendants(matching: .any).matching(identifier: "plan.current-subscription")
                .matching(NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", "Pro", "Pro")).firstMatch.exists)
            XCTAssertEqual(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "plan.purchase.")).count, 0)
            capture(app, name: "Pro now, Plus at next renewal · \(language)")
            app.terminate()
        }
    }

    func testSubscriptionActionsAndProgressAlignInChineseGermanAndArabic() {
        for (language, title, dark) in [("zh-Hans", "我的", false), ("zh-Hans", "我的", true), ("de", "Konto", false), ("ar", "الحساب", true)] {
            for working in [false, true] {
                let app = XCUIApplication()
                app.launchArguments = ["--ui-testing", "--ui-foreign-subscription", "-AppleLanguages", "(\(language))"]
                    + (dark ? ["--ui-dark"] : []) + (working ? ["--ui-purchase-working"] : [])
                app.launch()
                tab(app, symbol: "Glyph-account", title: title).tap()
                let plan = app.buttons["account.plan"]
                for _ in 0..<6 where !plan.isHittable { app.swipeUp() }
                XCTAssertTrue(plan.isHittable); plan.tap()
                let restore = app.buttons["plan.restore"], manage = app.buttons["plan.manage"]
                for _ in 0..<8 where !manage.isHittable { app.swipeUp() }
                XCTAssertTrue(restore.exists); XCTAssertTrue(manage.exists)
                XCTAssertEqual(restore.isEnabled, !working)
                XCTAssertEqual(manage.isEnabled, !working)
                XCTAssertEqual(restore.frame.midX, manage.frame.midX, accuracy: 1)
                let progress = app.descendants(matching: .any).matching(identifier: "plan.purchase-progress").firstMatch
                XCTAssertEqual(progress.exists, working)
                if working {
                    XCTAssertEqual(progress.frame.midX, restore.frame.midX, accuracy: 1)
                }
                capture(app, name: "Subscription alignment · \(language) · \(dark ? "dark" : "light") · \(working ? "loading" : "ready")")
                app.terminate()
            }
        }
    }

    func testSandboxSubscriptionNoticeInChinese() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-sandbox-subscription", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        tab(app, symbol: "Glyph-account", title: "我的").tap()
        let plan = app.buttons["account.plan"]
        for _ in 0..<6 where !plan.isHittable { app.swipeUp() }
        XCTAssertTrue(plan.isHittable); plan.tap()
        let notice = app.staticTexts["plan.sandbox"]
        XCTAssertTrue(notice.waitForExistence(timeout: 10))
        XCTAssertTrue(notice.label.contains("不会实际扣款"))
        capture(app, name: "Sandbox subscription · Chinese")
    }

    func testMembershipEntryAndManagedProSubscriptionInChinese() {
        assertMembershipEntry(language: "zh-Hans", locale: "zh_CN", accountTitle: "我的", tier: "Pro", dark: false)
    }

    func testMaxMembershipEntryInDarkArabic() {
        assertMembershipEntry(language: "ar", locale: "ar_SA", accountTitle: "الحساب", tier: "Max", dark: true)
    }

    func testFreeMembershipBadgeOpensPlanSelection() {
        assertMembershipEntry(language: "en", locale: "en_US", accountTitle: "Account", tier: "Free", dark: false)
    }

    func testMembershipLoadsWhenUnlockUncoversAccountWithoutSwitchingTabs() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-account-unlock", "--ui-sandbox-subscription", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        let badge = app.buttons["account.membership-badge"]
        XCTAssertTrue(badge.waitForExistence(timeout: 10))
        XCTAssertTrue(badge.label.contains("Pro"))
        XCTAssertTrue(app.buttons["account.profile"].exists)
        XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "account.membership-loading").firstMatch.exists)
        capture(app, name: "Membership after uncovering Account")
        XCUIDevice.shared.press(.home)
        app.activate()
        let visible = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"), object: badge)
        XCTAssertEqual(XCTWaiter.wait(for: [visible], timeout: 10), .completed)
        XCTAssertTrue(badge.label.contains("Pro"))
        XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "account.membership-loading").firstMatch.exists)
        capture(app, name: "Membership after returning to Account")
    }

    func testMembershipHeaderAdaptsToLongNameAndAccessibilityText() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-sandbox-subscription", "--ui-long-profile", "--ui-dark",
                               "-AppleLanguages", "(en)", "-AppleLocale", "en_US",
                               "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch()
        tab(app, symbol: "Glyph-account", title: "Account").tap()
        let profile = app.buttons["account.profile"], badge = app.buttons["account.membership-badge"]
        XCTAssertTrue(badge.waitForExistence(timeout: 10))
        XCTAssertGreaterThanOrEqual(badge.frame.minY, profile.frame.maxY)
        XCTAssertTrue(badge.isHittable)
        XCTAssertFalse(linkOrButton(app, "account.upgrade").exists)
        capture(app, name: "Membership header · long name · largest text")
    }

    private func assertMembershipEntry(language: String, locale: String, accountTitle: String, tier: String, dark: Bool) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-purchases", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if tier == "Pro" { app.launchArguments.append("--ui-sandbox-subscription") }
        if tier == "Max" { app.launchArguments.append("--ui-max-subscription") }
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        tab(app, symbol: "Glyph-account", title: accountTitle).tap()
        let badge = app.buttons["account.membership-badge"]
        XCTAssertTrue(badge.waitForExistence(timeout: 10)); XCTAssertTrue(badge.label.contains(tier))
        XCTAssertTrue(badge.isHittable)
        XCTAssertGreaterThanOrEqual(badge.frame.height, 44)
        let profile = app.buttons["account.profile"]
        XCTAssertEqual(profile.frame.midY, badge.frame.midY, accuracy: 2)
        XCTAssertFalse(profile.frame.intersects(badge.frame))
        XCTAssertFalse(linkOrButton(app, "account.upgrade").exists)
        capture(app, name: "Membership entry · \(tier) · \(language)")
        if tier == "Free" {
            profile.tap()
            XCTAssertTrue(app.textFields["profile.name"].waitForExistence(timeout: 10))
            back(app)
            XCTAssertTrue(badge.waitForExistence(timeout: 10))
        }
        // The Free badge opens plan selection. Paid membership uses Apple's
        // sheet; inspect the dMsg plan screen through its remaining list row.
        if tier == "Free" { badge.tap() }
        else {
            let plan = app.buttons["account.plan"]
            for _ in 0..<4 where !plan.isHittable { app.swipeUp() }
            XCTAssertTrue(plan.isHittable); plan.tap()
        }
        let usage = app.descendants(matching: .any).matching(identifier: "plan.usage").firstMatch
        XCTAssertTrue(usage.waitForExistence(timeout: 10))
        let manage = app.buttons["plan.manage"]
        for _ in 0..<5 where !manage.isHittable { app.swipeUp() }
        XCTAssertTrue(manage.isHittable)
        XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "plan.backup-limit.Free").firstMatch.exists)
        if tier != "Free" {
            XCTAssertTrue(app.descendants(matching: .any).matching(identifier: "plan.current-subscription").firstMatch.exists)
            XCTAssertEqual(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "plan.purchase.")).count, 0)
        }
        capture(app, name: "Subscription management · \(tier) · \(language)")
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
        XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "plan.backup-limit.Free").firstMatch.exists)
        // LabeledContent exposes its localized label and value as one element.
        let currentPlan = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", "Free")).firstMatch
        XCTAssertTrue(currentPlan.exists)
        let disabled = app.staticTexts["plan.purchases-disabled"]
        for _ in 0..<4 where !disabled.isHittable { app.swipeUp() }
        XCTAssertTrue(disabled.exists)
        XCTAssertFalse(app.buttons["plan.restore"].exists)
        XCTAssertFalse(app.buttons["plan.manage"].exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Read-only plan · \(language)"; shot.lifetime = .keepAlways; add(shot)
    }
}

final class SecretNavigationTests: NavigationTestCase {
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

    func testRepeatedManagedSecretAndFileNavigation() throws {
        try checkDetailNavigation(language: "en", locale: "en_US", dark: false, repetitions: 3)
    }

    func testDetailNavigationInDarkChinese() throws {
        try checkDetailNavigation(language: "zh-Hans", locale: "zh_CN", dark: true, repetitions: 1)
    }

    private func checkDetailNavigation(language: String, locale: String, dark: Bool, repetitions: Int) throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-detail-navigation", "--ui-slow-vault", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        app.launch()
        guard app.frame.width < 700 else { throw XCTSkip("Goes back, which a split layout has no need to") }
        for iteration in 0..<repetitions {
            for (id, title) in [("fixture-managed", "dMsg controller #1"), ("fixture-note", "Project notes")] {
                let row = app.buttons["vault.item.\(id)"]
                XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
                let detail = app.staticTexts["vault.detail.title"]
                guard detail.waitForExistence(timeout: 5) else { XCTFail("Secret detail did not finish loading: \(id)"); return }
                XCTAssertEqual(detail.label, title)
                if id == "fixture-managed" {
                    XCTAssertFalse(app.buttons["vault.field.more.key"].exists)
                    XCTAssertFalse(app.buttons["vault.copy.key"].exists)
                    if iteration == 0 { capture(app, name: "Managed secret · \(language)") }
                }
                back(app)
            }
            tab(app, symbol: "Glyph-folder", title: language == "en" ? "Files" : "文件").tap()
            let file = app.buttons["file.item.fixture-local"]
            XCTAssertTrue(file.waitForExistence(timeout: 5)); file.tap()
            XCTAssertTrue(app.buttons[language == "en" ? "Preview file" : "预览文件"].waitForExistence(timeout: 5))
            if iteration == 0 { capture(app, name: "File detail after secret navigation · \(language)") }
            back(app)
            tab(app, symbol: "Glyph-vault", title: language == "en" ? "Vault" : "保险库").tap()
        }
    }

    func testLeavingPendingSecretDoesNotReplaceNextDetail() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-detail-navigation", "--ui-slow-vault", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        guard app.frame.width < 700 else { throw XCTSkip("Goes back, which a split layout has no need to") }
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
        func select(_ identifier: String) {
            let row = app.buttons[identifier]
            XCTAssertTrue(row.waitForExistence(timeout: 5))
            if !row.isHittable { app.buttons["ToggleSidebar"].tap() }
            row.tap()
        }
        select("vault.item.fixture-managed")
        select("vault.item.fixture-note")
        let detail = app.staticTexts["vault.detail.title"]
        XCTAssertTrue(detail.waitForExistence(timeout: 5))
        XCTAssertEqual(detail.label, "Project notes")
        select("vault.item.fixture-github")
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
        app.returnToForeground(showing: app.staticTexts.matching(identifier: "vault.detail.title"))
        XCTAssertEqual(app.staticTexts["vault.detail.title"].label, "dMsg controller #1")
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
        XCTAssertTrue(app.buttons["vault.field.more.password"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["vault.copy.password"].value as? String, "Hidden value")
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
        let item = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 5)); item.tap()
        XCTAssertEqual(app.staticTexts["vault.detail.title"].label, "GitHub")
        let reveal = app.buttons["vault.field.more.password"]
        for _ in 0..<6 where !reveal.exists || !reveal.isHittable { app.swipeUp() }
        XCTAssertTrue(reveal.exists)
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
        let more = app.buttons["vault.detail.more"]
        XCTAssertTrue(more.waitForExistence(timeout: 5)); more.tap()
        let share = app.buttons["vault.share"]
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
}

final class AccountNavigationTests: NavigationTestCase {
    func testIPadAccountStaysResponsiveAcrossWorkspaceTabs() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-store-screenshots", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        guard app.frame.width > 700 else { throw XCTSkip("Requires a regular-width iPad") }
        for _ in 0..<3 {
            tab(app, symbol: "Glyph-folder", title: "Files").tap()
            XCTAssertTrue(app.buttons["files.scope.local"].waitForExistence(timeout: 5))
            tab(app, symbol: "Glyph-account", title: "Account").tap()
            let security = linkOrButton(app, "account.security")
            if !security.isHittable, app.buttons["ToggleSidebar"].exists { app.buttons["ToggleSidebar"].tap() }
            XCTAssertTrue(security.waitForExistence(timeout: 5))
            XCTAssertTrue(security.isHittable)
            security.tap()
            XCTAssertTrue(linkOrButton(app, "security.recovery").waitForExistence(timeout: 5))
            tab(app, symbol: "Glyph-vault", title: "Vault").tap()
            XCTAssertTrue(app.buttons["vault.item.fixture-github"].waitForExistence(timeout: 5))
        }
        capture(app, name: "iPad account navigation remains responsive")
    }

    func testHardwarePageDoesNotInventAvailabilityWithoutCoreData() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        tab(app, symbol: "Glyph-account", title: "Account").tap()
        let advanced = app.buttons["account.advanced"]
        for _ in 0..<6 where !advanced.isHittable { app.swipeUp() }
        advanced.tap()
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
        let status = app.buttons["activity.open"].firstMatch
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertEqual(status.label, "Account recovery is pending")
        let item = app.buttons["vault.item.fixture-github"]
        XCTAssertTrue(item.waitForExistence(timeout: 5)); item.tap()
        XCTAssertTrue(app.buttons["vault.field.more.password"].waitForExistence(timeout: 5))
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
        let syncSettings = app.buttons["account.sync-settings"]
        for _ in 0..<5 where !syncSettings.isHittable { app.swipeUp() }
        syncSettings.tap()
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
        for identifier in ["settings.language", "access.privacy"] {
            let link = linkOrButton(app, identifier)
            for _ in 0..<6 where !link.isHittable { app.swipeUp() }
            XCTAssertTrue(link.isHittable)
        }
        let footer = XCTAttachment(screenshot: app.screenshot())
        footer.name = "Dark Arabic large-text access footer"; footer.lifetime = .keepAlways; add(footer)
    }

    func testCacheExplanationAndControlsInEnglishChineseFrenchAndArabic() {
        for (language, locale, account, dark) in [("en", "en_US", "Account", false), ("zh-Hans", "zh_CN", "我的", true), ("fr", "fr_FR", "Compte", false), ("ar", "ar_SA", "الحساب", true)] {
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "-AppleLanguages", "(\(language))", "-AppleLocale", locale] + (dark ? ["--ui-dark"] : [])
            app.launch()
            tab(app, symbol: "Glyph-account", title: account).tap()
            let advanced = app.buttons["account.advanced"]
            for _ in 0..<8 where !advanced.isHittable { app.swipeUp() }
            advanced.tap()
            let cache = app.buttons["account.cache"]
            XCTAssertTrue(cache.waitForExistence(timeout: 5))
            for _ in 0..<8 where !cache.isHittable { app.swipeUp() }
            XCTAssertTrue(cache.isHittable); cache.tap()
            XCTAssertTrue(app.staticTexts["cache.explanation"].waitForExistence(timeout: 5))
            let clear = app.buttons["cache.clear"]
            XCTAssertTrue(clear.isHittable)
            XCTAssertFalse(clear.isEnabled, "a presentation fixture without a core cannot clear a real workspace")
            capture(app, name: "Encrypted cache explanation · \(language) · \(dark ? "dark" : "light")")
            app.terminate()
        }
    }

    func testFilesShowCloudBackupsWithoutOpeningImportMenu() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        tab(app, symbol: "Glyph-folder", title: "Files").tap()
        let cloudScope = app.buttons["files.scope.cloud"]
        if cloudScope.exists && cloudScope.isHittable { cloudScope.tap() }
        else { app.buttons["files.scope.local"].tap(); app.buttons["Cloud backups"].firstMatch.tap() }
        let backup = app.buttons["backup.item.fixture-backup"]
        XCTAssertTrue(backup.waitForExistence(timeout: 5)); backup.tap()
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
    func testAccessHasNoDemoAndPrivacyIsAvailableOffline() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-access", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.buttons["login.start"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["access.demo"].exists)
        XCTAssertFalse(linkOrButton(app, "Contact support").exists)
        let privacy = app.buttons["access.privacy"]
        for _ in 0..<6 where !privacy.isHittable { app.swipeUp() }
        privacy.tap()
        XCTAssertTrue(app.staticTexts["dMsg Privacy Policy"].waitForExistence(timeout: 5))
        XCTAssertTrue(linkOrButton(app, "Contact support").exists)
    }
}

final class DeviceNavigationTests: NavigationTestCase {
    func testDeviceHandoverEntriesInEnglish() { handover(language: "en", locale: "en_US", account: "Account", dark: false) }

    func testDeviceHandoverEntriesInDarkArabic() { handover(language: "ar", locale: "ar_SA", account: "الحساب", dark: true) }

    /// Devices links to replacing a device and to device transfers; without
    /// a core both show only what starts them.
    private func handover(language: String, locale: String, account: String, dark: Bool) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "-AppleLanguages", "(\(language))", "-AppleLocale", locale] + (dark ? ["--ui-dark"] : [])
        app.launch()
        tab(app, symbol: "Glyph-account", title: account).tap()
        let devices = app.buttons["account.devices"]
        XCTAssertTrue(devices.waitForExistence(timeout: 5))
        for _ in 0..<4 where !devices.isHittable { app.swipeUp() }
        devices.tap()
        let succession = app.buttons["devices.succession"], transfer = app.buttons["devices.transfer"]
        XCTAssertTrue(succession.waitForExistence(timeout: 5))
        XCTAssertTrue(transfer.exists)
        capture(app, name: "Devices with handover entries · \(language)")
        succession.tap()
        XCTAssertTrue(app.buttons["succession.review"].waitForExistence(timeout: 5))
        capture(app, name: "Replace a device · \(language)")
        back(app)
        XCTAssertTrue(transfer.waitForExistence(timeout: 5))
        transfer.tap()
        XCTAssertTrue(app.buttons["transfer.receive"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["transfer.resume"].exists)
        capture(app, name: "Transfer between devices · \(language)")
    }

    func testReplaceDeviceRequestBeforeJoiningInChinese() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-access", "--ui-pairing", "-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh_CN"]
        app.launch()
        let replace = app.buttons["login.succession"]
        XCTAssertTrue(replace.waitForExistence(timeout: 5))
        for _ in 0..<4 where !replace.isHittable { app.swipeUp() }
        replace.tap()
        let fields = app.textFields.matching(identifier: "succession.predecessor")
        let views = app.textViews.matching(identifier: "succession.predecessor")
        XCTAssertTrue(fields.firstMatch.waitForExistence(timeout: 5) || views.firstMatch.exists)
        let field = fields.firstMatch.exists ? fields.firstMatch : views.firstMatch
        let create = app.buttons["succession.create"]
        XCTAssertFalse(create.isEnabled)
        field.tap()
        field.typeText(String(repeating: "7", count: 64))
        wait(for: [expectation(for: NSPredicate(format: "isEnabled == true"), evaluatedWith: create)], timeout: 5)
        capture(app, name: "Chinese replace a device request")
    }

    func testDesktopInvitationEntryAndPasteInChineseGermanAndArabic() {
        for (language, locale) in [("zh-Hans", "zh_CN"), ("de", "de_DE"), ("ar", "ar")] {
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "--ui-access", "--ui-pairing", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
            if language == "ar" { app.launchArguments.append("--ui-dark") }
            app.launch()
            let authorize = app.buttons["login.authorize"]
            XCTAssertTrue(authorize.waitForExistence(timeout: 5))
            for _ in 0..<4 where !authorize.isHittable { app.swipeUp() }
            authorize.tap()
            let desktop = app.buttons["pair.desktop"]
            XCTAssertTrue(desktop.waitForExistence(timeout: 5))
            desktop.tap()
            let fields = app.textFields.matching(identifier: "pair.desktop.invitation")
            let views = app.textViews.matching(identifier: "pair.desktop.invitation")
            XCTAssertTrue(fields.firstMatch.waitForExistence(timeout: 5) || views.firstMatch.exists)
            let field = fields.firstMatch.exists ? fields.firstMatch : views.firstMatch
            let send = app.buttons["pair.desktop.send"]
            XCTAssertFalse(send.isEnabled)
            capture(app, name: "Desktop invitation empty · \(language)")
            field.tap()
            field.typeText("dmsg-device-invite/2:" + String(repeating: "a", count: 180))
            XCTAssertTrue(send.isEnabled)
            XCTAssertTrue((field.value as? String)?.hasPrefix("dmsg-device-invite/2:") == true)
            capture(app, name: "Desktop invitation paste · \(language)")
            XCTAssertFalse(app.staticTexts["pair.desktop.device"].exists, "Typing an invitation does not authorize a device")
            app.terminate()
        }
    }
}
