import CryptoKit
import DeviceCheck
import Foundation
@preconcurrency import DMsgCore

/// App Attest is separate from workspace unlock and human approval.
@MainActor
final class NativeApple {
    struct Challenge: Decodable { let challengeId: String; let challenge: String; let clientData: String; let expiresAt: UInt64 }
    private var working = false
    private var idleWaiters: [CheckedContinuation<Void, Never>] = []

    private func keyPath(files: PrivateFiles, account: String) -> URL {
        let environment = Bundle.main.object(forInfoDictionaryKey: "DMsgAttestEnvironment") as? String ?? "production"
        return files.root.appending(path: "apple").appending(path: account + "." + environment + ".json")
    }

    private func key(engine: MobileEngine, files: PrivateFiles, account: String) async throws -> String {
        guard DCAppAttestService.shared.isSupported else { throw MobileError.Failure(code: "APP_INTEGRITY_UNAVAILABLE") }
        let session = try await engine.sessionState()
        guard session.account == account, session.state == "unlocked", let device = session.deviceId else { throw MobileError.Failure(code: "LOCKED") }
        let directory = files.root.appending(path: "apple")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.complete])
        struct Status: Decodable { let registered: Bool }
        return try await AppAttestation.key(device: device, path: keyPath(files: files, account: account), service: .init(
            status: { key in
                let status: Status = try self.decode(try await engine.attestationStatus(keyId: key))
                return status.registered
            },
            challenge: { key in try self.decode(try await engine.attestationChallenge(installation: files.installation, keyId: key)) },
            register: { key, challenge, object in
                let status: Status = try self.decode(try await engine.registerAttestation(installation: files.installation, keyId: key, challengeId: challenge, object: object))
                guard status.registered else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
            }
        ))
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
        try await withKey(engine: engine, files: files, account: account) { key in
            let input = PushRegistration(installation: files.installation, token: token, environment: environment)
            let challenge: Challenge = try decode(try await engine.pushChallenge(registration: input, keyId: key))
            let proof = try await assert(challenge, key: key)
            return try await engine.registerPush(registration: input, proof: proof, requestId: Self.requestID())
        }
    }

    func preparePurchase(engine: MobileEngine, files: PrivateFiles, account: String) async throws {
        try await withKey(engine: engine, files: files, account: account) { _ in () }
    }

    private func withKey<T>(engine: MobileEngine, files: PrivateFiles, account: String,
                            operation: (String) async throws -> T) async throws -> T {
        while working { await withCheckedContinuation { idleWaiters.append($0) } }
        try Task.checkCancellation()
        working = true; defer {
            working = false; let waiting = idleWaiters; idleWaiters.removeAll(); for waiter in waiting { waiter.resume() }
        }
        for attempt in 0..<2 {
            var selected: String?
            do {
                let key = try await key(engine: engine, files: files, account: account)
                selected = key
                try Task.checkCancellation()
                return try await operation(key)
            } catch {
                if let selected {
                    try AppAttestation.discardInvalidKey(error, id: selected, path: keyPath(files: files, account: account))
                }
                // A restored installation can retain a registered key ID whose
                // private key no longer exists. Recover once within this action.
                guard attempt == 0, AppAttestation.invalidKey(error) else { throw error }
            }
        }
        throw MobileError.Failure(code: "APP_INTEGRITY_UNAVAILABLE")
    }

    func submitPurchase(engine: MobileEngine, files: PrivateFiles, account: String, jws: String) async throws {
        try await withKey(engine: engine, files: files, account: account) { key in
            let challenge: Challenge = try decode(try await engine.purchaseChallenge(installation: files.installation, jws: jws, keyId: key))
            let proof = try await assert(challenge, key: key)
            let result = try await engine.submitPurchase(installation: files.installation, jws: jws, proof: proof, requestId: Self.requestID())
            try PurchaseModel.requireApplied(Data(result.json.utf8))
        }
    }

    nonisolated static func requestID() -> String { (UUID().uuidString + UUID().uuidString).replacingOccurrences(of: "-", with: "").lowercased() }
    private func decode<T: Decodable>(_ snapshot: ObjectSnapshot) throws -> T {
        try Task.checkCancellation()
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(T.self, from: Data(snapshot.json.utf8))
    }
}
