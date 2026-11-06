import XCTest

extension XCUIElement {
    /// XCTest's own wait looks only once a second, so each one costs at least that even
    /// when the element is already there. This one shadows it for every test here.
    func waitForExistence(timeout: TimeInterval) -> Bool {
        poll(timeout) { self.exists }
    }

    func waitForNonExistence(timeout: TimeInterval) -> Bool {
        poll(timeout) { !self.exists }
    }
}

extension XCUIApplication {
    /// Comes back from the Home Screen. For a moment the system keeps a redacted copy
    /// of the screen beside the real one, so `query` has two matches until it goes.
    func returnToForeground(showing query: XCUIElementQuery) {
        activate()
        XCTAssertTrue(poll(8) { query.count == 1 })
    }
}

private func poll(_ timeout: TimeInterval, until met: () -> Bool) -> Bool {
    let deadline = Date.now.addingTimeInterval(timeout)
    while !met() {
        guard Date.now < deadline else { return false }
        RunLoop.current.run(until: .now.addingTimeInterval(0.1))
    }
    return true
}
