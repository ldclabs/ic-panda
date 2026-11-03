import Foundation
import Observation

struct ChannelInvitationSummary: Decodable {
    let format: String
    let channel: String
    let account: String
    let name: String
    let expiresAt: UInt64

    static func parse(_ input: String) -> Self? {
        let input = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard input.utf8.count <= 16 * 1024 else { return nil }
        let data: Data?
        if input.hasPrefix("dmsg://") {
            guard let parts = URLComponents(string: input), parts.scheme == "dmsg", parts.host == "channel", parts.path == "/join",
                  parts.user == nil, parts.password == nil, parts.port == nil, parts.query == nil, let fragment = parts.fragment,
                  fragment.range(of: "^[A-Za-z0-9_-]+$", options: .regularExpression) != nil else { return nil }
            var base64 = fragment.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
            base64 += String(repeating: "=", count: (4 - base64.count % 4) % 4)
            data = Data(base64Encoded: base64)
        } else { data = input.data(using: .utf8) }
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let data, let value = try? decoder.decode(Self.self, from: data), value.format == "dmsg-channel-invitation/1",
              validMessageAccount(value.account), value.channel.range(of: "^[0-9a-f]{64}$", options: .regularExpression) != nil else { return nil }
        return value
    }
}

struct ChannelInvitationLink: Identifiable, Equatable {
    let value: String
    var id: String { value }
    init?(_ url: URL) {
        guard url.scheme == "dmsg", url.host == "channel", ChannelInvitationSummary.parse(url.absoluteString) != nil else { return nil }
        value = url.absoluteString
    }
}

@MainActor @Observable
final class ChannelNavigation {
    static let shared = ChannelNavigation()
    var pending: ChannelInvitationLink?
}
