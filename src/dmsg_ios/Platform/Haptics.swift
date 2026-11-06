import UIKit

/// Short confirmations for actions that otherwise finish out of sight.
@MainActor enum Haptics {
    static func tap() { UIImpactFeedbackGenerator(style: .light).impactOccurred() }

    static func success() { UINotificationFeedbackGenerator().notificationOccurred(.success) }

    static func failure() { UINotificationFeedbackGenerator().notificationOccurred(.error) }
}
