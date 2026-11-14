import CryptoKit
import DeviceCheck
import Foundation

/// Durable enrollment of a one-time App Attest key. The original challenge is
/// saved before contacting Apple; a retry must use the same key and nonce.
@MainActor
enum AppAttestation {
    private struct KeyRecord: Codable {
        let id: String
        let device: String
        var challenge: String?
        var nonce: String?
        var object: String?
        var expires: UInt64?
    }

    struct Service {
        var status: (String) async throws -> Bool
        var challenge: (String) async throws -> NativeApple.Challenge
        var register: (String, String, String) async throws -> Void
        var generate: () async throws -> String = {
            try await withCheckedThrowingContinuation { continuation in
                DCAppAttestService.shared.generateKey { id, error in
                    if let id { continuation.resume(returning: id) }
                    else { continuation.resume(throwing: error ?? CocoaError(.coderInvalidValue)) }
                }
            }
        }
        var attest: (String, Data) async throws -> Data = { key, hash in
            try await withCheckedThrowingContinuation { continuation in
                DCAppAttestService.shared.attestKey(key, clientDataHash: hash) { data, error in
                    if let data { continuation.resume(returning: data) }
                    else { continuation.resume(throwing: error ?? CocoaError(.coderInvalidValue)) }
                }
            }
        }
    }

    static func key(device: String, path: URL, service: Service) async throws -> String {
        try Task.checkCancellation()
        var record = (try? Data(contentsOf: path)).flatMap { try? JSONDecoder().decode(KeyRecord.self, from: $0) }
        if record?.device != device { record = nil }
        if var old = record {
            // Resolve a lost registration response before expiring its challenge
            // or calling Apple's one-time attestKey again.
            if try await service.status(old.id) {
                try clearProof(&old, path: path)
                return old.id
            }
            if let expires = old.expires, Double(expires) <= Date.now.timeIntervalSince1970 * 1000 { record = nil }
        }
        if record == nil {
            record = KeyRecord(id: try await service.generate(), device: device)
            try save(record!, path: path)
        }
        guard var prepared = record else { throw CocoaError(.coderInvalidValue) }
        if prepared.object == nil {
            if prepared.nonce == nil {
                let challenge = try await service.challenge(prepared.id)
                prepared.challenge = challenge.challengeId
                prepared.nonce = challenge.challenge
                prepared.expires = challenge.expiresAt
                try save(prepared, path: path)
            }
            guard let nonce = prepared.nonce.flatMap({ Data(base64Encoded: $0) }), nonce.count == 32 else {
                throw CocoaError(.coderInvalidValue)
            }
            do {
                let object = try await service.attest(prepared.id, Data(SHA256.hash(data: nonce)))
                prepared.object = object.base64EncodedString()
                try save(prepared, path: path)
            } catch {
                // If Apple consumed the key before a local result was saved,
                // invalidKey retires only that record. Transient failures keep it.
                try discardInvalidKey(error, id: prepared.id, path: path)
                throw error
            }
        }
        guard let challenge = prepared.challenge, let object = prepared.object else { throw CocoaError(.coderInvalidValue) }
        do {
            try Task.checkCancellation()
            try await service.register(prepared.id, challenge, object)
        } catch {
            try Task.checkCancellation()
            // Registration may have committed even if both submission replies
            // were lost. This read is authoritative for the current device.
            guard (try? await service.status(prepared.id)) == true else { throw error }
        }
        try clearProof(&prepared, path: path)
        return prepared.id
    }

    private static func save(_ record: KeyRecord, path: URL) throws {
        try Task.checkCancellation()
        try JSONEncoder().encode(record).write(to: path, options: [.atomic, .completeFileProtection])
    }

    private static func clearProof(_ record: inout KeyRecord, path: URL) throws {
        try Task.checkCancellation()
        guard record.challenge != nil || record.object != nil || record.nonce != nil || record.expires != nil else { return }
        record.challenge = nil; record.nonce = nil; record.object = nil; record.expires = nil
        try save(record, path: path)
    }

    static func invalidKey(_ error: Error) -> Bool {
        let failure = error as NSError
        return failure.domain == DCErrorDomain && failure.code == DCError.Code.invalidKey.rawValue
    }

    static func discardInvalidKey(_ error: Error, id: String, path: URL) throws {
        guard invalidKey(error), let data = try? Data(contentsOf: path),
              let record = try? JSONDecoder().decode(KeyRecord.self, from: data), record.id == id else { return }
        try FileManager.default.removeItem(at: path)
    }
}
