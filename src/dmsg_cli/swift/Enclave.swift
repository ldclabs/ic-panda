// Secure Enclave keys of the dmsg daemon (macOS), through CryptoKit, for
// `src/enclave.rs`. A key leaves CryptoKit only as its `dataRepresentation`,
// an opaque blob that just this Mac's enclave opens, so the keys need no
// keychain item and no entitlement. Every use of a key first asks for the
// person: Touch ID, or the login password; a biometry-only key takes only
// the fingerprints enrolled when it was made.
//
// Results: 0 done, 1 unavailable, 2 the person declined, 3 the key is
// unusable here, 4 the person cannot be asked now (no Touch ID keyboard,
// locked out, not interactive); a negative result preserves an OSStatus
// failure.

import CryptoKit
import Foundation
import LocalAuthentication
import Security

private let done: Int32 = 0
private let unavailable: Int32 = 1
private let declined: Int32 = 2
private let unusable: Int32 = 3
private let notNow: Int32 = 4

/// LAError codes of a person who cannot answer now, as raw values: passcode
/// not set, biometry not available, not enrolled, locked out, not paired,
/// disconnected, and not interactive.
private let notNowCodes: Set<Int> = [-5, -6, -7, -8, -12, -13, -1004]

/// Preserve native status codes without exposing error descriptions or userInfo.
/// Other error domains cannot use the reserved bridge results (0 through 4).
private func systemFailure(_ error: Error) -> Int32 {
    let error = error as NSError
    guard error.domain == NSOSStatusErrorDomain,
        let status = Int32(exactly: error.code), status < 0
    else { return unavailable }
    return status
}

/// The answer of an evaluation, set on another queue.
private final class Outcome: @unchecked Sendable {
    var code = unavailable
}

/// A key the person must be present to use; a biometry-only one takes
/// only the fingerprints enrolled now, never the login password.
private func access(_ biometryOnly: Int32 = 0) -> SecAccessControl? {
    SecAccessControlCreateWithFlags(
        nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
        biometryOnly != 0 ? [.privateKeyUsage, .biometryCurrentSet] : [.privateKeyUsage, .userPresence],
        nil)
}

/// How long a question waits for the person; the daemon waits with it.
private let patience: DispatchTimeInterval = .seconds(60)

/// Ask for the person before `operation`; the context then carries the
/// answer to the key, so the key does not ask again. A question left
/// unanswered is withdrawn and counts as declined.
private func present(
    _ operation: LAAccessControlOperation, _ reason: UnsafePointer<CChar>, _ biometryOnly: Int32 = 0
) -> (LAContext?, Int32) {
    guard let access = access(biometryOnly) else { return (nil, unavailable) }
    let context = LAContext()
    let outcome = Outcome()
    let answered = DispatchSemaphore(value: 0)
    context.evaluateAccessControl(
        access, operation: operation, localizedReason: String(cString: reason)
    ) { success, error in
        if success {
            outcome.code = done
        } else if let error = error as? LAError,
            [
                .userCancel, .appCancel, .systemCancel, .authenticationFailed, .userFallback,
                .invalidContext,
            ].contains(error.code)
        {
            outcome.code = declined
        } else if let error = error as? LAError, notNowCodes.contains(error.code.rawValue) {
            outcome.code = notNow
        }
        answered.signal()
    }
    if answered.wait(timeout: .now() + patience) == .timedOut {
        context.invalidate()
        answered.wait()
    }
    return outcome.code == done ? (context, done) : (nil, outcome.code)
}

private func copy(_ data: Data, _ to: UnsafeMutablePointer<UInt8>, _ count: Int) -> Bool {
    guard data.count == count else { return false }
    data.copyBytes(to: to, count: count)
    return true
}

private func write(
    _ data: Data, _ to: UnsafeMutablePointer<UInt8>, _ capacity: Int,
    _ length: UnsafeMutablePointer<Int>
) -> Bool {
    guard data.count <= capacity else { return false }
    data.copyBytes(to: to, count: data.count)
    length.pointee = data.count
    return true
}

/// 2 with Touch ID, 1 with the login password only, 0 without a usable enclave.
@_cdecl("dmsg_se_presence")
public func dmsgSePresence() -> Int32 {
    guard SecureEnclave.isAvailable else { return 0 }
    let context = LAContext()
    if context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil) {
        return 2
    }
    return context.canEvaluatePolicy(.deviceOwnerAuthentication, error: nil) ? 1 : 0
}

/// A digest of the enrolled fingerprints (32 bytes), which changes when
/// they do: 0 done, 1 without biometry.
@_cdecl("dmsg_se_biometry_state")
public func dmsgSeBiometryState(_ state: UnsafeMutablePointer<UInt8>) -> Int32 {
    let context = LAContext()
    guard context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil),
        let domain = context.evaluatedPolicyDomainState
    else { return unavailable }
    return copy(Data(SHA256.hash(data: domain)), state, 32) ? done : unavailable
}

/// A new key-agreement key and the secret it shares with a software peer
/// key, which only the enclave key can recover: the key's handle, the
/// peer's X9.63 public key (65 bytes) and the 32-byte secret. Asks nothing.
@_cdecl("dmsg_se_agreement_create")
public func dmsgSeAgreementCreate(
    _ handle: UnsafeMutablePointer<UInt8>, _ capacity: Int, _ length: UnsafeMutablePointer<Int>,
    _ peer: UnsafeMutablePointer<UInt8>, _ secret: UnsafeMutablePointer<UInt8>,
    _ biometryOnly: Int32
) -> Int32 {
    guard let access = access(biometryOnly) else { return unavailable }
    do {
        let key = try SecureEnclave.P256.KeyAgreement.PrivateKey(accessControl: access)
        let ephemeral = P256.KeyAgreement.PrivateKey()
        let shared = try ephemeral.sharedSecretFromKeyAgreement(with: key.publicKey)
        let bytes = shared.withUnsafeBytes { Data($0) }
        guard write(key.dataRepresentation, handle, capacity, length),
            copy(ephemeral.publicKey.x963Representation, peer, 65), copy(bytes, secret, 32)
        else { return unavailable }
        return done
    } catch {
        return systemFailure(error)
    }
}

/// Recover the secret of `dmsg_se_agreement_create`, with the person present.
@_cdecl("dmsg_se_agreement_open")
public func dmsgSeAgreementOpen(
    _ handle: UnsafePointer<UInt8>, _ length: Int, _ peer: UnsafePointer<UInt8>,
    _ reason: UnsafePointer<CChar>, _ secret: UnsafeMutablePointer<UInt8>, _ biometryOnly: Int32
) -> Int32 {
    let (context, outcome) = present(.useKeyKeyExchange, reason, biometryOnly)
    guard let context else { return outcome }
    let key: SecureEnclave.P256.KeyAgreement.PrivateKey
    let other: P256.KeyAgreement.PublicKey
    do {
        key = try SecureEnclave.P256.KeyAgreement.PrivateKey(
            dataRepresentation: Data(bytes: handle, count: length), authenticationContext: context)
        other = try P256.KeyAgreement.PublicKey(x963Representation: Data(bytes: peer, count: 65))
    } catch {
        return unusable
    }
    do {
        let shared = try key.sharedSecretFromKeyAgreement(with: other)
        return copy(shared.withUnsafeBytes { Data($0) }, secret, 32) ? done : unavailable
    } catch {
        return systemFailure(error)
    }
}

/// A new signing key: its handle and its X9.63 public key (65 bytes). Asks nothing.
@_cdecl("dmsg_se_signing_create")
public func dmsgSeSigningCreate(
    _ handle: UnsafeMutablePointer<UInt8>, _ capacity: Int, _ length: UnsafeMutablePointer<Int>,
    _ publicKey: UnsafeMutablePointer<UInt8>, _ biometryOnly: Int32
) -> Int32 {
    guard let access = access(biometryOnly) else { return unavailable }
    do {
        let key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: access)
        guard write(key.dataRepresentation, handle, capacity, length),
            copy(key.publicKey.x963Representation, publicKey, 65)
        else { return unavailable }
        return done
    } catch {
        return systemFailure(error)
    }
}

/// A new hardware anchor: a signing key that never asks for the person and
/// works once the Mac has been unlocked after it started. It shows only that
/// a workspace still runs on the Mac that made it: its handle and its X9.63
/// public key (65 bytes).
@_cdecl("dmsg_se_anchor_create")
public func dmsgSeAnchorCreate(
    _ handle: UnsafeMutablePointer<UInt8>, _ capacity: Int, _ length: UnsafeMutablePointer<Int>,
    _ publicKey: UnsafeMutablePointer<UInt8>
) -> Int32 {
    guard SecureEnclave.isAvailable,
        let access = SecAccessControlCreateWithFlags(
            nil, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, [.privateKeyUsage], nil)
    else { return unavailable }
    do {
        let key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: access)
        guard write(key.dataRepresentation, handle, capacity, length),
            copy(key.publicKey.x963Representation, publicKey, 65)
        else { return unavailable }
        return done
    } catch {
        return systemFailure(error)
    }
}

/// Sign one 32-byte message as `r || s` with an anchor, asking nothing: 3
/// when this Mac's enclave cannot open the handle.
@_cdecl("dmsg_se_anchor_sign")
public func dmsgSeAnchorSign(
    _ handle: UnsafePointer<UInt8>, _ length: Int, _ message: UnsafePointer<UInt8>,
    _ signature: UnsafeMutablePointer<UInt8>
) -> Int32 {
    guard SecureEnclave.isAvailable else { return unavailable }
    let context = LAContext()
    context.interactionNotAllowed = true
    let key: SecureEnclave.P256.Signing.PrivateKey
    do {
        key = try SecureEnclave.P256.Signing.PrivateKey(
            dataRepresentation: Data(bytes: handle, count: length), authenticationContext: context)
    } catch {
        return unusable
    }
    do {
        let signed = try key.signature(for: Data(bytes: message, count: 32))
        return copy(signed.rawRepresentation, signature, 64) ? done : unavailable
    } catch {
        return systemFailure(error)
    }
}

/// Sign a batch of 32-byte messages as `r || s` (64 bytes each), with one
/// presence check and one key/context scoped to this batch.
@_cdecl("dmsg_se_sign_batch")
public func dmsgSeSignBatch(
    _ handle: UnsafePointer<UInt8>, _ length: Int, _ message: UnsafePointer<UInt8>,
    _ messageCount: Int, _ reason: UnsafePointer<CChar>, _ signature: UnsafeMutablePointer<UInt8>
) -> Int32 {
    guard messageCount > 0 && messageCount <= 32 else { return unusable }
    let (context, outcome) = present(.useKeySign, reason)
    guard let context else { return outcome }
    let key: SecureEnclave.P256.Signing.PrivateKey
    do {
        key = try SecureEnclave.P256.Signing.PrivateKey(
            dataRepresentation: Data(bytes: handle, count: length), authenticationContext: context)
    } catch {
        return unusable
    }
    do {
        for index in 0..<messageCount {
            let signed = try key.signature(for: Data(bytes: message.advanced(by: index * 32), count: 32))
            guard copy(signed.rawRepresentation, signature.advanced(by: index * 64), 64) else { return unavailable }
        }
        return done
    } catch {
        return systemFailure(error)
    }
}

/// Ask the person once about an approval; the answer, an evaluated
/// context, then signs with approval keys (`dmsg_se_sign_with`) until
/// `dmsg_se_release` withdraws it.
@_cdecl("dmsg_se_authorize")
public func dmsgSeAuthorize(
    _ reason: UnsafePointer<CChar>, _ biometryOnly: Int32,
    _ answer: UnsafeMutablePointer<UnsafeMutableRawPointer?>
) -> Int32 {
    let (context, outcome) = present(.useKeySign, reason, biometryOnly)
    guard let context else { return outcome }
    answer.pointee = Unmanaged.passRetained(context).toOpaque()
    return done
}

/// Sign one 32-byte message as `r || s` with the key `handle`, under the
/// person's answer from `dmsg_se_authorize`.
@_cdecl("dmsg_se_sign_with")
public func dmsgSeSignWith(
    _ answer: UnsafeMutableRawPointer, _ handle: UnsafePointer<UInt8>, _ length: Int,
    _ message: UnsafePointer<UInt8>, _ signature: UnsafeMutablePointer<UInt8>
) -> Int32 {
    let context = Unmanaged<LAContext>.fromOpaque(answer).takeUnretainedValue()
    let key: SecureEnclave.P256.Signing.PrivateKey
    do {
        key = try SecureEnclave.P256.Signing.PrivateKey(
            dataRepresentation: Data(bytes: handle, count: length), authenticationContext: context)
    } catch {
        return unusable
    }
    do {
        let signed = try key.signature(for: Data(bytes: message, count: 32))
        return copy(signed.rawRepresentation, signature, 64) ? done : unavailable
    } catch {
        return systemFailure(error)
    }
}

/// Withdraw an answer of `dmsg_se_authorize`.
@_cdecl("dmsg_se_release")
public func dmsgSeRelease(_ answer: UnsafeMutableRawPointer) {
    let context = Unmanaged<LAContext>.fromOpaque(answer).takeRetainedValue()
    context.invalidate()
}

/// Ask the person to confirm an approval, with Touch ID or the login
/// password: 0 confirmed, 2 declined, 1 no way to ask. A question left
/// unanswered is withdrawn and counts as declined.
@_cdecl("dmsg_se_confirm")
public func dmsgSeConfirm(_ reason: UnsafePointer<CChar>) -> Int32 {
    let context = LAContext()
    guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: nil) else {
        return unavailable
    }
    let outcome = Outcome()
    let answered = DispatchSemaphore(value: 0)
    context.evaluatePolicy(
        .deviceOwnerAuthentication, localizedReason: String(cString: reason)
    ) { success, error in
        if success {
            outcome.code = done
        } else if let error = error as? LAError,
            [
                .userCancel, .appCancel, .systemCancel, .authenticationFailed, .userFallback,
                .invalidContext,
            ].contains(error.code)
        {
            outcome.code = declined
        }
        answered.signal()
    }
    if answered.wait(timeout: .now() + patience) == .timedOut {
        context.invalidate()
        answered.wait()
    }
    return outcome.code
}
