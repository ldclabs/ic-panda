import XCTest

@MainActor
final class MessagingUITests: XCTestCase {
    private func start(_ args: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-inbox", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"] + args
        app.launch(); return app
    }
    private func field(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        let view = app.textViews[id]; return view.exists ? view : app.textFields[id]
    }
    private func capture(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot()); attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    private func letters(_ app: XCUIApplication) {
        let tab = app.buttons["messages.mode.letters"]
        XCTAssertTrue(tab.waitForExistence(timeout: 10)); tab.tap()
    }
    private func folder(_ app: XCUIApplication, _ label: String) {
        app.buttons["letters.folder"].tap(); app.buttons[label].tap()
    }
    func testDraftReturnsAndSuccessfulLetterRemainsInSent() {
        let app = start(); letters(app)
        app.buttons["letters.compose"].tap()
        let recipient = field(app, "message.recipient"); XCTAssertTrue(recipient.waitForExistence(timeout: 5))
        recipient.tap(); recipient.typeText("040g2082040g2082040g")
        let text = field(app, "letter.text"); text.tap(); text.typeText("Keep this letter draft")
        app.navigationBars.buttons["Done"].tap()
        folder(app, "Drafts")
        let row = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "letter.draft.")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        XCTAssertEqual(field(app, "letter.text").value as? String, "Keep this letter draft")
        let send = app.buttons["letter.send"]
        XCTAssertTrue(send.isEnabled); send.tap()
        XCTAssertTrue(app.staticTexts["Sent"].waitForExistence(timeout: 5))
        capture(app, "Letter acceptance retains recipient and body")
        app.navigationBars.buttons["Done"].tap(); folder(app, "Sent letters")
        let sent = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "letter.sent.")).firstMatch
        XCTAssertTrue(sent.waitForExistence(timeout: 5)); sent.tap()
        XCTAssertTrue(app.staticTexts["Keep this letter draft"].waitForExistence(timeout: 5))
        capture(app, "Sent letter can be reopened")
    }

    func testPartialRecipientIsValidatedAfterLeavingTheField() {
        let app = start()
        letters(app)
        app.buttons["letters.compose"].tap()
        let recipient = field(app, "message.recipient")
        recipient.tap(); recipient.typeText("040")
        let invalid = app.staticTexts["Enter a valid account ID."]
        XCTAssertFalse(invalid.exists)
        capture(app, "Partial recipient while editing")
        field(app, "letter.text").tap()
        XCTAssertTrue(invalid.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["letter.send"].isEnabled)
        capture(app, "Incomplete recipient after leaving the field")
    }
    func testSelectingAnAttachmentDoesNotSendIt() {
        let app = start(["--ui-channel", "--ui-detail-navigation"])
        let channel = app.buttons["channel.row.fixture-channel"]; XCTAssertTrue(channel.waitForExistence(timeout: 10)); channel.tap()
        app.buttons["Attach"].tap(); app.buttons["Attach file"].tap()
        let file = app.buttons["attachment.choose.fixture-local"]; XCTAssertTrue(file.waitForExistence(timeout: 5)); file.tap()
        XCTAssertTrue(app.descendants(matching: .any)["message.attachment.draft"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["message.send"].isEnabled)
        XCTAssertEqual(app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@", "message.pending.")).count, 0)
        let message = app.staticTexts["Hello channel · 频道消息已显示"]
        XCTAssertTrue(message.waitForExistence(timeout: 5))
        message.press(forDuration: 1); app.buttons["Reply"].tap()
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(channel.waitForExistence(timeout: 5)); channel.tap()
        XCTAssertTrue(app.descendants(matching: .any)["message.attachment.draft"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Cancel reply"].exists)
        capture(app, "Reopened conversation keeps its attachment and reply until Send")
        app.buttons["message.send"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["message.attachment.draft"].waitForNonExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Cancel reply"].exists)
    }
    func testSnapshotPickerKeepsResultsScopedToTheCurrentSearch() {
        let app = start(["--ui-channel", "--ui-snapshot-search-offline"])
        let channel = app.buttons["channel.row.fixture-channel"]
        XCTAssertTrue(channel.waitForExistence(timeout: 8)); channel.tap()
        app.buttons["Attach"].tap(); app.buttons["Share secret snapshot"].tap()
        let github = app.buttons["secret.picker.fixture-github"]
        XCTAssertTrue(github.waitForExistence(timeout: 5))
        let field = app.searchFields.firstMatch
        if !field.isHittable { app.swipeDown() }
        field.tap(); field.typeText("x")
        XCTAssertFalse(github.exists, "The old result must disappear while the new read is in flight")
        let retry = app.buttons["Retry"].firstMatch
        XCTAssertTrue(retry.waitForExistence(timeout: 5), "A failed search must offer an inline retry")
        XCTAssertFalse(github.exists, "A result from the previous search must no longer be selectable")
        guard retry.exists else { return }
        field.buttons["Clear text"].tap()
        field.typeText("Work"); XCTAssertEqual(field.value as? String, "Work")
        let mail = app.buttons["secret.picker.fixture-mail"]
        XCTAssertTrue(mail.waitForExistence(timeout: 5)); XCTAssertFalse(github.exists)
        mail.tap()
        XCTAssertTrue(app.descendants(matching: .any).matching(identifier: "secret.snapshot.destination").firstMatch.waitForExistence(timeout: 5))
    }

    func testChangingInviteeClearsTheOldShareLink() {
        let app = start(["--ui-channel"])
        let channel = app.buttons["channel.row.fixture-channel"]; XCTAssertTrue(channel.waitForExistence(timeout: 10)); channel.tap()
        app.buttons["conversation.more"].tap(); app.buttons["Invite member"].tap()
        let account = field(app, "message.recipient"); account.tap(); account.typeText("040g2082040g2082040g")
        app.buttons["channel.invitation.share"].tap()
        let ready = app.descendants(matching: .any)["channel.invitation.ready"]; XCTAssertTrue(ready.waitForExistence(timeout: 5))
        account.tap(); account.typeText(XCUIKeyboardKey.delete.rawValue)
        XCTAssertFalse(ready.exists)
    }
    func testReadOnlyConversationKeepsTheComposerDisabled() {
        let app = start(["--ui-channel", "--ui-channel-readonly"])
        let channel = app.buttons["channel.row.fixture-channel"]; XCTAssertTrue(channel.waitForExistence(timeout: 10)); channel.tap()
        XCTAssertTrue(app.staticTexts["You have read-only access to this conversation."].waitForExistence(timeout: 5))
        // The send button appears only once there is something to send.
        XCTAssertFalse(app.buttons["message.send"].exists)
        app.buttons["conversation.more"].tap()
        XCTAssertTrue(app.buttons["Conversation settings"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Invite member"].exists)
        capture(app, "Read only conversation explains why sending is unavailable")
    }
    func testWorkspaceSearchFindsOlderMessagesAndAttachmentsByChannel() {
        let app = start(["--ui-search-workspace"])
        let search = app.searchFields.firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: 5)); search.tap(); search.typeText("roadmap\n")
        let file = app.buttons["messages.hit.fixture-channel.1"], message = app.buttons["messages.hit.fixture-research.1"]
        XCTAssertTrue(file.waitForExistence(timeout: 5)); XCTAssertTrue(message.waitForExistence(timeout: 5))
        for sequence in 2...5 { XCTAssertFalse(app.buttons["messages.hit.fixture-channel.\(sequence)"].exists) }
        capture(app, "Workspace search groups the filename and earlier text")
        message.tap()
        XCTAssertTrue(app.staticTexts["Original roadmap discussion"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["Meeting notes"].exists)
        app.buttons["conversation.latest"].tap()
        XCTAssertTrue(app.staticTexts["Meeting notes"].waitForExistence(timeout: 5))
    }

    func testAttachmentNameSearchKeepsTheMatchingMessage() {
        let app = start(["--ui-channel", "--ui-channel-attachment"])
        let channel = app.buttons["channel.row.fixture-channel"]; XCTAssertTrue(channel.waitForExistence(timeout: 10)); channel.tap()
        app.buttons["conversation.more"].tap(); app.buttons["Search messages"].tap()
        let search = app.textFields["conversation.search"]; XCTAssertTrue(search.waitForExistence(timeout: 5)); search.tap(); search.typeText("plan.pdf")
        XCTAssertTrue(app.buttons.containing(NSPredicate(format: "label CONTAINS %@", "plan.pdf")).firstMatch.waitForExistence(timeout: 5))
        capture(app, "Search matches an attachment filename")
    }

    func testSavedAttachmentRefreshesAfterEvictionAndRestore() {
        let app = start(["--ui-channel", "--ui-channel-attachment", "--ui-channel-saved-attachment", "--ui-files-rich"])
        let channel = app.buttons["channel.row.fixture-channel"]
        XCTAssertTrue(channel.waitForExistence(timeout: 10)); channel.tap()
        let attachment = app.buttons.containing(NSPredicate(format: "label CONTAINS %@", "plan.pdf")).firstMatch
        XCTAssertTrue(attachment.waitForExistence(timeout: 5)); attachment.tap()
        let evict = app.buttons["file.evict"]
        XCTAssertTrue(evict.waitForExistence(timeout: 5)); evict.tap()
        app.sheets.buttons["Remove local copy"].tap()
        let restore = app.buttons["file.restore"]
        XCTAssertTrue(restore.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Preview file"].isEnabled)
        restore.tap()
        let preview = app.buttons["Preview file"]
        expectation(for: NSPredicate(format: "enabled == true"), evaluatedWith: preview)
        waitForExpectations(timeout: 5)
        capture(app, "Conversation attachment reflects its restored local version")
    }

    func testReplyPreviewDoesNotRevealAHiddenMessage() {
        let app = start(["--ui-channel", "--ui-channel-hidden-reply"])
        let channel = app.buttons["channel.row.fixture-channel"]
        XCTAssertTrue(channel.waitForExistence(timeout: 10)); channel.tap()
        let reply = app.buttons["Open original message"]
        XCTAssertTrue(reply.waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts.matching(identifier: "A message you hid").count, 2)
        XCTAssertFalse(app.staticTexts["Hidden original text"].exists)
        capture(app, "Reply preview preserves local message hiding")
    }
}

@MainActor
final class MessagingLayoutUITests: XCTestCase {
    private func inspect(language: String, locale: String, dark: Bool, large: Bool = false) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-inbox", "-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        if dark { app.launchArguments.append("--ui-dark") }
        if large { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch()
        let letters = app.buttons["messages.mode.letters"]
        if large {
            // Accessibility text sizes offer the two scopes as a menu.
            let scopes = app.buttons["messages.mode.conversations"]
            XCTAssertTrue(scopes.waitForExistence(timeout: 10)); scopes.tap()
            app.buttons["Lettres"].tap()
            XCTAssertTrue(letters.waitForExistence(timeout: 5))
        } else {
            XCTAssertTrue(letters.waitForExistence(timeout: 10))
            XCTAssertTrue(app.searchFields.firstMatch.exists)
            letters.tap()
            let selection = XCTNSPredicateExpectation(predicate: NSPredicate(format: "selected == true"), object: letters)
            if XCTWaiter.wait(for: [selection], timeout: 2) != .completed { letters.tap() }
            XCTAssertTrue(letters.isSelected)
        }
        let row = app.buttons["inbox.row.fixture-letter"]
        for _ in 0..<4 {
            if row.exists && row.isHittable { break }
            app.collectionViews.firstMatch.swipeUp()
        }
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        capture(app, "Letters · \(language) · \(large ? "accessibility" : "standard")")
        row.tap()
        let detail = app.collectionViews["inbox.detail"]
        XCTAssertTrue(detail.waitForExistence(timeout: 5))
        // Lists materialize offscreen rows lazily, especially at AX text sizes.
        for _ in 0..<4 {
            if app.buttons["inbox.reply"].exists && app.buttons["inbox.reply"].isHittable { break }
            detail.swipeUp()
        }
        // The tab bar floats over the list: a button under it still counts as hittable.
        detail.swipeUp()
        XCTAssertTrue(app.buttons["inbox.reply"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["inbox.reply"].isHittable)
        capture(app, "Letter detail · \(language)")
        app.buttons["inbox.reply"].tap()
        XCTAssertTrue(app.textFields["message.recipient"].waitForExistence(timeout: 5))
        capture(app, "Reply keeps the original sender · \(language)")
    }
    private func capture(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
    func testChineseDark() { inspect(language: "zh-Hans", locale: "zh_CN", dark: true) }
    func testArabicRTL() { inspect(language: "ar", locale: "ar_SA", dark: false) }
    func testFrenchLargeText() { inspect(language: "fr", locale: "fr_FR", dark: false, large: true) }
}
