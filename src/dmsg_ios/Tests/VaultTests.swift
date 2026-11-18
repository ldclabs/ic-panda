import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class VaultTests: XCTestCase {
    func testVaultImportRetryBelongsOnlyToTheCurrentStagedFile() async throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: base) }
        let paths = try PrivateFiles(base: base), account = "040g2081040g2081040g"
        let csv = base.appending(path: "first.csv")
        try Data("name,username\nWork,user".utf8).write(to: csv)
        func stage(_ url: URL) throws -> String {
            let copy = try paths.importDocument(url, account: account, purpose: "vault")
            return account + "/" + copy.deletingLastPathComponent().lastPathComponent + "/" + copy.lastPathComponent
        }
        let review = VaultImportReview()
        let preview = VaultImportReview.Preview(format: "csv", digest: "digest", skipped: 0, incomplete: 0,
            items: [.init(index: 0, title: "Work", username: "", duplicate: false)])
        try await review.inspect(stage: { try stage(csv) }, read: { _ in preview })
        let saved = try XCTUnwrap(paths.pending(account: account).first)
        do {
            try await review.inspect(stage: { try stage(base.appending(path: "missing.csv")) }) { _ in
                XCTFail("A failed copy has no source to preview"); return preview
            }
            XCTFail("Expected staging failure")
        } catch { XCTAssertTrue(error is CocoaError) }
        guard case .needsCopy = review.phase else { return XCTFail("The first file must not be offered for retry") }
        XCTAssertEqual(try paths.pending(account: account).map(\.handle), [saved.handle])
        do {
            try await review.inspect(stage: { saved.handle }, read: { _ in throw CocoaError(.fileReadUnknown) })
            XCTFail("Expected preview failure")
        } catch let error as CocoaError { XCTAssertEqual(error.code, .fileReadUnknown) }
        guard case let .staged(handle) = review.phase else { return XCTFail("The latest staged file should be retryable") }
        XCTAssertEqual(handle, saved.handle)
        try await review.inspect(stage: { handle }) { source in XCTAssertEqual(source, saved.handle); return preview }
        review.finish(ids: ["item"], picks: [0])
        guard case let .imported(ids, titles) = review.phase else { return XCTFail("Expected the committed records") }
        XCTAssertEqual(ids, ["item"]); XCTAssertEqual(titles["item"], "Work")
    }

    func testPickerIgnoresThePreviousPageWhenReturningToTheSameSearch() async {
        let search = VaultPickerSearch()
        let current = VaultPage(generation: 1, items: [], hasMore: false)
        var pending: CheckedContinuation<VaultPage, Error>?
        let old = Task { await search.load { _, _ in try await withCheckedThrowingContinuation { pending = $0 } } }
        while pending == nil { await Task.yield() }
        search.query = "Work"; search.query = ""
        await search.load { _, offset in XCTAssertEqual(offset, 0); return current }
        pending?.resume(throwing: CocoaError(.fileReadUnknown))
        await old.value
        XCTAssertEqual(search.phase, .ready(hasMore: false))
    }

    func testPickerRetriesOnlyTheFailedPageAndClearsItemsOnNewInput() async throws {
        let model = SessionModel(); model.loadFixtures()
        let first = try XCTUnwrap(model.vault.first), next = try XCTUnwrap(model.vault.dropFirst().first)
        let search = VaultPickerSearch()
        await search.load { _, offset in
            XCTAssertEqual(offset, 0); return VaultPage(generation: 1, items: [first], hasMore: true)
        }
        await search.load { _, offset in
            XCTAssertEqual(offset, 1)
            await search.load { _, _ in XCTFail("Only one read may own this page"); return VaultPage(generation: 1, items: [], hasMore: false) }
            throw MobileError.Failure(code: "INTEGRITY_FAILED")
        }
        XCTAssertEqual(search.failure, InboxLoading.message(MobileError.Failure(code: "INTEGRITY_FAILED")))
        XCTAssertEqual(search.items, [first])
        await search.load { _, offset in
            XCTAssertEqual(offset, 1); return VaultPage(generation: 1, items: [next], hasMore: false)
        }
        XCTAssertEqual(search.phase, .ready(hasMore: false)); XCTAssertEqual(search.items, [first, next])
        search.query = "different"; XCTAssertEqual(search.phase, .waiting); XCTAssertTrue(search.items.isEmpty)
    }

    func testWebsiteActionsOnlyOpenWebURLsAndKeepTheOriginalAddress() {
        func field(_ value: String, kind: String = "url", concealed: Bool = false) -> VaultField {
            VaultField(id: "url", kind: kind, label: "", section: "", value: value, concealed: concealed)
        }
        let address = "https://example.org/path?token=a%2Fb#section"
        XCTAssertEqual(vaultWebsiteURL(field(address))?.absoluteString, address)
        XCTAssertNotNil(vaultWebsiteURL(field("http://example.org")))
        for value in ["javascript:alert(1)", "file:///private/data", "dmsg://connect/request", "https:", "example.org"] {
            XCTAssertNil(vaultWebsiteURL(field(value)), value)
        }
        XCTAssertNil(vaultWebsiteURL(field(address, kind: "text")))
        XCTAssertNil(vaultWebsiteURL(field(address, concealed: true)))
        XCTAssertTrue(vaultFieldIsLTR(field("token", kind: "concealed", concealed: true), value: "token"))
    }

    func testChangingTypeRetainsValuesLabelsGroupsAndCustomFields() {
        var draft = VaultDraft(target: VaultEditorTarget(item: nil))
        draft.fields[0].value = "person@example.org"
        draft.fields[0].label = "Work account"; draft.fields[0].section = "Work"
        draft.fields.append(.init(id: "field_custom", kind: "text", label: "Project", section: "Work", value: "Shipping", concealed: false, changed: true))
        draft.kind = "ssh"; draft.addTemplate(from: "login")
        if let key = draft.fields.firstIndex(where: { $0.id == "private_key" }) { draft.fields[key].value = "key draft" }
        draft.kind = "login"; draft.addTemplate(from: "ssh")
        XCTAssertEqual(draft.fields.filter { $0.id == "username" }.count, 1)
        XCTAssertEqual(draft.fields.first { $0.id == "username" }?.value, "person@example.org")
        XCTAssertEqual(draft.fields.first { $0.id == "username" }?.section, "Work")
        XCTAssertEqual(draft.fields.first { $0.id == "field_custom" }?.value, "Shipping")
        XCTAssertTrue(draft.fields.contains { $0.id == "private_key" })
    }

    func testChangingAnUntouchedNewItemReplacesEmptyTemplateFields() {
        var draft = VaultDraft(target: VaultEditorTarget(item: nil))
        draft.kind = "note"; draft.addTemplate(from: "login")
        XCTAssertTrue(draft.fields.isEmpty)
        draft.kind = "api"; draft.addTemplate(from: "note")
        XCTAssertEqual(draft.fields.map(\.id), ["username", "credential", "url", "expires"])
    }

    func testLoginWithAFieldFromAnotherTemplateUsesTheFullEditor() throws {
        var draft = VaultDraft(target: VaultEditorTarget(item: nil))
        XCTAssertTrue(draft.isSimpleLogin)
        draft.kind = "ssh"; draft.addTemplate(from: "login")
        let key = try XCTUnwrap(draft.fields.firstIndex { $0.id == "private_key" })
        draft.fields[key].value = "key draft"
        draft.kind = "login"; draft.addTemplate(from: "ssh")
        XCTAssertEqual(draft.fields.first { $0.id == "private_key" }?.value, "key draft")
        XCTAssertFalse(draft.isSimpleLogin)
    }

    func testOTPExpiresIndependentlyOfOtherDisplayedFields() {
        let now = Date(timeIntervalSince1970: 1000)
        var shown = ["otp": RevealedVaultValue(text: "123456", expires: now.addingTimeInterval(2))]
        shown["password"] = RevealedVaultValue(text: "new field", expires: nil)
        XCTAssertEqual(shown["otp"]?.value(at: now), "123456")
        XCTAssertNil(shown["otp"]?.value(at: now.addingTimeInterval(2)))
        XCTAssertEqual(shown["password"]?.value(at: now.addingTimeInterval(10)), "new field")
    }

    func testFieldValidationDistinguishesKeptValuesAndInvalidNewValues() {
        var field = VaultDraft.Field(id: "expires", kind: "date", label: "", section: "", value: "2025-02-29", concealed: false, changed: true)
        XCTAssertNotNil(vaultInputError(field, existing: false))
        field.value = "2024-02-29"; XCTAssertNil(vaultInputError(field, existing: false))
        field.kind = "month"; field.value = "2026-13"; XCTAssertNotNil(vaultInputError(field, existing: false))
        field.kind = "otp"; field.value = "not a seed!"; XCTAssertNotNil(vaultInputError(field, existing: false))
        field.value = "JBSWY3DPEHPK3PXP"; XCTAssertNil(vaultInputError(field, existing: false))
        field.value = "old unsupported value"; field.changed = false; XCTAssertNil(vaultInputError(field, existing: true))
    }

    func testGeneratedPasswordsMeetSelectedLengthAndCharacterGroups() {
        var generated: Set<String> = []
        for _ in 0..<20 {
            let value = makeVaultPassword(length: 24, symbols: true)
            XCTAssertEqual(value.count, 24)
            XCTAssertTrue(value.contains(where: \.isUppercase)); XCTAssertTrue(value.contains(where: \.isLowercase))
            XCTAssertTrue(value.contains(where: \.isNumber)); XCTAssertTrue(value.contains { !$0.isLetter && !$0.isNumber })
            generated.insert(value)
        }
        XCTAssertEqual(generated.count, 20)
        XCTAssertTrue(makeVaultPassword(length: 12, symbols: false).allSatisfy { $0.isLetter || $0.isNumber })
    }

    func testPresentationDataFiltersTheFullInventoryBeforePaging() {
        let model = SessionModel()
        model.fixtureVaultItems = (0..<130).map { index in
            vaultFixtureItem(id: "item-\(index)", revision: "1", kind: index % 2 == 0 ? "login" : "note", title: String(format: "Item %03d", index), subtitle: "", notes: "",
                favorite: index < 9, tags: [index % 2 == 0 ? "work" : "personal"], synced: true, conflict: false, reserved: false, files: [], fields: [])
        }
        model.vaultSort = "title"
        XCTAssertEqual(model.fixtureVaultPage(offset: 60, limit: 60).items.first?.title, "Item 060")
        model.vaultFilter = "favorites"
        XCTAssertEqual(model.fixtureVaultPage(offset: 0, limit: 60).items.count, 9)
        model.vaultTag = "work"
        XCTAssertEqual(model.fixtureVaultPage(offset: 0, limit: 60).items.count, 5)
        model.search = "008"
        XCTAssertEqual(model.fixtureVaultPage(offset: 0, limit: 60).items.map(\.title), ["Item 008"])
    }
}
