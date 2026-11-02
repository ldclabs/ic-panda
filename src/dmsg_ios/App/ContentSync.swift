import Foundation
import OSLog
@preconcurrency import DMsgCore

enum ContentSyncPolicy {
    static func isDue(status: ContentSyncStatus?, edited: Bool, idleDue: Bool, now: UInt64) -> Bool {
        guard status?.running != true else { return false }
        if let status, status.retryRequired {
            return status.retryAtMs.map { $0 <= now } ?? false
        }
        return edited || idleDue
    }
}

enum ContentSyncEvent: String { case inactive, background, locked, disabled, sessionChanged = "session_changed" }

enum ContentSyncLog {
    private static let logger = Logger(subsystem: "net.dmsg.ios", category: "content-sync")

    static func record(_ event: ContentSyncEvent) {
        logger.info("lifecycle=\(event.rawValue, privacy: .public)")
    }
}
