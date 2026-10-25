import CryptoKit
import Foundation
import LocalAuthentication
import Security
@preconcurrency import DMsgCore

/// Callbacks run on Rust worker queues. The mutex protects only the active
/// prompt reference; it is never held while waiting for user interaction.
final class AppleSecurity: Platform, @unchecked Sendable {
    private let service: String
    private let mutex = NSLock()
    private var active: LAContext?

    init(installation: String) {
        service = "\(Bundle.main.bundleIdentifier ?? "net.dmsg.ios").production.\(installation)"
    }

    private func query(_ name: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service, kSecAttrAccount as String: name,
         kSecAttrSynchronizable as String: false]
    }

    func get(name: String) throws -> Data? {
        var query = query(name)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else { throw failure(status) }
        return data
    }

    func set(name: String, secret: Data) throws {
        let attributes: [String: Any] = [kSecValueData as String: secret,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly]
        let status = SecItemUpdate(query(name) as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            let status = SecItemAdd(query(name).merging(attributes) { _, value in value } as CFDictionary, nil)
            guard status == errSecSuccess else { throw failure(status) }
        } else if status != errSecSuccess { throw failure(status) }
    }

    func delete(name: String) throws {
        let status = SecItemDelete(query(name) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw failure(status) }
    }

    func presence() -> UInt8 {
        guard SecureEnclave.isAvailable else { return 0 }
        let context = LAContext()
        if context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil) { return 2 }
        return context.canEvaluatePolicy(.deviceOwnerAuthentication, error: nil) ? 1 : 0
    }

    private func access() throws -> SecAccessControl {
        guard let access = SecAccessControlCreateWithFlags(nil,
            kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly, [.privateKeyUsage, .userPresence], nil)
        else { throw MobileError.Failure(code: "UNAVAILABLE") }
        return access
    }

    func createAgreement() throws -> Agreement {
        guard SecureEnclave.isAvailable else { throw MobileError.Failure(code: "UNAVAILABLE") }
        do {
            let key = try SecureEnclave.P256.KeyAgreement.PrivateKey(accessControl: access())
            let peer = P256.KeyAgreement.PrivateKey()
            let shared = try peer.sharedSecretFromKeyAgreement(with: key.publicKey)
            return Agreement(handle: key.dataRepresentation, peer: peer.publicKey.x963Representation,
                             secret: shared.withUnsafeBytes { Data($0) })
        } catch { throw mapped(error) }
    }

    func openAgreement(handle: Data, peer: Data) throws -> Data {
        let context = try authenticate(reason: String(localized: "Unlock your dMsg workspace"))
        defer { finish(context) }
        do {
            let key = try SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: handle, authenticationContext: context)
            let peer = try P256.KeyAgreement.PublicKey(x963Representation: peer)
            return try key.sharedSecretFromKeyAgreement(with: peer).withUnsafeBytes { Data($0) }
        } catch { throw mapped(error) }
    }

    func confirm(requestId: String, digest: String, reason: String) throws {
        guard !requestId.isEmpty, digest.count == 64 else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
        let explanation: String
        switch reason {
        case "device_pair": explanation = String(localized: "Approve this device and distribute new content keys")
        case "device_revoke": explanation = String(localized: "Revoke this device and rotate content keys")
        case "account", "device_capabilities": explanation = String(localized: "Confirm this account security change")
        case "attest": explanation = String(localized: "Sign the statement you reviewed")
        default: explanation = String(localized: "Confirm the dMsg request you reviewed")
        }
        let context = try authenticate(reason: explanation)
        finish(context)
    }

    func cancelPresence() {
        mutex.lock(); let context = active; active = nil; mutex.unlock()
        context?.invalidate()
    }

    private func finish(_ context: LAContext) {
        mutex.lock(); if active === context { active = nil }; mutex.unlock()
        context.invalidate()
    }

    private final class Answer: @unchecked Sendable {
        var error: Error?
        var success = false
    }

    private func authenticate(reason: String) throws -> LAContext {
        precondition(!Thread.isMainThread)
        let context = LAContext()
        context.touchIDAuthenticationAllowableReuseDuration = 0
        mutex.lock()
        guard active == nil else { mutex.unlock(); throw MobileError.Failure(code: "UNAVAILABLE") }
        active = context
        mutex.unlock()
        let answer = Answer(), semaphore = DispatchSemaphore(value: 0)
        context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { success, error in
            answer.success = success; answer.error = error; semaphore.signal()
        }
        if semaphore.wait(timeout: .now() + 60) == .timedOut {
            finish(context)
            throw MobileError.Failure(code: "CANCELLED")
        }
        guard answer.success else {
            finish(context)
            throw answer.error.map(mapped) ?? MobileError.Failure(code: "UNAVAILABLE")
        }
        return context
    }

    private func mapped(_ error: Error) -> MobileError {
        if let error = error as? MobileError { return error }
        if let error = error as? CryptoKitError {
            switch error {
            case .incorrectParameterSize, .incorrectKeySize: return MobileError.Failure(code: "INTEGRITY_FAILED")
            default: break
            }
        }
        if let error = error as? LAError,
           [.userCancel, .appCancel, .systemCancel, .authenticationFailed].contains(error.code) {
            return MobileError.Failure(code: "CANCELLED")
        }
        let error = error as NSError
        if error.domain == NSOSStatusErrorDomain { return failure(OSStatus(error.code)) }
        return MobileError.Failure(code: "UNAVAILABLE")
    }

    private func failure(_ status: OSStatus) -> MobileError {
        MobileError.Failure(code: [errSecItemNotFound, errSecDecode, errSecParam].contains(status) ? "INTEGRITY_FAILED" : "UNAVAILABLE")
    }
}
