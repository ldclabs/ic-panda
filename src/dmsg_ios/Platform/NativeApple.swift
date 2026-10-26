import CryptoKit
import DeviceCheck
import Foundation
@preconcurrency import DMsgCore

/// App Attest is separate from workspace unlock and human approval.
@MainActor
final class NativeApple {
    struct Challenge: Decodable { let challengeId: String; let challenge: String; let clientData: String; let expiresAt: UInt64 }
    private struct KeyRecord: Codable { let id: String; let device: String; var challenge: String?; var object: String?; var expires: UInt64? }
    private var working = false
    private var idleWaiters: [CheckedContinuation<Void, Never>] = []

    private func key(engine: MobileEngine, files: PrivateFiles, account: String) async throws -> String {
        guard DCAppAttestService.shared.isSupported else { throw MobileError.Failure(code: "APP_INTEGRITY_UNAVAILABLE") }
        let session = try await engine.sessionState()
        guard session.account == account, session.state == "unlocked", let device = session.deviceId else { throw MobileError.Failure(code: "LOCKED") }
        let directory = files.root.appending(path: "apple")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.complete])
        let attestEnvironment = Bundle.main.object(forInfoDictionaryKey: "DMsgAttestEnvironment") as? String ?? "production"
        let path = directory.appending(path: account + "." + attestEnvironment + ".json")
        var record = (try? Data(contentsOf: path)).flatMap { try? JSONDecoder().decode(KeyRecord.self, from: $0) }
        if record?.device != device { record = nil }
        if let old = record {
            struct Status: Decodable { let registered: Bool }
            let snapshot = try await engine.attestationStatus(keyId: old.id)
            let status: Status = try decode(snapshot)
            if status.registered { return old.id }
            if let expires = old.expires, Double(expires) <= Date.now.timeIntervalSince1970 * 1000 { record = nil }
        }
        if record == nil {
            let id: String = try await withCheckedThrowingContinuation { continuation in
                DCAppAttestService.shared.generateKey { id, error in
                    if let id { continuation.resume(returning: id) }
                    else { continuation.resume(throwing: error ?? CocoaError(.coderInvalidValue)) }
                }
            }
            record = KeyRecord(id: id, device: device)
            try JSONEncoder().encode(record).write(to: path, options: [.atomic, .completeFileProtection])
        }
        guard var prepared = record else { throw CocoaError(.coderInvalidValue) }
        if prepared.object == nil {
            let challenge: Challenge = try decode(try await engine.attestationChallenge(installation: files.installation, keyId: prepared.id))
            guard let nonce = Data(base64Encoded: challenge.challenge) else { throw CocoaError(.coderInvalidValue) }
            let object: Data
            do {
                object = try await withCheckedThrowingContinuation { continuation in
                    DCAppAttestService.shared.attestKey(prepared.id, clientDataHash: Data(SHA256.hash(data: nonce))) { data, error in
                        if let data { continuation.resume(returning: data) }
                        else { continuation.resume(throwing: error ?? CocoaError(.coderInvalidValue)) }
                    }
                }
            } catch {
                // A one-time attestation can succeed at Apple before its local
                // result reaches disk. A retired key must not strand retries.
                let failure = error as NSError
                if failure.domain == DCErrorDomain && failure.code == DCError.Code.invalidKey.rawValue { try? FileManager.default.removeItem(at: path) }
                throw error
            }
            prepared.challenge = challenge.challengeId; prepared.object = object.base64EncodedString(); prepared.expires = challenge.expiresAt
            try JSONEncoder().encode(prepared).write(to: path, options: [.atomic, .completeFileProtection])
        }
        guard let challenge = prepared.challenge, let object = prepared.object else { throw CocoaError(.coderInvalidValue) }
        _ = try await engine.registerAttestation(installation: files.installation, keyId: prepared.id, challengeId: challenge, object: object)
        prepared.challenge = nil; prepared.object = nil; prepared.expires = nil
        try JSONEncoder().encode(prepared).write(to: path, options: [.atomic, .completeFileProtection])
        return prepared.id
    }

    private func assert(_ challenge: Challenge, key: String) async throws -> AppAssertion {
        guard let data = Data(base64Encoded: challenge.clientData) else { throw CocoaError(.coderInvalidValue) }
        let assertion: Data = try await withCheckedThrowingContinuation { continuation in
            DCAppAttestService.shared.generateAssertion(key, clientDataHash: Data(SHA256.hash(data: data))) { value, error in
                if let value { continuation.resume(returning: value) }
                else { continuation.resume(throwing: error ?? CocoaError(.coderInvalidValue)) }
            }
        }
        return AppAssertion(challengeId: challenge.challengeId, keyId: key, object: assertion.base64EncodedString())
    }

    func registerPush(engine: MobileEngine, files: PrivateFiles, account: String, token: String, environment: String) async throws -> ObjectSnapshot {
        while working { await withCheckedContinuation { idleWaiters.append($0) } }
        try Task.checkCancellation()
        working = true; defer {
            working = false; let waiting = idleWaiters; idleWaiters.removeAll(); for waiter in waiting { waiter.resume() }
        }
        let key = try await key(engine: engine, files: files, account: account)
        let input = PushRegistration(installation: files.installation, token: token, environment: environment)
        let challenge: Challenge = try decode(try await engine.pushChallenge(registration: input, keyId: key))
        let proof = try await assert(challenge, key: key)
        return try await engine.registerPush(registration: input, proof: proof, requestId: Self.requestID())
    }

    func submitPurchase(engine: MobileEngine, files: PrivateFiles, account: String, jws: String) async throws {
        while working { await withCheckedContinuation { idleWaiters.append($0) } }
        try Task.checkCancellation()
        working = true; defer {
            working = false; let waiting = idleWaiters; idleWaiters.removeAll(); for waiter in waiting { waiter.resume() }
        }
        let key = try await key(engine: engine, files: files, account: account)
        let challenge: Challenge = try decode(try await engine.purchaseChallenge(installation: files.installation, jws: jws, keyId: key))
        let proof = try await assert(challenge, key: key)
        _ = try await engine.submitPurchase(installation: files.installation, jws: jws, proof: proof, requestId: Self.requestID())
    }

    static func requestID() -> String { (UUID().uuidString + UUID().uuidString).replacingOccurrences(of: "-", with: "").lowercased() }
    private func decode<T: Decodable>(_ snapshot: ObjectSnapshot) throws -> T {
        try Task.checkCancellation()
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(T.self, from: Data(snapshot.json.utf8))
    }
}
