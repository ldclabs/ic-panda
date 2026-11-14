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
            message.text = String(localized: "Ready") + "\n\n" + String(localized: "AutoFill receives only the login items you select and requires system authentication independently of the workspace. Usernames and website names may appear in system suggestions.")
        } catch { showFailure(error) }
    }
    override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        loadViewIfNeeded()
        configuring = false; cancelButton.setTitle(String(localized: "Cancel"), for: .normal)
        let sites = serviceIdentifiers.map(\.identifier).joined(separator: "\n")
        message.text = String(localized: "Choose a credential for:") + "\n" + sites
        services = serviceIdentifiers.map(\.identifier)
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
                if let entry = CredentialExport.requested(requested, in: result.0.header, entries: entries) {
                    provide(entry)
                } else {
                    requested = nil
                    message.text = String(localized: "Choose a credential for:") + "\n" + services.joined(separator: "\n")
                    unlockButton.isHidden = true; search.isHidden = false; table.isHidden = false; emptySpace.isHidden = true
                    arrange()
                }
            } catch { if generation == self.generation { showFailure(error) } }
        }
    }
    /// Logins for the asking site come first, under their own heading.
    private func arrange() {
        let query = search.text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let shown = query.isEmpty ? entries : entries.filter { $0.contains(query) }
        let matching = shown.filter { entry in services.contains { entry.matches(service: $0) } }
        if matching.isEmpty { sections = shown.isEmpty ? [] : [(nil, shown)] }
        else {
            let others = shown.filter { entry in !matching.contains { $0.id == entry.id } }
            sections = [(String(localized: "For this site"), matching)] + (others.isEmpty ? [] : [(String(localized: "Other logins"), others)])
        }
        table.reloadData()
    }
    func searchBar(_ searchBar: UISearchBar, textDidChange searchText: String) { arrange() }
    func searchBarSearchButtonClicked(_ searchBar: UISearchBar) { searchBar.resignFirstResponder() }
    private func provide(_ entry: CredentialExport) {
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
            self.opened = nil
            extensionContext.completeRequest(withSelectedCredential: ASPasswordCredential(user: entry.username, password: password), completionHandler: nil)
        } catch { showFailure(error) }
    }
    private func showFailure(_ error: Error) {
        clearCredentials()
        unlockButton.isHidden = true
        switch error {
        case ExportAccessError.notConfigured:
            message.text = String(localized: "Open dMsg → Account → System integrations → Password AutoFill → Select login items.")
        case ExportAccessError.authenticationCancelled:
            message.text = String(localized: "Authenticate to continue")
            unlockButton.isHidden = configuring
        case ExportAccessError.authenticationFailed:
            message.text = String(localized: "Device authentication is temporarily unavailable. Your keys are kept; try again when this device is unlocked.")
            unlockButton.isHidden = configuring
        case ExportAccessError.expired:
            message.text = String(localized: "Credential expired") + "\n\n" + String(localized: "Open and unlock dMsg to update AutoFill, then try again.")
        default:
            message.text = String(localized: "Open and unlock dMsg to update AutoFill, then try again.")
        }
    }
    private func clearCredentials() {
        opened = nil; entries = []; sections = []; table.reloadData()
        table.isHidden = true; search.isHidden = true; emptySpace.isHidden = false
    }
    func numberOfSections(in tableView: UITableView) -> Int { sections.count }
    func tableView(_ tableView: UITableView, titleForHeaderInSection section: Int) -> String? { sections[section].title }
    func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { sections[section].entries.count }
    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell(style: .subtitle, reuseIdentifier: nil), entry = sections[indexPath.section].entries[indexPath.row]
        var content = cell.defaultContentConfiguration(); content.text = entry.title; content.secondaryText = entry.username + "\n" + entry.origin
        content.textProperties.numberOfLines = 0; content.secondaryTextProperties.numberOfLines = 0; cell.contentConfiguration = content
        return cell
    }
    func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) { provide(sections[indexPath.section].entries[indexPath.row]) }
    override func viewDidDisappear(_ animated: Bool) { super.viewDidDisappear(animated); generation += 1; clearCredentials() }
}
