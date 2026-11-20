import AuthenticationServices
import Foundation
import Observation

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

@MainActor @Observable
final class CredentialPickerSession {
    enum Page {
        case loading, list, confirmation(CredentialExport), message(String), finished
    }

    private(set) var page: Page = .loading
    private(set) var configuring = false
    var query = ""
    private var services: [ASCredentialServiceIdentifier] = []
    private var requested: ASPasswordCredentialIdentity?
    private var entries: [CredentialExport] = []
    private var recent: [String] = []
    private var opened: OpenExport?
    private var authenticatedAt = ContinuousClock.now
    @ObservationIgnored private(set) var operation: Task<Void, Never>?

    private let store: () throws -> SystemExports
    private let authenticate: (SystemExports) async throws -> (OpenExport, [CredentialExport])
    private let now: () -> ContinuousClock.Instant
    private let complete: (ASPasswordCredential, ASPasswordCredentialIdentity?) -> Void

    init(store: @escaping () throws -> SystemExports = { try SystemExports(.credentials) },
         authenticate: @escaping (SystemExports) async throws -> (OpenExport, [CredentialExport]) = { store in
             try await Task.detached {
                 let opened = try store.authenticate()
                 return (opened, try store.credentialMetadata(opened))
             }.value
         },
         now: @escaping () -> ContinuousClock.Instant = { .now },
         complete: @escaping (ASPasswordCredential, ASPasswordCredentialIdentity?) -> Void) {
        self.store = store
        self.authenticate = authenticate
        self.now = now
        self.complete = complete
    }

    var destination: String? {
        guard let target = services.first, !target.identifier.isEmpty else { return nil }
        if #available(iOS 26.2, *), target.type == .app, let name = target.displayName,
           !name.isEmpty, name != target.identifier { return "\(name) (\(target.identifier))" }
        return target.identifier
    }

    var sections: [(title: String?, entries: [CredentialExport])] {
        CredentialExport.sections(entries: entries, services: services.map(\.identifier), recent: recent,
                                  query: query.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    func canRemember(_ entry: CredentialExport) -> Bool {
        services.first?.loginServiceKey != nil && entry.itemID != nil
    }

    func configure() {
        close()
        configuring = true
        do {
            _ = try store().header()
            page = .message(String(localized: "Choose a login when an app or website asks for a password."))
        } catch { fail(error) }
    }

    func start(services: [ASCredentialServiceIdentifier], requested: ASPasswordCredentialIdentity? = nil) {
        close()
        self.services = services
        self.requested = requested
        unlock()
    }

    func retry() {
        guard case .message = page, !configuring else { return }
        unlock()
    }

    private func unlock() {
        page = .loading
        operation = Task {
            do {
                let store = try store()
                let header = try await load(store)
                if let requested,
                   let entry = CredentialExport.requested(requested.recordIdentifier, user: requested.user, in: header, entries: entries),
                   let id = entry.itemID, recent.contains(id) {
                    try deliver(entry, remember: true, store: store)
                } else {
                    requested = nil
                    page = .list
                }
            } catch { if !Task.isCancelled { fail(error) } }
        }
    }

    private func load(_ store: SystemExports) async throws -> ExportHeader {
        try Task.checkCancellation()
        clearCatalog()
        let (opened, entries) = try await authenticate(store)
        try Task.checkCancellation()
        recent = try store.recentLogins(account: opened.header.accountHash, service: services.first?.loginServiceKey)
        self.opened = opened
        self.entries = entries
        authenticatedAt = now()
        return opened.header
    }

    func choose(_ entry: CredentialExport) {
        guard case .list = page else { return }
        if let id = entry.itemID, recent.contains(id) {
            submit(entry, remember: true, previouslyUsed: true)
        } else {
            page = .confirmation(entry)
        }
    }

    func back() {
        guard case .confirmation = page else { return }
        page = .list
    }

    func confirm(remember: Bool) {
        guard case let .confirmation(entry) = page, !remember || canRemember(entry) else { return }
        submit(entry, remember: remember, previouslyUsed: false)
    }

    private func submit(_ entry: CredentialExport, remember: Bool, previouslyUsed: Bool) {
        guard let previous = opened?.header else { return }
        page = .loading
        operation = Task {
            do {
                try Task.checkCancellation()
                let store = try store()
                var selected = entry
                let current = try store.header()
                if current.id != previous.id || authenticatedAt.duration(to: now()) >= .seconds(60) {
                    let header = try await load(store)
                    // Carry the user's decision through reauthentication only for the same
                    // account and displayed login. A refreshed catalog has new file IDs.
                    guard header.accountHash == previous.accountHash else { query = ""; page = .list; return }
                    guard let refreshed = entries.first(where: { candidate in
                        let sameItem = header.id == previous.id ? candidate.id == entry.id
                            : entry.itemID != nil && candidate.itemID == entry.itemID
                        return sameItem && candidate.title == entry.title && candidate.username == entry.username && candidate.origin == entry.origin
                    }) else { page = .list; return }
                    if previouslyUsed, let id = refreshed.itemID, !recent.contains(id) {
                        page = .confirmation(refreshed)
                        return
                    }
                    selected = refreshed
                }
                try deliver(selected, remember: remember, store: store)
            } catch { if !Task.isCancelled { fail(error) } }
        }
    }

    private func deliver(_ entry: CredentialExport, remember: Bool, store: SystemExports) throws {
        guard let opened else { throw ExportAccessError.notConfigured }
        let password = try store.credential(entry.id, opened: opened)
        var identity: ASPasswordCredentialIdentity?
        if remember, let target = services.first, let key = target.loginServiceKey, let id = entry.itemID {
            try store.remember(id, service: key, opened: opened)
            let service = target.type == .URL ? ASCredentialServiceIdentifier(identifier: key, type: .URL) : target
            identity = ASPasswordCredentialIdentity(serviceIdentifier: service, user: entry.username,
                                                    recordIdentifier: opened.header.accountHash + ":" + id)
            identity?.rank = Int(Date.now.timeIntervalSince1970)
        }
        finish()
        complete(ASPasswordCredential(user: entry.username, password: password), identity)
    }

    private func fail(_ error: Error) {
        clearCatalog()
        let message: String = switch error {
        case ExportAccessError.notConfigured: String(localized: "Open and unlock dMsg to update AutoFill, then try again.")
        case ExportAccessError.authenticationCancelled: String(localized: "Authenticate to continue")
        case ExportAccessError.authenticationFailed: String(localized: "Device authentication is temporarily unavailable. Your keys are kept; try again when this device is unlocked.")
        case ExportAccessError.expired: String(localized: "Credential expired") + "\n\n" + String(localized: "Open and unlock dMsg to load your logins.")
        default: String(localized: "Open and unlock dMsg to load your logins.")
        }
        page = .message(message)
    }

    private func clearCatalog() {
        opened = nil
        entries = []
        recent = []
    }

    func close() {
        operation?.cancel()
        finish()
    }

    private func finish() {
        operation = nil
        clearCatalog()
        requested = nil
        services = []
        query = ""
        configuring = false
        page = .finished
    }
}
