import AuthenticationServices
import Foundation

extension ASCredentialServiceIdentifier {
    /// Bind a remembered choice to the first (most specific) target supplied by iOS.
    var loginServiceKey: String? {
        let value = identifier
        if #available(iOS 26.2, *), type == .app {
            return value.isEmpty ? nil : "app:" + value
        }
        switch type {
        case .domain, .URL:
            let url = type == .domain ? "https://" + value : value
            guard let parts = URLComponents(string: url), let scheme = parts.scheme?.lowercased(),
                  scheme == "https", let host = parts.host?.lowercased(), !host.isEmpty,
                  parts.user == nil, parts.password == nil else { return nil }
            let port = parts.port ?? 443
            return "\(scheme)://\(host):\(port)"
        default: return nil
        }
    }
}

extension CredentialExport {
    static func sections(entries: [CredentialExport], services: [String], recent: [String], query: String) -> [(title: String?, entries: [CredentialExport])] {
        let shown = query.isEmpty ? entries : entries.filter { $0.contains(query) }
        let remembered = Dictionary(uniqueKeysWithValues: shown.compactMap { entry in entry.itemID.map { ($0, entry) } })
        let used = recent.compactMap { remembered[$0] }
        let usedIDs = Set(used.map(\.id))
        let remaining = shown.filter { !usedIDs.contains($0.id) }.sorted { lhs, rhs in
            let titles = lhs.title.localizedStandardCompare(rhs.title)
            return titles == .orderedSame ? lhs.username.localizedStandardCompare(rhs.username) == .orderedAscending : titles == .orderedAscending
        }
        let matching = remaining.filter { entry in services.contains { entry.matches(service: $0) } }
        let matchingIDs = Set(matching.map(\.id))
        let other = remaining.filter { !matchingIDs.contains($0.id) }
        var sections: [(title: String?, entries: [CredentialExport])] = []
        if !used.isEmpty { sections.append((String(localized: "Previously used"), used)) }
        if !matching.isEmpty { sections.append((String(localized: "For this site"), matching)) }
        if !other.isEmpty { sections.append((sections.isEmpty ? nil : String(localized: "Other logins"), other)) }
        return sections
    }
}
