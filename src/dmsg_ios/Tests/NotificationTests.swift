import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

final class NotificationTests: XCTestCase {
    private func decode<T: Decodable>(_ json: String, as type: T.Type) throws -> T {
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(type, from: Data(json.utf8))
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
