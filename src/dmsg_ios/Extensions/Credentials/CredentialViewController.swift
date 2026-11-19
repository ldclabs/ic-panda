import AuthenticationServices
import UIKit

final class CredentialViewController: ASCredentialProviderViewController, UITableViewDataSource, UITableViewDelegate, UISearchBarDelegate {
    private let table = UITableView(frame: .zero, style: .insetGrouped)
    private let message = UILabel()
    private let search = UISearchBar()
    private let unlockButton = UIButton(type: .system)
    private let cancelButton = UIButton(type: .system)
    private let progress = UIActivityIndicatorView(style: .medium)
    private let emptySpace = UIView()
    private var services: [String] = []
    private var target: ASCredentialServiceIdentifier?
    private var serviceKey: String? { target?.loginServiceKey }
    private var destination: String {
        guard let target, !target.identifier.isEmpty else { return String(localized: "Unidentified app or website") }
        if #available(iOS 26.2, *), target.type == .app, let name = target.displayName, !name.isEmpty, name != target.identifier {
            return name + " (" + target.identifier + ")"
        }
        return target.identifier
    }
    private var recent: [String] = []
    private var sections: [(title: String?, entries: [CredentialExport])] = []
    private var opened: OpenExport?
    private var entries: [CredentialExport] = []
    private var requested: String?
    private var authenticatedAt = Date.distantPast
    private var generation = 0
    private var working = false
    private var configuring = false

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = NativeTheme.paper; view.tintColor = NativeTheme.forest
        message.numberOfLines = 0; message.font = .preferredFont(forTextStyle: .body); message.adjustsFontForContentSizeCategory = true
        message.text = String(localized: "Authenticate to continue")
        message.accessibilityIdentifier = "autofill.message"
        table.dataSource = self; table.delegate = self; table.rowHeight = UITableView.automaticDimension; table.estimatedRowHeight = 72
        table.backgroundColor = .clear; table.isHidden = true; table.accessibilityIdentifier = "autofill.credentials"
        search.searchBarStyle = .minimal; search.delegate = self; search.isHidden = true
        unlockButton.setTitle(String(localized: "Authenticate to continue"), for: .normal)
        unlockButton.accessibilityIdentifier = "autofill.authenticate"
        unlockButton.addAction(UIAction { [weak self] _ in self?.unlock() }, for: .touchUpInside)
        cancelButton.setTitle(String(localized: "Cancel"), for: .normal)
        cancelButton.addAction(UIAction { [weak self] _ in
            guard let self else { return }
            if configuring { extensionContext.completeExtensionConfigurationRequest() }
            else { extensionContext.cancelRequest(withError: NSError(domain: ASExtensionErrorDomain, code: ASExtensionError.userCanceled.rawValue)) }
        }, for: .touchUpInside)
        progress.hidesWhenStopped = true
        emptySpace.setContentHuggingPriority(.init(1), for: .vertical)
        let stack = UIStackView(arrangedSubviews: [message, progress, unlockButton, search, table, emptySpace, cancelButton]); stack.axis = .vertical; stack.spacing = 12; stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 16),stack.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -16),stack.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 16),stack.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -12),unlockButton.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),cancelButton.heightAnchor.constraint(greaterThanOrEqualToConstant: 44)])
    }
    override func prepareInterfaceForExtensionConfiguration() {
        loadViewIfNeeded(); configuring = true
        cancelButton.setTitle(String(localized: "Done"), for: .normal)
        unlockButton.isHidden = true
        do {
            _ = try SystemExports(.credentials).header()
            message.text = String(localized: "Choose a login when an app or website asks for a password.")
        } catch { showFailure(error) }
    }
    override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        loadViewIfNeeded()
        configuring = false; cancelButton.setTitle(String(localized: "Cancel"), for: .normal)
        services = serviceIdentifiers.map(\.identifier)
        target = serviceIdentifiers.first
        message.text = String(localized: "Choose a credential for:") + "\n" + destination
        requested = nil; unlock()
    }
    override func provideCredentialWithoutUserInteraction(for credentialRequest: any ASCredentialRequest) {
        extensionContext.cancelRequest(withError: NSError(domain: ASExtensionErrorDomain, code: ASExtensionError.userInteractionRequired.rawValue))
    }
    override func prepareInterfaceToProvideCredential(for credentialRequest: any ASCredentialRequest) {
        loadViewIfNeeded()
        configuring = false; cancelButton.setTitle(String(localized: "Cancel"), for: .normal)
        guard let identity = credentialRequest.credentialIdentity as? ASPasswordCredentialIdentity else { showFailure(CocoaError(.fileReadCorruptFile)); return }
        services = [identity.serviceIdentifier.identifier]
        target = identity.serviceIdentifier
        requested = identity.recordIdentifier
        message.text = String(localized: "Use this password for:") + "\n" + identity.serviceIdentifier.identifier
        unlock()
    }
    private func unlock() {
        guard !working else { return }; working = true
        clearCredentials()
        unlockButton.isEnabled = false; progress.startAnimating()
        let generation = generation
        Task {
            defer { working = false; unlockButton.isEnabled = true; progress.stopAnimating() }
            do {
                let result = try await Task.detached { () -> (OpenExport, [CredentialExport]) in
                    let store = try SystemExports(.credentials), value = try store.authenticate()
                    return (value, try store.credentialMetadata(value))
                }.value
                guard generation == self.generation else { return }
                opened = result.0; entries = result.1; authenticatedAt = .now
                recent = try SystemExports(.credentials).recentLogins(account: result.0.header.accountHash, service: serviceKey)
                if let entry = CredentialExport.requested(requested, in: result.0.header, entries: entries),
                   let id = entry.itemID, recent.contains(id) {
                    provide(entry, remember: true)
                } else {
                    requested = nil
                    message.text = services.isEmpty ? String(localized: "Password AutoFill") : String(localized: "Choose a credential for:") + "\n" + destination
                    unlockButton.isHidden = true; search.isHidden = false; table.isHidden = false; emptySpace.isHidden = true
                    arrange()
                }
            } catch { if generation == self.generation { showFailure(error) } }
        }
    }
    /// Logins for the asking site come first, under their own heading.
    private func arrange() {
        let query = search.text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        sections = CredentialExport.sections(entries: entries, services: services, recent: recent, query: query)
        if sections.isEmpty {
            let empty = UILabel()
            empty.text = query.isEmpty ? String(localized: "No items") : String(localized: "No matching results")
            empty.font = .preferredFont(forTextStyle: .body)
            empty.adjustsFontForContentSizeCategory = true
            empty.textColor = .secondaryLabel
            empty.textAlignment = .center
            empty.numberOfLines = 0
            table.backgroundView = empty
        } else { table.backgroundView = nil }
        table.reloadData()
    }
    func searchBar(_ searchBar: UISearchBar, textDidChange searchText: String) { arrange() }
    func searchBarSearchButtonClicked(_ searchBar: UISearchBar) { searchBar.resignFirstResponder() }
    private func choose(_ entry: CredentialExport) {
        if let id = entry.itemID, recent.contains(id) { provide(entry, remember: true); return }
        let alert = UIAlertController(title: String(localized: "Fill this login?"), message: [entry.title, entry.username, entry.origin, String(localized: "Use this password for:") + " " + destination].filter { !$0.isEmpty }.joined(separator: "\n"), preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: String(localized: "Cancel"), style: .cancel) { [weak self] _ in
            guard let self else { return }
            requested = nil; search.isHidden = false; table.isHidden = false; emptySpace.isHidden = true; arrange()
        })
        alert.addAction(UIAlertAction(title: String(localized: "Fill once"), style: .default) { [weak self] _ in self?.provide(entry, remember: false) })
        if serviceKey != nil, entry.itemID != nil {
            alert.addAction(UIAlertAction(title: String(localized: "Fill and remember"), style: .default) { [weak self] _ in self?.provide(entry, remember: true) })
        }
        present(alert, animated: true)
    }

    private func provide(_ entry: CredentialExport, remember: Bool) {
        guard let opened, Date.now.timeIntervalSince(authenticatedAt) < 60 else { self.opened = nil; unlockButton.isHidden = false; unlock(); return }
        do {
            let store = try SystemExports(.credentials)
            if try store.header().id != opened.header.id {
                self.opened = nil; requested = nil
                // A renewal while the picker is open needs the current key and
                // another explicit selection, not an "expired" dead end.
                Task { unlock() }; return
            }
            let password = try store.credential(entry.id, opened: opened)
            if remember, let target, let serviceKey, let id = entry.itemID {
                try store.remember(id, service: serviceKey, opened: opened)
                let identifier = target.type == .URL ? ASCredentialServiceIdentifier(identifier: serviceKey, type: .URL) : target
                let identity = ASPasswordCredentialIdentity(serviceIdentifier: identifier, user: entry.username, recordIdentifier: opened.header.accountHash + ":" + id)
                identity.rank = Int(Date.now.timeIntervalSince1970)
                ASCredentialIdentityStore.shared.saveCredentialIdentities([identity]) { _, _ in }
            }
            self.opened = nil
            extensionContext.completeRequest(withSelectedCredential: ASPasswordCredential(user: entry.username, password: password), completionHandler: nil)
        } catch { showFailure(error) }
    }
    private func showFailure(_ error: Error) {
        clearCredentials()
        unlockButton.isHidden = configuring
        unlockButton.setTitle(String(localized: "Retry"), for: .normal)
        switch error {
        case ExportAccessError.notConfigured:
            message.text = String(localized: "Open and unlock dMsg to load your logins.")
        case ExportAccessError.authenticationCancelled:
            message.text = String(localized: "Authenticate to continue")
        case ExportAccessError.authenticationFailed:
            message.text = String(localized: "Device authentication is temporarily unavailable. Your keys are kept; try again when this device is unlocked.")
        case ExportAccessError.expired:
            message.text = String(localized: "Credential expired") + "\n\n" + String(localized: "Open and unlock dMsg to load your logins.")
        default:
            message.text = String(localized: "Open and unlock dMsg to load your logins.")
        }
    }
    private func clearCredentials() {
        opened = nil; entries = []; sections = []; recent = []; table.reloadData()
        table.isHidden = true; search.isHidden = true; emptySpace.isHidden = false
    }
    func numberOfSections(in tableView: UITableView) -> Int { sections.count }
    func tableView(_ tableView: UITableView, titleForHeaderInSection section: Int) -> String? { sections[section].title }
    func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { sections[section].entries.count }
    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = tableView.dequeueReusableCell(withIdentifier: "credential") ?? UITableViewCell(style: .subtitle, reuseIdentifier: "credential")
        let entry = sections[indexPath.section].entries[indexPath.row]
        var content = cell.defaultContentConfiguration(); content.text = entry.title; content.secondaryText = entry.username + "\n" + entry.origin
        content.textProperties.numberOfLines = 0; content.secondaryTextProperties.numberOfLines = 0; cell.contentConfiguration = content
        return cell
    }
    func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) { choose(sections[indexPath.section].entries[indexPath.row]) }
    override func viewDidDisappear(_ animated: Bool) { super.viewDidDisappear(animated); generation += 1; clearCredentials() }
}
