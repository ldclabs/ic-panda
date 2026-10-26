import UIKit
import UniformTypeIdentifiers

final class ShareViewController: UIViewController {
    private let status = UILabel()
    private let button = UIButton(type: .system)
    private var completedProviders: Set<Int> = []
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = NativeTheme.paper; view.tintColor = NativeTheme.forest
        button.titleLabel?.font = .preferredFont(forTextStyle: .headline); button.titleLabel?.adjustsFontForContentSizeCategory = true; button.titleLabel?.numberOfLines = 0
        status.numberOfLines = 0; status.font = .preferredFont(forTextStyle: .body); status.adjustsFontForContentSizeCategory = true
        status.text = String(localized: "Save these items for dMsg. Choose an account and import them after opening and unlocking the app.")
        button.setTitle(String(localized: "Save to dMsg"), for: .normal)
        button.addTarget(self, action: #selector(save), for: .touchUpInside)
        let cancel = UIButton(type: .system)
        cancel.setTitle(String(localized: "Cancel"), for: .normal)
        cancel.addAction(UIAction { [weak self] _ in self?.extensionContext?.cancelRequest(withError: CocoaError(.userCancelled)) }, for: .touchUpInside)
        let stack = UIStackView(arrangedSubviews: [status, button, cancel]); stack.axis = .vertical; stack.spacing = 20; stack.translatesAutoresizingMaskIntoConstraints = false
        let scroll = UIScrollView(); scroll.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(scroll); scroll.addSubview(stack)
        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor), scroll.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor), scroll.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor, constant: 24), stack.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor, constant: -24),
            stack.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor, constant: 24), stack.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor, constant: -24),
            stack.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor, constant: -48),
            button.heightAnchor.constraint(greaterThanOrEqualToConstant: 44), cancel.heightAnchor.constraint(greaterThanOrEqualToConstant: 44)])
    }
    @objc private func save() {
        button.isEnabled = false
        let providers = (extensionContext?.inputItems as? [NSExtensionItem] ?? []).flatMap { $0.attachments ?? [] }
        guard !providers.isEmpty, providers.count <= 10 else { fail(); return }
        Task { [weak self] in
            guard let self else { return }
            do {
                let storage = try IncomingShares()
                for (index, provider) in providers.enumerated() where !completedProviders.contains(index) {
                    if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                            provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier) { value, error in
                                do { guard let url = value as? URL, url.isFileURL else { throw error ?? CocoaError(.fileReadUnknown) }; _ = try storage.stage(url); continuation.resume() }
                                catch { continuation.resume(throwing: error) }
                            }
                        }
                    } else if let type = provider.registeredTypeIdentifiers.first(where: { UTType($0)?.conforms(to: .data) == true && $0 != UTType.url.identifier && $0 != UTType.plainText.identifier }) {
                        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                            provider.loadFileRepresentation(forTypeIdentifier: type) { url, error in
                                do { guard let url else { throw error ?? CocoaError(.fileReadUnknown) }; _ = try storage.stage(url); continuation.resume() }
                                catch { continuation.resume(throwing: error) }
                            }
                        }
                    } else {
                        let type = provider.hasItemConformingToTypeIdentifier(UTType.url.identifier) ? UTType.url.identifier : UTType.plainText.identifier
                        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                            provider.loadItem(forTypeIdentifier: type) { value, error in
                                do {
                                    let text: String
                                    if let url = value as? URL { text = url.absoluteString }
                                    else if let value = value as? String { text = value }
                                    else { throw error ?? CocoaError(.fileReadUnknown) }
                                    _ = try storage.stageText(text); continuation.resume()
                                } catch { continuation.resume(throwing: error) }
                            }
                        }
                    }
                    completedProviders.insert(index)
                }
                status.text = String(localized: "Saved for dMsg. Open the app to review and import these items.")
                button.setTitle(String(localized: "Done"), for: .normal); button.removeTarget(self, action: #selector(save), for: .touchUpInside)
                button.addAction(UIAction { [weak self] _ in self?.extensionContext?.completeRequest(returningItems: nil) }, for: .touchUpInside); button.isEnabled = true
            } catch { fail() }
        }
    }
    private func fail() { status.text = String(localized: "The selected items could not be staged. Open dMsg and try importing them there."); button.isEnabled = true }
}
