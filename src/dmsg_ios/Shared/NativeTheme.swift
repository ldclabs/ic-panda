import UIKit

/// The palette lives in `Shared/Palette.xcassets`, which the app and every
/// extension compile into their own bundles. Radii are the only shape tokens.
enum NativeTheme {
    static let forest = UIColor(named: "Forest")!
    static let paper = UIColor(named: "Paper")!

    /// Thumbnails, quotes and small avatars.
    static let radiusSmall: CGFloat = 8
    /// Cards, fields and notices.
    static let radius: CGFloat = 12
    /// Message bubbles and feature cards.
    static let radiusLarge: CGFloat = 18
}
