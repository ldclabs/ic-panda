import CryptoKit
import Foundation
import LocalAuthentication
import Security
import XCTest
@testable import dMsg

final class KeychainMigrationTests: XCTestCase {
    private let source = KeychainMigration.originalPrefix + "net.dmsg.ios"
    private let destination = KeychainMigration.mainGroup

    private func service(_ installation: String, environment: String = "production") -> String {
        "\(Bundle.main.bundleIdentifier!).\(environment).\(installation)"
    }

    private func query(_ service: String, group: String, account: String? = nil) -> [String: Any] {
        var query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service, kSecAttrAccessGroup as String: group,
            kSecAttrSynchronizable as String: false]
        if let account { query[kSecAttrAccount as String] = account }
        return query
    }

    private func insert(_ service: String, group: String, account: String, data: Data) throws {
        var item = query(service, group: group, account: account)
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly
        let status = SecItemAdd(item as CFDictionary, nil)
        guard status == errSecSuccess else { throw NSError(domain: NSOSStatusErrorDomain, code: Int(status)) }
    }

    private func item(_ service: String, group: String, account: String) throws -> [String: Any]? {
        var query = query(service, group: group, account: account)
        query[kSecReturnAttributes as String] = true
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw NSError(domain: NSOSStatusErrorDomain, code: Int(status)) }
        return try XCTUnwrap(result as? [String: Any])
    }

    private func remove(_ service: String) {
        for group in [source, destination] { SecItemDelete(query(service, group: group) as CFDictionary) }
    }

    func testUpgradeMovesAllAccountsWithoutChangingBytesOrProtection() throws {
        let installation = UUID().uuidString, another = UUID().uuidString
        let current = service(installation), staging = service(installation, environment: "staging"), other = service(another)
        defer { [current, staging, other].forEach(remove) }
        let records = ["account-a:device-bound": Data([1,2,3]), "account-b:fast-unlock": Data([4,5]),
                       "account-b:passkey-session": Data([6]), "dmsg:notifications:ios:installation": Data(repeating: 7, count: 32)]
        for (name, data) in records { try insert(current, group: source, account: name, data: data) }
        for service in [staging, other] { try insert(service, group: source, account: "untouched", data: Data([8])) }
        let upgraded = try AppleSecurity(installation: installation, signingPrefix: KeychainMigration.originalPrefix)
        for (name, data) in records {
            XCTAssertEqual(try upgraded.get(name: name), data)
            XCTAssertNil(try item(current, group: source, account: name))
            let moved = try XCTUnwrap(item(current, group: destination, account: name))
            XCTAssertEqual(moved[kSecAttrAccessible as String] as? String, kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly as String)
            XCTAssertEqual(moved[kSecAttrSynchronizable as String] as? Bool, false)
        }
        for service in [staging, other] {
            XCTAssertNotNil(try item(service, group: source, account: "untouched"))
            XCTAssertNil(try item(service, group: destination, account: "untouched"))
        }
        // Re-running migration and then using the recipient's prefix preserves
        // identity. This models routing only, not actual cross-team signing.
        _ = try AppleSecurity(installation: installation, signingPrefix: KeychainMigration.originalPrefix)
        let recipient = try AppleSecurity(installation: installation, signingPrefix: "NEWTEAM1234.")
        XCTAssertEqual(try recipient.get(name: "account-a:device-bound"), records["account-a:device-bound"])
        try recipient.set(name: "account-a:device-bound", secret: Data([9]))
        try recipient.delete(name: "account-b:fast-unlock")
        let restarted = try AppleSecurity(installation: installation, signingPrefix: KeychainMigration.originalPrefix)
        XCTAssertEqual(try restarted.get(name: "account-a:device-bound"), Data([9]))
        XCTAssertNil(try restarted.get(name: "account-b:fast-unlock"), "Deleted secrets must not return from the old group")
    }

    func testDuplicateStopsMigrationWithoutDeletingEitherKeyAndCanBeRetried() throws {
        let installation = UUID().uuidString, name = service(installation)
        defer { remove(name) }
        try insert(name, group: source, account: "binding", data: Data([1]))
        try insert(name, group: destination, account: "binding", data: Data([2]))
        XCTAssertThrowsError(try AppleSecurity(installation: installation, signingPrefix: KeychainMigration.originalPrefix)) {
            XCTAssertEqual($0 as? KeychainMigration.Failure, .unavailable(errSecDuplicateItem))
        }
        XCTAssertEqual(try item(name, group: source, account: "binding")?[kSecValueData as String] as? Data, Data([1]))
        XCTAssertEqual(try item(name, group: destination, account: "binding")?[kSecValueData as String] as? Data, Data([2]))
        XCTAssertNil(try item(name, group: destination, account: "dmsg:keychain:app-group:v1"))
        // Simulate an explicitly resolved conflict; production never deletes it.
        SecItemDelete(query(name, group: destination, account: "binding") as CFDictionary)
        let retry = try AppleSecurity(installation: installation, signingPrefix: KeychainMigration.originalPrefix)
        XCTAssertEqual(try retry.get(name: "binding"), Data([1]))
    }

    func testSkippedPreparationLeavesExistingWorkspaceAndOldKeysUntouched() throws {
        let installation = UUID().uuidString, name = service(installation)
        let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { remove(name); try? FileManager.default.removeItem(at: directory) }
        let workspace = directory.appending(path: "workspace.db"), bytes = Data("existing encrypted workspace".utf8)
        try bytes.write(to: workspace)
        try insert(name, group: source, account: "binding", data: Data([1]))
        XCTAssertThrowsError(try AppleSecurity(installation: installation, notificationWorkspace: directory, signingPrefix: "NEWTEAM1234.")) {
            XCTAssertEqual($0 as? KeychainMigration.Failure, .preparationMissing)
        }
        XCTAssertEqual(try Data(contentsOf: workspace), bytes)
        XCTAssertNotNil(try item(name, group: source, account: "binding"))
        XCTAssertNil(try item(name, group: destination, account: "dmsg:keychain:app-group:v1"))
        _ = try AppleSecurity(installation: installation, notificationWorkspace: directory, signingPrefix: KeychainMigration.originalPrefix)
        let prepared = try AppleSecurity(installation: installation, notificationWorkspace: directory, signingPrefix: "NEWTEAM1234.")
        XCTAssertEqual(try prepared.get(name: "binding"), Data([1]))
        XCTAssertEqual(try Data(contentsOf: workspace), bytes)
    }

    func testFreshRecipientInstallCanPrepareAnEmptyWorkspace() throws {
        let installation = UUID().uuidString, directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { remove(service(installation)); try? FileManager.default.removeItem(at: directory) }
        let fresh = try AppleSecurity(installation: installation, notificationWorkspace: directory, signingPrefix: "NEWTEAM1234.")
        let receipt = try XCTUnwrap(item(service(installation), group: destination, account: "dmsg:keychain:app-group:v1"))
        XCTAssertEqual(receipt[kSecAttrAccessible as String] as? String, kSecAttrAccessibleWhenUnlockedThisDeviceOnly as String,
                       "A non-secret preparation receipt must not require setting a device passcode just to launch")
        try fresh.set(name: "new-device", secret: Data([1]))
        XCTAssertEqual(try fresh.get(name: "new-device"), Data([1]))
    }

    func testUnavailableKeychainIsNeverTreatedAsAnEmptyMigration() {
        for status in [errSecInteractionNotAllowed, errSecMissingEntitlement, errSecAuthFailed, errSecUserCanceled, errSecNotAvailable] {
            XCTAssertThrowsError(try KeychainMigration.move(service: "dmsg-test", from: source, to: destination, update: { query, _ in
                let context = (query as NSDictionary)[kSecUseAuthenticationContext] as? LAContext
                XCTAssertEqual(context?.interactionNotAllowed, true)
                return status
            })) { XCTAssertEqual($0 as? KeychainMigration.Failure, .unavailable(status)) }
        }
    }

    func testExportMigrationMovesOnlyTheRequestedSnapshot() throws {
        let name = "net.dmsg.export.test." + UUID().uuidString
        defer { remove(name) }
        for account in ["selected", "expired", "file-session"] {
            try insert(name, group: source, account: account, data: Data(account.utf8))
        }
        try KeychainMigration.move(service: name, from: source, to: destination, account: "selected", context: LAContext())
        XCTAssertNotNil(try item(name, group: destination, account: "selected"))
        for account in ["expired", "file-session"] {
            XCTAssertNotNil(try item(name, group: source, account: account))
            XCTAssertNil(try item(name, group: destination, account: account))
        }
    }
}
