import Foundation
import Network
import OSLog
@preconcurrency import DMsgCore

enum ContentSyncPolicy {
    /// A network failure retries by itself; it is news only after about a minute.
    static func needsAttention(_ status: ContentSyncStatus?) -> Bool {
        guard let status, let error = status.error else { return false }
        let network = ["UNAVAILABLE", "NETWORK_UNAVAILABLE", "EXECUTION_UNKNOWN", "RESULT_EXPIRED", "POLICY_STALE"]
        return !network.contains(error) || status.failures >= 4
    }
}

/// Whether the device has a usable network path, on every change.
enum NetworkPath {
    static func updates() -> AsyncStream<Bool> {
        AsyncStream { continuation in
            let monitor = NWPathMonitor()
            monitor.pathUpdateHandler = { continuation.yield($0.status == .satisfied) }
            continuation.onTermination = { _ in monitor.cancel() }
            monitor.start(queue: DispatchQueue(label: "net.dmsg.ios.network-path"))
        }
    }
}

/// The sync driver's callbacks, delivered to the session in order.
final class ContentSyncBridge: ContentSyncObserver, Sendable {
    enum Event: Sendable {
        case status(ContentSyncStatus)
        case passed(changed: Bool, error: String?)
    }

    private let events: AsyncStream<Event>.Continuation

    init(_ events: AsyncStream<Event>.Continuation) { self.events = events }

    func status(status: ContentSyncStatus) { events.yield(.status(status)) }

    func passed(changed: Bool, error: String?) { events.yield(.passed(changed: changed, error: error)) }
}

/// The hint channel's callbacks, delivered to the session in order.
final class HintBridge: HintObserver, Sendable {
    enum Event: Sendable {
        case connected(Bool)
        case hint(kind: String, resource: String?, news: Bool)
    }

    private let events: AsyncStream<Event>.Continuation

    init(_ events: AsyncStream<Event>.Continuation) { self.events = events }

    func connected(connected: Bool) { events.yield(.connected(connected)) }

    func hint(kind: String, resource: String?, news: Bool) {
        events.yield(.hint(kind: kind, resource: resource, news: news))
    }
}

/// A login as the AutoFill snapshot holds it.
struct AutoFillEntry: Decodable, Sendable {
    let title: String; let username: String; let password: String; let url: String

    static func origin(_ text: String) -> String {
        guard var url = URLComponents(string: text), ["https", "http"].contains(url.scheme?.lowercased()), url.host != nil else { return "" }
        url.user = nil; url.password = nil; url.path = ""; url.query = nil; url.fragment = nil
        return url.string ?? ""
    }
}

struct AutoFillRefresh: Decodable, Sendable {
    let changed: Bool
    let entries: [AutoFillEntry]
    let biometryOnly: Bool
    let digest: String
}

enum ContentSyncEvent: String { case background, locked, disabled, sessionChanged = "session_changed" }

enum ContentSyncLog {
    private static let logger = Logger(subsystem: "net.dmsg.ios", category: "content-sync")

    static func record(_ event: ContentSyncEvent) {
        logger.info("lifecycle=\(event.rawValue, privacy: .public)")
    }
}
