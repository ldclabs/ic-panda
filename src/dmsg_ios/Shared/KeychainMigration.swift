import Foundation
import LocalAuthentication
import Security

/// App-group keychain names stay unchanged when their groups move to another
/// developer team. Ordinary team-prefixed keychain names do not.
enum KeychainMigration {
    static let originalPrefix = "S9Z2WAMAV8."
    static let mainGroup = "group.net.dmsg.ios.keychain"

    enum Failure: Error, Equatable {
        case unavailable(OSStatus)
        case preparationMissing

        var message: String {
            switch self {
            case .unavailable:
                String(localized: "Your saved keys could not be prepared. Unlock this iPhone and try again. Your workspace has not been erased.")
            case .preparationMissing:
                String(localized: "This update cannot access your saved keys. Keep dMsg installed and contact support from another authorized device. Your workspace has not been erased.")
            }
        }
    }

    static func signingPrefix() throws -> String {
        guard let prefix = Bundle.main.object(forInfoDictionaryKey: "DMsgKeychainPrefix") as? String,
              prefix.count == 11, prefix.last == ".",
              prefix.dropLast().utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) }) else {
            throw Failure.unavailable(errSecMissingEntitlement)
        }
        return prefix
    }

    /// Move exactly this service's non-synchronizing items, including inactive
    /// accounts. SecItemUpdate preserves data and access controls. A duplicate
    /// or any other error stops the caller; never delete either conflicting item.
    static func move(service: String, from source: String, to destination: String,
                     account: String? = nil, context: LAContext? = nil,
                     update: (CFDictionary, CFDictionary) -> OSStatus = SecItemUpdate) throws {
        precondition(!service.isEmpty && !source.isEmpty && !destination.isEmpty && source != destination)
        let authentication = context ?? LAContext()
        if context == nil { authentication.interactionNotAllowed = true }
        defer { if context == nil { authentication.invalidate() } }
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccessGroup as String: source,
            kSecAttrSynchronizable as String: false,
            kSecUseAuthenticationContext as String: authentication
        ]
        if let account { query[kSecAttrAccount as String] = account }
        // Only an explicit export-access action supplies a context. Background
        // work and opening the main credential store never trigger a prompt.
        let status = update(query as CFDictionary, [kSecAttrAccessGroup as String: destination] as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw Failure.unavailable(status) }
    }
}
