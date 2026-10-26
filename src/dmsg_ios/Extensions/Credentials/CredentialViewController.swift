import AuthenticationServices
import UIKit

final class CredentialViewController: ASCredentialProviderViewController, UITableViewDataSource, UITableViewDelegate {
    private let table = UITableView(frame: .zero, style: .insetGrouped)
    private let message = UILabel()
    private var opened: OpenExport?
    private var entries: [CredentialExport] = []
    private var requested: String?
    private var authenticatedAt = Date.distantPast
    private var generation = 0
    private var working = false

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = NativeTheme.paper; view.tintColor = NativeTheme.forest
        message.numberOfLines = 0; message.font = .preferredFont(forTextStyle: .body); message.adjustsFontForContentSizeCategory = true
        message.text = String(localized: "Use a password from the snapshot you explicitly exported in dMsg.")
        table.dataSource = self; table.delegate = self; table.rowHeight = UITableView.automaticDimension; table.estimatedRowHeight = 72
        let unlock = UIButton(type: .system); unlock.setTitle(String(localized: "Authenticate to continue"), for: .normal)
        unlock.addAction(UIAction { [weak self] _ in self?.unlock() }, for: .touchUpInside)
        let cancel = UIButton(type: .system); cancel.setTitle(String(localized: "Cancel"), for: .normal)
        cancel.addAction(UIAction { [weak self] _ in self?.extensionContext.cancelRequest(withError: NSError(domain: ASExtensionErrorDomain, code: ASExtensionError.userCanceled.rawValue)) }, for: .touchUpInside)
        let stack = UIStackView(arrangedSubviews: [message, unlock, table, cancel]); stack.axis = .vertical; stack.spacing = 12; stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 16),stack.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -16),stack.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 16),stack.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -12),unlock.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),cancel.heightAnchor.constraint(greaterThanOrEqualToConstant: 44)])
    }
    override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        loadViewIfNeeded()
        let sites = serviceIdentifiers.map(\.identifier).joined(separator: "\n")
        message.text = String(localized: "Choose a credential for:") + "\n" + sites
        requested = nil; unlock()
    }
    override func provideCredentialWithoutUserInteraction(for credentialRequest: any ASCredentialRequest) {
        extensionContext.cancelRequest(withError: NSError(domain: ASExtensionErrorDomain, code: ASExtensionError.userInteractionRequired.rawValue))
    }
    override func prepareInterfaceToProvideCredential(for credentialRequest: any ASCredentialRequest) {
        loadViewIfNeeded()
        guard let identity = credentialRequest.credentialIdentity as? ASPasswordCredentialIdentity else { unavailable(); return }
        requested = identity.recordIdentifier
        message.text = String(localized: "Use this password for:") + "\n" + identity.serviceIdentifier.identifier
        unlock()
    }
    private func unlock() {
        guard !working else { return }; working = true
        let generation = generation
        Task {
            defer { working = false }
            do {
                let result = try await Task.detached { () -> (OpenExport, [CredentialExport]) in
                    let store = try SystemExports(.credentials), value = try store.authenticate()
                    return (value, try store.credentialMetadata(value))
                }.value
                guard generation == self.generation else { return }
                opened = result.0; entries = result.1; authenticatedAt = .now
                if let requested {
                    let parts = requested.split(separator: ":")
                    guard parts.count == 2, parts[0] == result.0.header.id,
                          let entry = entries.first(where: { $0.id == parts[1] }) else { unavailable(); return }
                    provide(entry)
                } else { table.reloadData() }
            } catch { if generation == self.generation { opened = nil; entries = []; table.reloadData(); unavailable() } }
        }
    }
    private func provide(_ entry: CredentialExport) {
        guard let opened, Date.now.timeIntervalSince(authenticatedAt) < 60 else { self.opened = nil; unlock(); return }
        do {
            let password = try SystemExports(.credentials).credential(entry.id, opened: opened)
            self.opened = nil
            extensionContext.completeRequest(withSelectedCredential: ASPasswordCredential(user: entry.username, password: password), completionHandler: nil)
        } catch { unavailable() }
    }
    private func unavailable() {
        message.text = String(localized: "The exported credentials are unavailable or expired. Open dMsg to create a new snapshot.")
    }
    func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { entries.count }
    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell(style: .subtitle, reuseIdentifier: nil), entry = entries[indexPath.row]
        var content = cell.defaultContentConfiguration(); content.text = entry.title; content.secondaryText = entry.username + "\n" + entry.origin
        content.textProperties.numberOfLines = 0; content.secondaryTextProperties.numberOfLines = 0; cell.contentConfiguration = content
        return cell
    }
    func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) { provide(entries[indexPath.row]) }
    override func viewDidDisappear(_ animated: Bool) { super.viewDidDisappear(animated); generation += 1; opened = nil; entries = [] }
}
