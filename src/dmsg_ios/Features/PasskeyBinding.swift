import Foundation
import Observation
@preconcurrency import DMsgCore

/// Keep the system's response while the same binding is being confirmed.
/// Retrying never calls the authenticator again; leaving the flow discards it.
@MainActor @Observable
final class PasskeyBinding {
    private struct Attempt {
        let start: PasskeyStart
        let response: NativePasskeyResponse
    }
    private var attempt: Attempt?
    private var revision: UInt64 = 0
    private(set) var working = false
    var hasSavedCredential: Bool { attempt != nil }

    func run(
        begin: () async throws -> PasskeyStart,
        authenticate: (PasskeyStart) async throws -> NativePasskeyResponse,
        complete: (PasskeyStart, NativePasskeyResponse) async throws -> ObjectSnapshot
    ) async throws -> ObjectSnapshot {
        guard !working else { throw CancellationError() }
        working = true
        let current = revision
        defer { if revision == current { working = false } }
        do {
            if attempt == nil {
                let start = try await begin()
                guard current == revision, !Task.isCancelled else { throw CancellationError() }
                let response = try await authenticate(start)
                guard current == revision, !Task.isCancelled else { throw CancellationError() }
                attempt = Attempt(start: start, response: response)
            }
            guard let attempt else { throw CancellationError() }
            let result = try await complete(attempt.start, attempt.response)
            guard current == revision, !Task.isCancelled else { throw CancellationError() }
            self.attempt = nil
            return result
        } catch {
            if current == revision && !Self.canRetry(error) { attempt = nil }
            throw error
        }
    }

    func cancel() {
        revision &+= 1
        attempt = nil
        working = false
    }

    private static func canRetry(_ error: Error) -> Bool {
        guard !Task.isCancelled, let error = error as? MobileError, case .Failure(let code) = error else { return false }
        return ["NETWORK_UNAVAILABLE", "UNAVAILABLE", "EXECUTION_UNKNOWN", "SYNC_BUSY", "HARDWARE_UNAVAILABLE", "LOCAL_STORAGE_UNAVAILABLE"].contains(code)
    }
}
