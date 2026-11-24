import XCTest

@MainActor
final class SearchExperienceUITests: XCTestCase {
    private struct Look {
        let language: String
        let locale: String
        let dark: Bool
        let large: Bool
        var allScopeTitle: String { ["zh-Hans": "全部", "ar": "الكل"][language] ?? "All" }
        var name: String { "\(language)-\(dark ? "dark" : "light")\(large ? "-ax" : "")" }
        var arguments: [String] {
            ["--ui-testing", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
                + (dark ? ["--ui-dark"] : [])
                + (large ? ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityL"] : [])
        }
    }

    private let looks = [
        Look(language: "zh-Hans", locale: "zh_CN", dark: false, large: false),
        Look(language: "zh-Hans", locale: "zh_CN", dark: true, large: false),
        Look(language: "ar", locale: "ar_SA", dark: false, large: false),
        Look(language: "zh-Hans", locale: "zh_CN", dark: false, large: true),
    ]

    override func setUp() { continueAfterFailure = false }

    func testVaultSearchScopesAndVisualChecks() {
        for look in looks {
            let app = XCUIApplication()
            app.launchArguments = look.arguments
            app.launch()
            let item = app.buttons["vault.item.fixture-github"]
            XCTAssertTrue(item.waitForExistence(timeout: 10), look.name)
            let field = app.searchFields.firstMatch
            XCTAssertTrue(field.waitForExistence(timeout: 5), look.name)
            field.tap()
            field.typeText("GitHub")
            Thread.sleep(forTimeInterval: 0.6)
            attach(app, "search-vault-\(look.name)")
            let allScope = app.segmentedControls.buttons[look.allScopeTitle].firstMatch
            XCTAssertTrue(allScope.waitForExistence(timeout: 3), look.name)
            allScope.tap()
            XCTAssertTrue(app.buttons["global.vault.fixture-github"].waitForExistence(timeout: 5), look.name)
            attach(app, "search-global-\(look.name)")
            app.terminate()
        }
    }

    private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
