import XCTest
import UIKit
@preconcurrency import DMsgCore
@testable import dMsg

final class NotificationTests: XCTestCase {
    func testServiceRemindersDescribeTheIssueAndKeepAnIndependentReadState() throws {
        let network = try decode(#"{"id":"network","account":"a","target":{"kind":"service","id":"network:1"},"version":"9","open":true,"actionable":true,"first_at":1,"updated_at":1,"observed":"9","stage":1}"#, as: ReminderItem.self)
        XCTAssertTrue(network.isUnread, "Visibility is separate from explicit acknowledgement")
        var read = network; read.readVersion = "9"
        XCTAssertFalse(read.isUnread)
        XCTAssertTrue(read.open && read.actionable)
        read.readVersion = "8"; XCTAssertTrue(read.isUnread)
        let registration = try decode(#"{"id":"registration","account":"a","target":{"kind":"service","id":"registration:1"},"version":"1","open":true,"actionable":true,"first_at":1,"updated_at":1,"observed":"0","stage":1}"#, as: ReminderItem.self)
        XCTAssertNotEqual(network.title, registration.title)
        XCTAssertNotEqual(network.body, registration.body)
    }

    @MainActor
    func testRepeatedAPNsTokenDoesNotRestartRegistration() {
        let previous = DMsgAppDelegate.token
        defer { DMsgAppDelegate.token = previous }
        DMsgAppDelegate.token = nil
        let changed = expectation(description: "Only new tokens wake registration")
        changed.expectedFulfillmentCount = 2
        let observer = NotificationCenter.default.addObserver(forName: .dmsgPushToken, object: nil, queue: .main) { _ in changed.fulfill() }
        defer { NotificationCenter.default.removeObserver(observer) }
        let delegate = DMsgAppDelegate()
        delegate.application(.shared, didRegisterForRemoteNotificationsWithDeviceToken: Data([1, 2]))
        delegate.application(.shared, didRegisterForRemoteNotificationsWithDeviceToken: Data([1, 2]))
        delegate.application(.shared, didRegisterForRemoteNotificationsWithDeviceToken: Data([3, 4]))
        XCTAssertEqual(DMsgAppDelegate.token, "0304")
        wait(for: [changed], timeout: 1)
    }

    @MainActor
    func testAPNsFailureReportsAnErrorWithoutRequestingAnotherRegistration() {
        let failed = expectation(description: "APNs failure reaches the UI")
        let observer = NotificationCenter.default.addObserver(forName: .dmsgPushRegistrationFailed, object: nil, queue: .main) { _ in failed.fulfill() }
        defer { NotificationCenter.default.removeObserver(observer) }
        DMsgAppDelegate().application(.shared, didFailToRegisterForRemoteNotificationsWithError: URLError(.notConnectedToInternet))
        wait(for: [failed], timeout: 1)
        let previous = DMsgAppDelegate.token
        defer { DMsgAppDelegate.token = previous }
        DMsgAppDelegate.token = nil
        let push = NativePush()
        push.registrationFailed()
        guard case .failed = push.registration else { return XCTFail("A failed APNs request must not remain waiting") }
        XCTAssertFalse(push.working)
    }

    @MainActor
    func testPendingNotificationSyncKeepsTheValidAppleRegistration() {
        let now: UInt64 = 1000
        var subscription = NativePush.Subscription(route: "route", token: "token", expires: now + 2 * 86_400_000, revision: 7, unified: false)
        XCTAssertTrue(subscription.current(token: "token", now: now), "Retry the notification stage without a new App Attest registration")
        subscription.unified = true
        XCTAssertTrue(subscription.current(token: "token", now: now))
        XCTAssertFalse(subscription.current(token: "rotated-token", now: now))
        XCTAssertFalse(subscription.current(token: "token", now: now + 86_400_000))
        subscription.revision = nil
        XCTAssertFalse(subscription.current(token: "token", now: now), "Legacy leases still need a registration revision")
    }

    private func decode<T: Decodable>(_ json: String, as type: T.Type) throws -> T {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(type, from: Data(json.utf8))
    }

    func testAccountRemindersUseIdentityAndIgnoreSystemBadgePreference() {
        let status = ReminderStatus(counts: .init(actionable: 2, unread: 120, unverified: true), accounts: [
            .init(account: "other", enabled: false, counts: .init(actionable: 2, unread: 120, unverified: false)),
            .init(account: "current", enabled: true, counts: .init(actionable: 0, unread: 0, unverified: true))
        ], badge: 0, enabled: false)
        XCTAssertEqual(status.reminderCount(for: "current"), 0)
        XCTAssertEqual(status.reminderCount(for: "other"), 122)
        XCTAssertEqual(status.reminderCount(for: "missing"), 0)
    }

    func testTargetsKeepExactWalletVersionsAndRejectUnknownNavigation() throws {
        let target = ReminderTarget.wallet("aaaaa-aa", "18446744073709551615")
        XCTAssertEqual(try decode(NoticeEncoding.json(target), as: ReminderTarget.self), target)
        for json in [
            #"{"kind":"url","id":"https://example.com"}"#,
            #"{"kind":"channel","id":"not-a-hash"}"#,
            #"{"kind":"wallet","id":{"wallet":"aaaaa-aa","proposal":"01"}}"#,
            #"{"kind":"task","id":"arbitrary-rpc"}"#
        ] { XCTAssertThrowsError(try decode(json, as: ReminderTarget.self)) }
    }

    func testSecurityAlertsNeedDeviceAccountAndCategory() {
        let categories = Dictionary(uniqueKeysWithValues: ReminderCategory.allCases.map { ($0.rawValue, NoticeCategoryPreference(enabled: true, sound: false)) })
        var preferences = NoticePreferences(global: .init(revision: 0, enabled: false, pausedUntil: nil, quietHours: nil, securityException: false, badge: "all"),
            account: .init(revision: 0, enabled: false, categories: categories, mergeDevices: true, agentMail: [:], remoteRevision: 0), accountId: "a", remoteSupported: true)
        XCTAssertFalse(preferences.securityAlertsOn)
        preferences.global.enabled = true
        XCTAssertFalse(preferences.securityAlertsOn)
        preferences.account?.enabled = true
        XCTAssertTrue(preferences.securityAlertsOn)
        preferences.account?.categories["security"]?.enabled = false
        XCTAssertFalse(preferences.securityAlertsOn)
    }

    func testPushRoutesAllowOnlyVersionedOpaqueIdentity() {
        let route = String(repeating: "a", count: 128), local = String(repeating: "b", count: 64)
        XCTAssertEqual(NotificationRoute.parse(["version": 1, "route": route]), route)
        XCTAssertEqual(NotificationRoute.parse(["version": 1, "local": local]), "local:" + local)
        XCTAssertNil(NotificationRoute.parse(["version": 2, "route": route]))
        XCTAssertNil(NotificationRoute.parse(["version": 1, "route": "https://example.com"]))
        XCTAssertNil(NotificationRoute.parse(["version": 1, "local": String(repeating: "z", count: 64)]))
    }

    func testQuietHoursCrossMidnightAndDSTUsingTheirSavedZone() throws {
        let parser = ISO8601DateFormatter()
        var preference = NoticeGlobalPreference(revision: 0, enabled: true, pausedUntil: nil,
            quietHours: .init(start: 22 * 60, end: 8 * 60, timezone: "America/Los_Angeles"),
            securityException: false, badge: "all")
        let spring = try XCTUnwrap(parser.date(from: "2026-03-08T09:30:00Z"))
        let springEnd = try XCTUnwrap(parser.date(from: "2026-03-08T15:00:00Z"))
        XCTAssertEqual(NotificationTime.quietUntil(preference, at: spring), UInt64(springEnd.timeIntervalSince1970 * 1000))
        XCTAssertNil(NotificationTime.quietUntil(preference, at: springEnd))
        preference.quietHours?.end = 90
        let fall = try XCTUnwrap(parser.date(from: "2026-11-01T07:30:00Z"))
        let fallEnd = try XCTUnwrap(parser.date(from: "2026-11-01T09:30:00Z"))
        XCTAssertEqual(NotificationTime.quietUntil(preference, at: fall), UInt64(fallEnd.timeIntervalSince1970 * 1000))
        let repeatedHour = try XCTUnwrap(parser.date(from: "2026-11-01T08:45:00Z"))
        XCTAssertEqual(NotificationTime.quietUntil(preference, at: repeatedHour), UInt64(fallEnd.timeIntervalSince1970 * 1000))
        XCTAssertNil(NotificationTime.quietUntil(preference, at: fallEnd))
        preference.quietHours?.end = 150
        let gapEnd = try XCTUnwrap(parser.date(from: "2026-03-08T10:30:00Z"))
        let gap = try XCTUnwrap(parser.date(from: "2026-03-08T10:15:00Z"))
        XCTAssertEqual(NotificationTime.quietUntil(preference, at: gap), UInt64(gapEnd.timeIntervalSince1970 * 1000))
    }

    func testNamedRustSettingsPersistConflictAndRejectForeignAccount() async throws {
        let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let engine = try await MobileEngine.open(directory: directory.appending(path: "account").path,
            staging: directory.appending(path: "imports").path, platform: CoreBridgeTests.NoHardware())
        var settings = try decode(try await engine.notificationPreferences(selected: nil).json, as: NoticePreferences.self)
        XCTAssertFalse(settings.global.enabled)
        settings.global.enabled = true
        let saved = try await engine.notificationSetGlobal(expected: settings.global.revision, settingsJson: NoticeEncoding.json(settings.global))
        let global = try decode(saved.json, as: NoticeGlobalPreference.self)
        XCTAssertTrue(global.enabled)
        XCTAssertEqual(global.revision, 1)
        do {
            _ = try await engine.notificationSetGlobal(expected: 0, settingsJson: NoticeEncoding.json(settings.global))
            XCTFail("Stale settings overwrote the saved revision")
        } catch MobileError.Failure(let code) { XCTAssertEqual(code, "VERSION_CONFLICT") }
        do {
            _ = try await engine.notificationItems(selected: "080g2081040g2081040g", scope: .recent, offset: 0)
            XCTFail("Unbound account exposed reminder state")
        } catch MobileError.Failure { }
        let batch = try decode(try await engine.notificationNext(context: .init(allowed: true, foreground: true, quietUntil: nil, visible: [])).json, as: ReminderBatch.self)
        XCTAssertTrue(batch.plans.isEmpty)
        engine.lockNow()
        do {
            _ = try await engine.notificationStatus()
            XCTFail("Stale UI handle read a replacement session")
        } catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        try await engine.close()
    }
}
