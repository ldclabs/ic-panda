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
        let value = #"{"enabled":true,"ready":false,"member":false,"recovery_delay_ms":259200000,"pending":{"op_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","execute_after":1800000000000,"expires_at":1800100000000,"this_device":true}}"#
        let state = try decoder.decode(RecoveryView.RecoveryState.self, from: Data(value.utf8))
        XCTAssertEqual(state.recoveryDelayMs, 259200000)
        XCTAssertEqual(state.pending?.executeAfter, 1800000000000)
        XCTAssertEqual(state.pending?.thisDevice, true)
        XCTAssertFalse(state.canComplete(at: Date(timeIntervalSince1970: 1_799_000_000)))
    }

    @MainActor func testApprovalReceiptsReuseARequestUntilItEndsLikeTheDesktop() async throws {
        let receipts = ApprovalReceipts()
        var created = 0
        func make() -> ApprovalView {
            created += 1
            return ApprovalView(id: "request-\(created)", digest: "digest", type: "account", state: "awaiting_user", source: "gui",
                                risk: "high", expiresAt: 0, mobileSupported: true, updatedAt: nil)
        }
        let first = try await receipts.request("recovery.cancel", generation: 1) { make() }
        // Asking again shows the same request instead of creating another.
        let again = try await receipts.request("recovery.cancel", generation: 1) { make() }
        XCTAssertEqual(again.id, first.id); XCTAssertEqual(created, 1)
        _ = try await receipts.request("hardware.policy", generation: 1) { make() }
        XCTAssertEqual(created, 2)
        // Waiting, running and unreadable requests stay kept.
        for state: String? in ["awaiting_user", "executing", "unknown", nil] {
            await receipts.settle(generation: 1) { _ in state }
            XCTAssertEqual(receipts["recovery.cancel"]?.id, first.id)
        }
        // A final state clears only that request's receipt.
        await receipts.settle(generation: 1) { $0 == first.id ? "rejected" : "awaiting_user" }
        XCTAssertNil(receipts["recovery.cancel"]); XCTAssertNotNil(receipts["hardware.policy"])
        // Locking or switching accounts starts empty.
        await receipts.settle(generation: 2) { _ in "awaiting_user" }
        XCTAssertNil(receipts["hardware.policy"])
        for state in ApprovalReceipts.finalStates {
            _ = try await receipts.request("device.revoke", generation: 2) { make() }
            await receipts.settle(generation: 2) { _ in state }
            XCTAssertNil(receipts["device.revoke"], state)
        }
        // A failed creation keeps nothing.
        struct Failure: Error {}
        do { _ = try await receipts.request("pair.token", generation: 2) { throw Failure() }; XCTFail("creation should fail") } catch {}
        XCTAssertNil(receipts["pair.token"])
    }

    func testRecoveryDeadlinesAllowAReplacementAfterExpiryAndCompletionOnlyInItsWindow() {
        let waiting = Date(timeIntervalSince1970: 1000)
        let ready = Date(timeIntervalSince1970: 1100)
        let expired = Date(timeIntervalSince1970: 1200)
        // The core can retain an expired pending record and its ready flag.
        let state = RecoveryView.RecoveryState(enabled: true, ready: true, member: false,
            pending: .init(opId: String(repeating: "a", count: 64), executeAfter: 1_100_000, expiresAt: 1_200_000, thisDevice: true))
        XCTAssertFalse(state.canRequest(at: waiting))
        XCTAssertFalse(state.canComplete(at: waiting))
        XCTAssertFalse(state.canRequest(at: ready))
        XCTAssertTrue(state.canComplete(at: ready))
        XCTAssertTrue(state.canRequest(at: expired))
        XCTAssertFalse(state.canComplete(at: expired))
        let other = RecoveryView.RecoveryState(enabled: true, ready: false, member: false,
            pending: .init(opId: String(repeating: "a", count: 64), executeAfter: 1_100_000, expiresAt: 1_200_000, thisDevice: false))
        XCTAssertFalse(other.canComplete(at: ready))
        XCTAssertFalse(other.canRequest(at: ready))
        XCTAssertTrue(other.canRequest(at: expired))
        let takenOver = RecoveryView.RecoveryState(enabled: true, ready: false, member: true)
        XCTAssertTrue(takenOver.canComplete(at: expired))
        XCTAssertFalse(takenOver.canRequest(at: expired))
        let disabled = RecoveryView.RecoveryState(enabled: false, ready: true, member: false, pending: state.pending)
        XCTAssertFalse(disabled.canRequest(at: expired))
        XCTAssertFalse(disabled.canComplete(at: ready))
    }

    #if DEBUG
    @MainActor func testAccountNamesFillOlderAccountsPreserveOwnProfileAndIgnoreLateReads() async {
        let current = "040g2081040g2081040g", other = "040g2082040g2082040g"
        let model = SessionModel()
        model.installFixtures(SessionSnapshot(generation: 1, sequence: 1, state: "unlocked", account: current, deviceId: "device", pendingSync: 0, conflicts: 0, approvalsWaiting: 0, fastUnlock: false, loginExpiresAtMs: nil, loginRenewable: false, passkeyAvailable: false, setupPending: false, lastSyncAtMs: nil, syncError: nil), items: [])
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

    @MainActor func testSettingsDraftsSurviveBackgroundButClearOnLockAndIgnoreOldViewWrites() async {
        let model = SessionModel()
        model.installFixtures(SessionSnapshot(generation: 1, sequence: 1, state: "unlocked", account: "040g2081040g2081040g", deviceId: "device", pendingSync: 0, conflicts: 0, approvalsWaiting: 0, fastUnlock: false, loginExpiresAtMs: nil, loginRenewable: false, passkeyAvailable: false, setupPending: false, lastSyncAtMs: nil, syncError: nil), items: [])
        let epoch = model.epoch, generation = model.viewGeneration
        model.keepFormDraft("profile", value: ProfileDraft(name: "Draft"), generation: generation)
        XCTAssertNotNil(model.formDraft("profile", as: ProfileDraft.self))
        model.background()
        XCTAssertNotEqual(model.epoch, epoch)
        XCTAssertEqual(model.viewGeneration, generation)
        XCTAssertEqual(model.formDraft("profile", as: ProfileDraft.self)?.name, "Draft")
        model.keepFormDraft("profile", value: ProfileDraft(name: "Continued draft"), generation: generation)
        XCTAssertEqual(model.formDraft("profile", as: ProfileDraft.self)?.name, "Continued draft")
        await model.lock()
        XCTAssertNil(model.formDraft("profile", as: ProfileDraft.self))
        model.keepFormDraft("profile", value: ProfileDraft(name: "Late result"), generation: generation)
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
