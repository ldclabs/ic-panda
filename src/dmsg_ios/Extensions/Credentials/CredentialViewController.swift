import AuthenticationServices
import SwiftUI

final class CredentialViewController: ASCredentialProviderViewController {
    private lazy var picker = CredentialPickerSession { [weak self] credential, identity in
        guard let self else { return }
        if let identity {
            ASCredentialIdentityStore.shared.saveCredentialIdentities([identity] as [any ASCredentialIdentity], completion: nil)
        }
        extensionContext.completeRequest(withSelectedCredential: credential, completionHandler: nil)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = NativeTheme.paper
        let host = UIHostingController(rootView: CredentialPickerView(picker: picker) { [weak self] in self?.cancel() })
        addChild(host)
        host.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(host.view)
        NSLayoutConstraint.activate([
            host.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            host.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            host.view.topAnchor.constraint(equalTo: view.topAnchor),
            host.view.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])
        host.didMove(toParent: self)
    }

    override func prepareInterfaceForExtensionConfiguration() {
        picker.configure()
    }

    override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        picker.start(services: serviceIdentifiers)
    }

    override func provideCredentialWithoutUserInteraction(for credentialRequest: any ASCredentialRequest) {
        extensionContext.cancelRequest(withError: ASExtensionError(.userInteractionRequired))
    }

    override func prepareInterfaceToProvideCredential(for credentialRequest: any ASCredentialRequest) {
        guard let identity = credentialRequest.credentialIdentity as? ASPasswordCredentialIdentity else {
            extensionContext.cancelRequest(withError: ASExtensionError(.credentialIdentityNotFound))
            return
        }
        picker.start(services: [identity.serviceIdentifier], requested: identity)
    }

    private func cancel() {
        if case .finished = picker.page { return }
        let configuring = picker.configuring
        picker.close()
        if configuring { extensionContext.completeExtensionConfigurationRequest() }
        else { extensionContext.cancelRequest(withError: ASExtensionError(.userCanceled)) }
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        picker.close()
    }
}
