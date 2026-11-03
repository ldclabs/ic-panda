import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

final class AccountSettingsTests: XCTestCase {
    func testMoneyKeepsAllDigitsAndRejectsRoundingOverflowAndExponentNotation() {
        XCTAssertEqual(AtomicAmount.parse("9007199254740993.00000001", decimals: 8, separator: "."), "900719925474099300000001")
        XCTAssertEqual(AtomicAmount.display("900719925474099300000001", decimals: 8), "9007199254740993.00000001")
        XCTAssertEqual(AtomicAmount.parse("١٢٫٣٤", decimals: 2, separator: "٫"), "1234")
        XCTAssertEqual(AtomicAmount.parse("1,25", decimals: 8, separator: ","), "125000000")
        XCTAssertNil(AtomicAmount.parse("1.000000001", decimals: 8, separator: "."))
        XCTAssertNil(AtomicAmount.parse("1e8", decimals: 8))
        XCTAssertNil(AtomicAmount.parse("-1", decimals: 8))
        XCTAssertNil(AtomicAmount.parse("340282366920938463463374607431768211456", decimals: 0))
        XCTAssertEqual(AtomicAmount.parse("340282366920938463463374607431768211455", decimals: 0), "340282366920938463463374607431768211455")
    }

    func testPublicProfileRequiresNamedHttpsLinksAndEnforcesPublishedLimits() {
        var draft = ProfileDraft(name: "Name", about: "About", links: [PublicProfileLink(name: "Website", url: "https://example.com")])
        XCTAssertTrue(draft.valid)
        draft.links[0].url = "javascript:alert(1)"; XCTAssertFalse(draft.valid)
        draft.links[0].url = "https://example.com"; draft.links[0].name = " "; XCTAssertFalse(draft.valid)
        draft.links = []; draft.name = String(repeating: "界", count: 81); XCTAssertFalse(draft.valid)
        draft.name = ""; draft.about = String(repeating: "a", count: 2001); XCTAssertFalse(draft.valid)
    }

    func testDelegationFormRejectsWildcardsDuplicatesPathsAndOversizedScopes() {
        XCTAssertTrue(AgentAuthorityDraft(scopes: "message.read\nmessage.write", audiences: "https://example.com").valid)
        for scopes in ["*", "message. read", "read\nread", String(repeating: "x", count: 65)] {
            XCTAssertFalse(AgentAuthorityDraft(scopes: scopes, audiences: "https://example.com").valid)
        }
        for origin in ["http://example.com", "https://example.com/private", "https://example.com?token=1"] {
            XCTAssertFalse(AgentAuthorityDraft(scopes: "read", audiences: origin).valid)
        }
    }

    func testProfileDraftRetainsExpectedVersionAndOnlyEncodesPublicLinkFields() throws {
        let draft = ProfileDraft(name: "Draft", links: [PublicProfileLink(name: "Website", url: "https://example.com")])
        let record = ProfileDraftRecord(value: draft, expectedVersion: 17)
        let restored = try JSONDecoder().decode(ProfileDraftRecord.self, from: JSONEncoder().encode(record))
        XCTAssertEqual(restored.expectedVersion, 17)
        XCTAssertEqual(restored.value, draft)
        let wire = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(draft.links[0])) as? [String: Any])
        XCTAssertEqual(Set(wire.keys), ["name", "url", "rel"])
    }

    func testRecoveryStateDecodesCoreDeadlinesWhileLocked() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let value = #"{"enabled":true,"ready":false,"member":false,"recovery_delay_ms":259200000,"pending":{"execute_after":1800000000000,"expires_at":1800100000000,"this_device":true}}"#
        let state = try decoder.decode(RecoveryView.RecoveryState.self, from: Data(value.utf8))
        XCTAssertEqual(state.recoveryDelayMs, 259200000)
        XCTAssertEqual(state.pending?.executeAfter, 1800000000000)
        XCTAssertEqual(state.pending?.thisDevice, true)
        XCTAssertFalse(state.canComplete)
    }

    #if DEBUG
    @MainActor func testAccountNamesFillOlderAccountsPreserveOwnProfileAndIgnoreLateReads() async {
        let current = "040g2081040g2081040g", other = "040g2082040g2082040g"
        let model = SessionModel()
        model.installFixtures(SessionSnapshot(generation: 1, sequence: 1, state: "unlocked", account: current, deviceId: "device", pendingSync: 0, conflicts: 0, approvalsWaiting: 0, fastUnlock: false, setupPending: false, lastSyncAtMs: nil, syncError: nil), items: [])
        let epoch = model.epoch
        model.rememberIdentity(name: "New profile name", avatar: nil)
        model.rememberAccountNames([current: "Older public name", other: "  Personal  "], epoch: epoch)
        XCTAssertEqual(model.accountIdentities[current]?.name, "New profile name")
        XCTAssertEqual(model.accountIdentities[other]?.name, "Personal")
        model.rememberAccountNames([other: "Renamed"], epoch: epoch)
        XCTAssertEqual(model.accountIdentities[other]?.name, "Renamed")
        model.rememberAccountNames([other: nil], epoch: epoch)
        XCTAssertNil(model.accountIdentities[other]?.name)
        await model.lock()
        model.rememberAccountNames([other: "Late result"], epoch: epoch)
        XCTAssertNil(model.accountIdentities[other]?.name)
    }

    @MainActor func testSettingsDraftsAreClearedOnLockAndLateWritesAreIgnored() async {
        let model = SessionModel()
        model.installFixtures(SessionSnapshot(generation: 1, sequence: 1, state: "unlocked", account: "040g2081040g2081040g", deviceId: "device", pendingSync: 0, conflicts: 0, approvalsWaiting: 0, fastUnlock: false, setupPending: false, lastSyncAtMs: nil, syncError: nil), items: [])
        let epoch = model.epoch
        model.keepFormDraft("profile", value: ProfileDraft(name: "Draft"), epoch: epoch)
        XCTAssertNotNil(model.formDraft("profile", as: ProfileDraft.self))
        await model.lock()
        XCTAssertNil(model.formDraft("profile", as: ProfileDraft.self))
        model.keepFormDraft("profile", value: ProfileDraft(name: "Late result"), epoch: epoch)
        XCTAssertNil(model.formDraft("profile", as: ProfileDraft.self))
    }
    #endif

    func testIncompleteUsageIsNotPresentedAsZeroAndRemainingCannotUnderflow() throws {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let result = try decoder.decode(SubscriptionView.Usage.self, from: Data(#"{"cloud":null,"execution":null,"cloud_error":"UNAVAILABLE","execution_error":"AUTH_REQUIRED"}"#.utf8))
        XCTAssertNil(result.cloud); XCTAssertNil(result.execution)
        let full = SubscriptionView.Usage.Cloud(committed: 100, reserved: 50, storageLimit: 120, activeChannels: nil, channelLimit: nil)
        XCTAssertEqual(full.remaining, 0)
    }

    func testInboxRejectsDuplicatesAndInvalidAccountRows() {
        let account = "040g2081040g2081040g"
        XCTAssertTrue(InboxSettingsView.Change(mode: "contacts", allow: [account]).valid)
        XCTAssertFalse(InboxSettingsView.Change(mode: "contacts", allow: [account, account]).valid)
        XCTAssertFalse(InboxSettingsView.Change(mode: "contacts", allow: [""]).valid)
    }
}

extension SystemExportsTests {
    func testSnapshotStatusKeepsLegacyHeadersReadableAndReportsNewMetadata() throws {
        let legacy = Data(#"{"id":"794D36D7-3244-42F2-BE9F-203F27AC23DA","purpose":"credentials","accountHash":"one","expires":1}"#.utf8)
        let old = try JSONDecoder().decode(ExportHeader.self, from: legacy)
        XCTAssertNil(old.createdAt); XCTAssertNil(old.count)
        let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = try SystemExports(.credentials, root: directory, keyGroup: "test")
        let prepared = try store.prepareCredentials([CredentialInput(title: "One", username: "user", password: "secret", origin: "https://example.com")], account: "one", expires: UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000)
        XCTAssertEqual(prepared.header.count, 1)
        XCTAssertNotNil(prepared.header.createdAt)
        XCTAssertLessThan(prepared.header.createdAt!, prepared.header.expires)
        store.discard(prepared)
    }
}
