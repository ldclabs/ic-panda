import CryptoKit
import Foundation
import Security
import XCTest
@testable import dMsg

final class NotificationKeychainTests: XCTestCase {
    func testNotificationCredentialsSurviveAContainerPathChange() throws {
        let installation = UUID().uuidString
        let security = AppleSecurity(installation: installation)
        defer { removeService(installation) }
        let before = "dmsg:notifications:" + String(repeating: "1", count: 64) + ":"
        let after = "dmsg:notifications:" + String(repeating: "2", count: 64) + ":"
        let key = Data(repeating: 7, count: 32)
        let receiver = Data("saved receiver".utf8)
        try security.set(name: before + "installation", secret: key)
        try security.set(name: before + "040g2081040g2081040g", secret: receiver)
        XCTAssertEqual(try security.get(name: after + "installation"), key)
        XCTAssertEqual(try security.get(name: after + "040g2081040g2081040g"), receiver)
        try security.delete(name: after + "040g2081040g2081040g")
        XCTAssertNil(try security.get(name: before + "040g2081040g2081040g"))
    }

    func testUpgradeRecoversTheRegisteredKeyDespiteLaterFailedRetries() throws {
        let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let old = try PrivateFiles(base: directory.appending(path: "old-container"))
        let installation = old.installation
        defer { removeService(installation) }
        let originalPrefix = prefix(old.workspace)
        let seed = Data(repeating: 7, count: 32)
        // SHA256(public_hex) from tests/vectors/client/primitives-v1.json's
        // Ed25519 seed 07 vector, also verified by the Rust core tests.
        let registered = "fe812c12f3ab4ce6ac5db69ac352f906cb1b11ef43fb33e252ef7ff552263889"
        try JSONSerialization.data(withJSONObject: ["installation": registered])
            .write(to: old.workspace.appending(path: "notification-runtime.json"))
        let account = "040g2081040g2081040g"
        let otherAccount = "080g2081040g2081040g"
        let receiver = Data("registered receiver".utf8)
        let revocation = "revoke:" + String(repeating: "3", count: 64) + ":4"
        try insertLegacy(installation, name: originalPrefix + "installation", data: seed)
        try insertLegacy(installation, name: originalPrefix + account, data: receiver)
        try insertLegacy(installation, name: originalPrefix + otherAccount, data: Data("other receiver".utf8))
        try insertLegacy(installation, name: originalPrefix + revocation, data: Data("retired receiver".utf8))

        // TestFlight moves the container, preserving files and the Keychain.
        let updated = directory.appending(path: "updated-container")
        try FileManager.default.moveItem(at: old.root, to: updated)
        let files = try PrivateFiles(base: updated)
        XCTAssertEqual(files.installation, installation)
        let currentPrefix = prefix(files.workspace)
        XCTAssertNotEqual(currentPrefix, originalPrefix)
        // The broken retry already generated a new key, but that key was never
        // accepted as the existing APNs registration's notification owner.
        try insertLegacy(installation, name: currentPrefix + "installation", data: Data(repeating: 8, count: 32))
        let security = AppleSecurity(installation: installation, notificationWorkspace: files.workspace)
        // Receiver reads can happen before an installation-key read.
        XCTAssertEqual(try security.get(name: currentPrefix + account), receiver)
        XCTAssertEqual(try security.get(name: currentPrefix + "installation"), seed)
        XCTAssertEqual(try security.get(name: currentPrefix + revocation), Data("retired receiver".utf8))
        XCTAssertEqual(try security.get(name: currentPrefix + otherAccount), Data("other receiver".utf8))
        let renewed = Data("renewed receiver".utf8)
        try security.set(name: currentPrefix + account, secret: renewed)
        XCTAssertEqual(try security.get(name: originalPrefix + account), renewed)

        // A second update and adapter instance use the pinned namespace.
        let next = directory.appending(path: "next-container")
        try FileManager.default.moveItem(at: files.root, to: next)
        let nextFiles = try PrivateFiles(base: next)
        let restarted = AppleSecurity(installation: installation, notificationWorkspace: nextFiles.workspace)
        let nextPrefix = prefix(nextFiles.workspace)
        XCTAssertEqual(try restarted.get(name: nextPrefix + "installation"), seed)
        XCTAssertEqual(try restarted.get(name: nextPrefix + account), renewed)
        try restarted.delete(name: nextPrefix + account)
        XCTAssertNil(try restarted.get(name: nextPrefix + account), "Deleting a receiver must not resurrect its legacy copy")
        XCTAssertEqual(try restarted.get(name: nextPrefix + otherAccount), Data("other receiver".utf8))
        try restarted.delete(name: nextPrefix + revocation)
        XCTAssertNil(try restarted.get(name: nextPrefix + revocation))
    }

    func testNotificationNamespacesStayWithinTheirInstallationAndEnvironment() throws {
        let first = UUID().uuidString, second = UUID().uuidString
        defer { removeService(first); removeService(first, environment: "staging"); removeService(second) }
        let production = AppleSecurity(installation: first)
        let staging = AppleSecurity(installation: first, environment: "staging")
        let reinstalled = AppleSecurity(installation: second)
        let name = "dmsg:notifications:" + String(repeating: "1", count: 64) + ":installation"
        try production.set(name: name, secret: Data(repeating: 7, count: 32))
        XCTAssertNil(try staging.get(name: name))
        XCTAssertNil(try reinstalled.get(name: name))
        let other = "dmsg:ordinary:" + String(repeating: "1", count: 64) + ":key"
        try production.set(name: other, secret: Data([1]))
        XCTAssertNil(try production.get(name: other.replacingOccurrences(of: "1", with: "2")))
        XCTAssertEqual(try production.get(name: other), Data([1]))
    }

    private func prefix(_ workspace: URL) -> String {
        let path = workspace.appending(path: "notification-runtime.json").path
        return "dmsg:notifications:" + SHA256.hash(data: Data(path.utf8)).map { String(format: "%02x", $0) }.joined() + ":"
    }

    private func query(_ installation: String, environment: String = "production") -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: "\(Bundle.main.bundleIdentifier ?? "net.dmsg.ios").\(environment).\(installation)",
         kSecAttrSynchronizable as String: false]
    }

    private func insertLegacy(_ installation: String, name: String, data: Data) throws {
        var item = query(installation)
        item[kSecAttrAccount as String] = name
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly
        let status = SecItemAdd(item as CFDictionary, nil)
        XCTAssertEqual(status, errSecSuccess)
        if status != errSecSuccess { throw NSError(domain: NSOSStatusErrorDomain, code: Int(status)) }
    }

    private func removeService(_ installation: String, environment: String = "production") {
        SecItemDelete(query(installation, environment: environment) as CFDictionary)
    }
}
