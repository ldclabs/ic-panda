#if DEBUG
import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// App Store preview takes only (`--ui-store-preview`): marks each tap with the
/// orange diamond from the app icon, so the recording shows touch hotspots
/// without any compositing. Drags remove the marker as soon as they move.
struct PreviewTouchInstaller: UIViewRepresentable {
    func makeUIView(context: Context) -> InstallerView { InstallerView() }

    func updateUIView(_ view: InstallerView, context: Context) {}

    final class InstallerView: UIView {
        override func didMoveToWindow() {
            super.didMoveToWindow()
            if let window { PreviewTouches.shared.install(on: window) }
        }
    }
}

@MainActor
final class PreviewTouches: NSObject, UIGestureRecognizerDelegate {
    static let shared = PreviewTouches()
    private weak var observed: UIWindow?
    private var overlay: UIWindow?
    private var marker: UIView?

    func install(on window: UIWindow) {
        guard observed !== window, let scene = window.windowScene else { return }
        observed = window
        let recognizer = TouchObserver()
        recognizer.cancelsTouchesInView = false
        recognizer.delaysTouchesBegan = false
        recognizer.delaysTouchesEnded = false
        recognizer.delegate = self
        recognizer.began = { [weak self] point in self?.show(at: point) }
        recognizer.moved = { [weak self] in self?.hide(after: 0, duration: 0.08) }
        recognizer.ended = { [weak self] in self?.hide(after: 0.15, duration: 0.3) }
        window.addGestureRecognizer(recognizer)
        let overlay = UIWindow(windowScene: scene)
        overlay.windowLevel = .alert + 1
        overlay.isUserInteractionEnabled = false
        overlay.backgroundColor = .clear
        let root = UIViewController()
        root.view.backgroundColor = .clear
        root.view.isUserInteractionEnabled = false
        overlay.rootViewController = root
        overlay.isHidden = false
        self.overlay = overlay
    }

    nonisolated func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }

    private func show(at point: CGPoint) {
        guard let view = overlay?.rootViewController?.view else { return }
        marker?.removeFromSuperview()
        let orange = UIColor(red: 0.949, green: 0.651, blue: 0.353, alpha: 1)
        let container = UIView(frame: CGRect(x: point.x - 41, y: point.y - 41, width: 82, height: 82))
        container.isUserInteractionEnabled = false
        let ring = UIView(frame: CGRect(x: 29, y: 29, width: 24, height: 24))
        ring.layer.cornerRadius = 12
        ring.layer.borderWidth = 2
        ring.layer.borderColor = orange.cgColor
        ring.alpha = 0.55
        let gem = UIView(frame: CGRect(x: 31, y: 31, width: 20, height: 20))
        gem.backgroundColor = orange
        gem.layer.cornerRadius = 3
        gem.layer.borderWidth = 2
        gem.layer.borderColor = UIColor(red: 0.961, green: 0.969, blue: 0.949, alpha: 1).cgColor
        gem.layer.shadowColor = UIColor(red: 0.47, green: 0.24, blue: 0, alpha: 1).cgColor
        gem.layer.shadowOpacity = 0.3
        gem.layer.shadowRadius = 4
        gem.layer.shadowOffset = CGSize(width: 0, height: 2)
        gem.transform = CGAffineTransform(rotationAngle: .pi / 4)
        container.addSubview(ring)
        container.addSubview(gem)
        view.addSubview(container)
        marker = container
        UIView.animate(withDuration: 0.08, delay: 0, options: [.curveEaseOut]) {
            gem.transform = CGAffineTransform(rotationAngle: .pi / 4).scaledBy(x: 0.8, y: 0.8)
        } completion: { _ in
            UIView.animate(withDuration: 0.12) { gem.transform = CGAffineTransform(rotationAngle: .pi / 4) }
        }
        UIView.animate(withDuration: 0.4, delay: 0, options: [.curveEaseOut]) {
            ring.frame = CGRect(x: 0, y: 0, width: 82, height: 82)
            ring.layer.cornerRadius = 41
            ring.alpha = 0
        }
    }

    private func hide(after delay: TimeInterval, duration: TimeInterval) {
        guard let container = marker else { return }
        marker = nil
        UIView.animate(withDuration: duration, delay: delay, options: [.beginFromCurrentState]) {
            container.alpha = 0
        } completion: { _ in container.removeFromSuperview() }
    }
}

/// Watches touches in the app window without ever recognising, so every control
/// behaves exactly as it does without the overlay.
private final class TouchObserver: UIGestureRecognizer {
    var began: ((CGPoint) -> Void)?
    var moved: (() -> Void)?
    var ended: (() -> Void)?
    private var start: CGPoint?

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        guard start == nil, let touch = touches.first, let window = view else { return }
        let point = touch.location(in: window)
        start = point
        began?(point)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        guard let start, let touch = touches.first, let window = view else { return }
        let point = touch.location(in: window)
        if hypot(point.x - start.x, point.y - start.y) > 8 { moved?() }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) { finish() }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { finish() }

    private func finish() {
        ended?()
        start = nil
        state = .failed
    }

    override func reset() { start = nil }
}
#endif
