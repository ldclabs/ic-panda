import UIKit

/// A tap for copies, which finish out of sight. SwiftUI views use `sensoryFeedback`.
@MainActor enum Haptics {
    static func tap() { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
}
