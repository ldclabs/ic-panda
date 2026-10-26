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
    private var authorization: UInt64?
    private var nextAuthorization: UInt64 = 0

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

    func biometryState() -> Data? {
        let context = LAContext()
        guard context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil),
              let state = context.domainState.biometry.stateHash else { return nil }
        return Data(SHA256.hash(data: state))
    }

    private func access(_ biometryOnly: Bool) throws -> SecAccessControl {
        if biometryOnly && biometryState() == nil { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        guard let access = SecAccessControlCreateWithFlags(nil,
            kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly,
            biometryOnly ? [.privateKeyUsage, .biometryCurrentSet] : [.privateKeyUsage, .userPresence], nil)
        else { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        return access
    }

    func createAgreement(biometryOnly: Bool) throws -> Agreement {
        guard SecureEnclave.isAvailable else { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        do {
            let key = try SecureEnclave.P256.KeyAgreement.PrivateKey(accessControl: access(biometryOnly))
            let peer = P256.KeyAgreement.PrivateKey()
            let shared = try peer.sharedSecretFromKeyAgreement(with: key.publicKey)
            return Agreement(handle: key.dataRepresentation, peer: peer.publicKey.x963Representation,
                             secret: shared.withUnsafeBytes { Data($0) })
        } catch { throw mapped(error) }
    }

    func openAgreement(handle: Data, peer: Data, biometryOnly: Bool) throws -> Data {
        guard !handle.isEmpty, peer.count == 65, peer.first == 4 else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
        let context = try authenticate(reason: String(localized: "Unlock your dMsg workspace"), operation: .useKeyKeyExchange, biometryOnly: biometryOnly)
        defer { finish(context) }
        do {
            let key = try SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: handle, authenticationContext: context)
            let peer = try P256.KeyAgreement.PublicKey(x963Representation: peer)
            return try key.sharedSecretFromKeyAgreement(with: peer).withUnsafeBytes { Data($0) }
        } catch { throw mapped(error) }
    }

    func createSigning(biometryOnly: Bool) throws -> SigningKey {
        guard SecureEnclave.isAvailable else { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        do {
            let key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: access(biometryOnly))
            let publicKey = key.publicKey.x963Representation
            guard publicKey.count == 65, publicKey.first == 4, let last = publicKey.last else {
                throw MobileError.Failure(code: "INTEGRITY_FAILED")
            }
            var compressed = Data([2 | (last & 1)])
            compressed.append(publicKey.dropFirst().prefix(32))
            return SigningKey(publicKey: compressed, handle: key.dataRepresentation)
        } catch { throw mapped(error) }
    }

    /// The context belongs to exactly one Rust decision. It stays usable only
    /// until that decision drops its private authorization or the session ends.
    func authorize(reason: String, biometryOnly: Bool) throws -> UInt64 {
        let context = try authenticate(reason: explanation(reason), operation: .useKeySign, biometryOnly: biometryOnly)
        mutex.lock()
        guard active === context else { mutex.unlock(); context.invalidate(); throw MobileError.Failure(code: "CANCELLED") }
        nextAuthorization &+= 1
        let token = nextAuthorization
        authorization = token
        mutex.unlock()
        return token
    }

    func signAuthorized(authorization token: UInt64, handle: Data, message: Data) throws -> Data {
        guard !handle.isEmpty, message.count == 32 else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
        mutex.lock()
        let context = authorization == token ? active : nil
        mutex.unlock()
        guard let context else { throw MobileError.Failure(code: "CANCELLED") }
        do {
            let key = try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: handle, authenticationContext: context)
            // H is the domain-separated message, not a prehashed digest.
            let signature = try key.signature(for: message).rawRepresentation
            mutex.lock(); let current = active === context && authorization == token; mutex.unlock()
            guard current else { throw MobileError.Failure(code: "CANCELLED") }
            return signature
        } catch { throw mapped(error) }
    }

    func releaseAuthorization(authorization token: UInt64) {
        mutex.lock()
        let context = authorization == token ? active : nil
        if context != nil { active = nil; authorization = nil }
        mutex.unlock()
        context?.invalidate()
    }

    func confirm(requestId: String, digest: String, reason: String) throws {
        guard !requestId.isEmpty, digest.count == 64 else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
        let context = try authenticate(reason: explanation(reason))
        finish(context)
    }

    private func explanation(_ reason: String) -> String {
        switch reason {
        case "device_pair": String(localized: "Approve this device and distribute new content keys")
        case "device_revoke": String(localized: "Revoke this device and rotate content keys")
        case "account", "device_capabilities", "account_control", "device_control", "principal_control": String(localized: "Confirm this account security change")
        case "approval_key": String(localized: "Authorize this device's approval key")
        case "attest", "attestation": String(localized: "Sign the statement you reviewed")
        default: String(localized: "Confirm the dMsg request you reviewed")
        }
    }

    func cancelPresence() {
        mutex.lock(); let context = active; active = nil; authorization = nil; mutex.unlock()
        context?.invalidate()
    }

    private func finish(_ context: LAContext) {
        mutex.lock(); if active === context { active = nil; authorization = nil }; mutex.unlock()
        context.invalidate()
    }

    private final class Answer: @unchecked Sendable {
        var error: Error?
        var success = false
    }

    private func authenticate(reason: String, operation: LAAccessControlOperation? = nil, biometryOnly: Bool = false) throws -> LAContext {
        precondition(!Thread.isMainThread)
        let accessControl = try operation.map { _ in try access(biometryOnly) }
        let context = LAContext()
        context.touchIDAuthenticationAllowableReuseDuration = 0
        mutex.lock()
        guard active == nil else { mutex.unlock(); throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        active = context
        mutex.unlock()
        let answer = Answer(), semaphore = DispatchSemaphore(value: 0)
        let reply: @Sendable (Bool, Error?) -> Void = { success, error in
            answer.success = success; answer.error = error; semaphore.signal()
        }
        if let operation, let accessControl {
            context.evaluateAccessControl(accessControl, operation: operation, localizedReason: reason, reply: reply)
        } else {
            context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason, reply: reply)
        }
        if semaphore.wait(timeout: .now() + 60) == .timedOut {
            finish(context)
            throw MobileError.Failure(code: "CANCELLED")
        }
        mutex.lock(); let current = active === context; mutex.unlock()
        guard current else { context.invalidate(); throw MobileError.Failure(code: "CANCELLED") }
        guard answer.success else {
            finish(context)
            throw answer.error.map(mapped) ?? MobileError.Failure(code: "HARDWARE_UNAVAILABLE")
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
           [.userCancel, .appCancel, .systemCancel, .authenticationFailed, .userFallback, .invalidContext].contains(error.code) {
            return MobileError.Failure(code: "CANCELLED")
        }
        let error = error as NSError
        if error.domain == NSOSStatusErrorDomain,
           [errSecItemNotFound, errSecDecode].contains(OSStatus(error.code)) {
            return MobileError.Failure(code: "INTEGRITY_FAILED")
        }
        return MobileError.Failure(code: "HARDWARE_UNAVAILABLE")
    }

    private func failure(_ status: OSStatus) -> MobileError {
        MobileError.Failure(code: [errSecItemNotFound, errSecDecode].contains(status) ? "INTEGRITY_FAILED" : "UNAVAILABLE")
    }
}
