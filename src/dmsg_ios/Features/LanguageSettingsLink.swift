import SwiftUI

/// Native localization follows the system's per-app language preference.
struct LanguageSettingsLink: View {
    @ScaledMetric(relativeTo: .footnote) var iconSize: CGFloat = 18

    var body: some View {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            Link(destination: url) {
                Label { LabeledContent("Language", value: Locale.current.localizedString(forLanguageCode: Bundle.main.preferredLocalizations.first ?? "en") ?? "") } icon: {
                    AppGlyph.language.image.resizable().scaledToFit().frame(width: iconSize, height: iconSize)
                }
            }
                .accessibilityIdentifier("settings.language").accessibilityLabel("Language")
                .accessibilityValue(Locale.current.localizedString(forLanguageCode: Bundle.main.preferredLocalizations.first ?? "en") ?? "")
                .accessibilityHint("Choose the language for dMsg in iOS Settings.")
        }
    }
}
