import UIKit

/// A scene-level cover also hides native previews and share sheets from the
/// app-switcher snapshot. SwiftUI's root overlay alone sits below those sheets.
@MainActor
final class PrivacyShield {
    private var cover: UIWindow?

    func show() {
        guard cover == nil, let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState != .background }) else { return }
        let window = UIWindow(windowScene: scene)
        window.windowLevel = .alert + 1
        let controller = UIViewController()
        controller.view.backgroundColor = .systemBackground
        let symbol = UIImageView(image: UIImage(systemName: "lock.shield"))
        symbol.tintColor = .secondaryLabel
        symbol.contentMode = .scaleAspectFit
        symbol.translatesAutoresizingMaskIntoConstraints = false
        controller.view.addSubview(symbol)
        NSLayoutConstraint.activate([
            symbol.centerXAnchor.constraint(equalTo: controller.view.centerXAnchor),
            symbol.centerYAnchor.constraint(equalTo: controller.view.centerYAnchor),
            symbol.widthAnchor.constraint(equalToConstant: 36), symbol.heightAnchor.constraint(equalToConstant: 44),
        ])
        window.rootViewController = controller
        window.isHidden = false
        cover = window
    }

    func hide() { cover?.isHidden = true; cover = nil }
}
