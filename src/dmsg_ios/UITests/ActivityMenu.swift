import XCTest

extension XCUIApplication {
    /// Approvals, reminders and sync status share one toolbar menu.
    @MainActor func openActivity(_ item: String) {
        let menu = buttons["activity.open"].firstMatch
        XCTAssertTrue(menu.waitForExistence(timeout: 10))
        menu.tap()
        let entry = buttons[item].firstMatch
        XCTAssertTrue(entry.waitForExistence(timeout: 5), item)
        entry.tap()
    }
}
