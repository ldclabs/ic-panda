import FileProviderUI
import FileProvider
import UIKit

final class FileAccessViewController: FPUIActionExtensionViewController {
    private let message = UILabel()
    private var working = false
    private var generation = 0
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = NativeTheme.paper; view.tintColor = NativeTheme.forest
        message.numberOfLines = 0; message.font = .preferredFont(forTextStyle: .body); message.adjustsFontForContentSizeCategory = true
        message.text = String(localized: "Authenticate to access the file copies you exported from dMsg. Access lasts five minutes; the snapshot expires after 24 hours.")
        let button = UIButton(type: .system); button.setTitle(String(localized: "Authenticate to continue"), for: .normal)
        button.addAction(UIAction { [weak self] _ in self?.unlock() }, for: .touchUpInside)
        let cancel = UIButton(type: .system); cancel.setTitle(String(localized: "Cancel"), for: .normal)
        cancel.addAction(UIAction { [weak self] _ in self?.extensionContext.cancelRequest(withError: NSError(domain: FPUIErrorDomain, code: Int(FPUIExtensionErrorCode.userCancelled.rawValue))) }, for: .touchUpInside)
        let stack = UIStackView(arrangedSubviews: [message,button,cancel]); stack.axis = .vertical; stack.spacing = 24; stack.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 24),stack.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -24),stack.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 32),button.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),cancel.heightAnchor.constraint(greaterThanOrEqualToConstant: 44)])
    }
    override func viewDidDisappear(_ animated: Bool) { super.viewDidDisappear(animated); generation += 1 }
    override func prepare(forError error: any Error) { loadViewIfNeeded() }
    private func unlock() {
        guard !working else { return }; working = true
        let generation = generation
        Task {
            defer { working = false }
            do {
                let opened = try await Task.detached { try SystemExports(.files).authenticate() }.value
                guard generation == self.generation else { return }
                try SystemExports(.files).unlockFiles(opened)
                if let identifier = extensionContext.domainIdentifier {
                    let domain = NSFileProviderDomain(identifier: identifier, displayName: "dMsg")
                    try await NSFileProviderManager(for: domain)?.signalErrorResolved(NSFileProviderError(.notAuthenticated))
                }
                extensionContext.completeRequest()
            } catch { message.text = String(localized: "The file copies are unavailable or expired. Open dMsg to create a new snapshot, then retry.") }
        }
    }
}
