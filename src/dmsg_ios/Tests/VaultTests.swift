import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class VaultTests: XCTestCase {
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
