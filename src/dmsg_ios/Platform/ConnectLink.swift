import Foundation
import Observation

struct ConnectLink: Equatable, Sendable, Identifiable {
    enum Destination: String, Sendable { case pair, request }
    let destination: Destination
    let account: String?
    let request: String?
    var id: String { destination.rawValue + (account ?? "") + (request ?? "") }
    init?(_ url: URL) {
        guard let parts = URLComponents(url: url, resolvingAgainstBaseURL: false), parts.user == nil, parts.password == nil,
              parts.port == nil, parts.fragment == nil else { return nil }
        let path: String
        if parts.scheme == "https", parts.host == "connect.dmsg.net", parts.path.hasPrefix("/app/") { path = String(parts.path.dropFirst(5)) }
        else if parts.scheme == "dmsg", parts.host == "connect", parts.path.hasPrefix("/") { path = String(parts.path.dropFirst()) }
        else { return nil }
        guard let destination = Destination(rawValue: path) else { return nil }
        let query = parts.queryItems ?? []
        guard Set(query.map(\.name)).count == query.count, query.allSatisfy({ ["account","request"].contains($0.name) && $0.value != nil }) else { return nil }
        let account = query.first(where: { $0.name == "account" })?.value
        let request = query.first(where: { $0.name == "request" })?.value
        if let account, account.range(of: "^[0-9a-v]{20}$", options: .regularExpression) == nil { return nil }
        if destination == .request {
            guard account != nil, let request, request.range(of: "^[0-9a-f]{64}$", options: .regularExpression) != nil else { return nil }
        } else if request != nil { return nil }
        self.destination = destination; self.account = account; self.request = request
    }
}

@MainActor @Observable
final class ConnectNavigation {
    static let shared = ConnectNavigation()
    var pending: ConnectLink?
}
