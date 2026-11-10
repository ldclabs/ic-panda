import Foundation
import OSLog

/// Fixed phases, elapsed time and allowlisted failure codes; no message or account data.
@MainActor
enum MessageTiming {
    enum Phase: String { case localPage = "local_page", queueVisible = "queue_visible", ackVisible = "ack_visible" }
    enum Operation: String { case send, refresh }
    private static let logger = Logger(subsystem: "net.dmsg.ios", category: "message-timing")

    static func record(_ phase: Phase, since: ContinuousClock.Instant) {
        let parts = since.duration(to: .now).components
        let milliseconds = max(0, min(86_400_000, parts.seconds * 1000 + parts.attoseconds / 1_000_000_000_000_000))
        logger.info("\(phase.rawValue, privacy: .public) elapsed_ms=\(milliseconds, privacy: .public)")
    }

    static func failure(_ operation: Operation, code: String) {
        let code = MessageFailure.diagnosticCode(code)
        logger.error("\(operation.rawValue, privacy: .public)_failed code=\(code, privacy: .public)")
    }
}
