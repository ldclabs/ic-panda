import Foundation
import Observation
@preconcurrency import DMsgCore

struct InboxSnapshot {
    let letters: [InboxLetter]
    let complete: Bool
}

@MainActor @Observable
final class InboxLoading {
    private(set) var letters: [InboxLetter] = []
    private(set) var known = false
    private(set) var refreshing = false
    private(set) var failure: String?
    private var revision: UInt64 = 0

    typealias Read = @MainActor (Bool) async throws -> InboxSnapshot

    func cached(read: Read) async {
        let before = revision
        do {
            let snapshot = try await read(false)
            guard !Task.isCancelled, before == revision else { return }
            apply(snapshot)
        } catch {
            guard !Task.isCancelled, before == revision, !Self.cancelled(error) else { return }
            failure = Self.message(error)
        }
    }

    func refresh(read: Read) async {
        guard !refreshing, !Task.isCancelled else { return }
        refreshing = true
        defer { refreshing = false }
        do {
            let snapshot = try await read(true)
            guard !Task.isCancelled else { return }
            apply(snapshot)
            failure = nil
        } catch {
            guard !Task.isCancelled, !Self.cancelled(error) else { return }
            // Earlier verified pages remain readable even if a later page fails.
            await cached(read: read)
            guard !Task.isCancelled else { return }
            failure = Self.message(error)
        }
    }

    private func apply(_ snapshot: InboxSnapshot) {
        revision &+= 1
        letters = snapshot.letters.sorted {
            $0.createdAt == $1.createdAt ? $0.id < $1.id : $0.createdAt > $1.createdAt
        }
        known = snapshot.complete || !letters.isEmpty
    }

    private static func cancelled(_ error: Error) -> Bool {
        if error is CancellationError { return true }
        if case MobileError.Failure(let code) = error { return code == "LOCKED" || code == "CANCELLED" }
        return false
    }

    static func message(_ error: Error) -> String {
        if case MobileError.Failure(let code) = error {
            switch code {
            case "RECOVERY_INCOMPLETE":
                return String(localized: "The inbox key has not synced to this device. Sync the device that enabled receiving, then retry here.")
            case "INTEGRITY_FAILED":
                return String(localized: "A record or device key could not be verified. Your workspace has not been erased.")
            case "NETWORK_UNAVAILABLE", "UNAVAILABLE":
                return String(localized: "Connection unavailable. Your saved content stays on this device.")
            default:
                return String(localized: "The operation could not be completed.") + " (\(code))"
            }
        }
        return String(localized: "The operation could not be completed. Please try again.")
    }
}
