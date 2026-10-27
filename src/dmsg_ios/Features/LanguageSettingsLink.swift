import SwiftUI

/// Native localization follows the system's per-app language preference.
struct LanguageSettingsLink: View {
    @ScaledMetric(relativeTo: .footnote) var iconSize: CGFloat = 18

    var body: some View {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            Link(destination: url) {
                Label { Text("Language") } icon: {
                    AppGlyph.language.image.resizable().scaledToFit().frame(width: iconSize, height: iconSize)
                }
            }
                .accessibilityIdentifier("settings.language")
                .accessibilityHint("Choose the language for dMsg in iOS Settings.")
        }
    }
}
