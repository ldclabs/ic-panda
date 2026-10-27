import SwiftUI

/// Native localization follows the system's per-app language preference.
struct LanguageSettingsLink: View {
    var body: some View {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            Link(destination: url) { Label("App language", glyph: .language) }
                .accessibilityIdentifier("settings.language")
                .accessibilityHint("Choose the language for dMsg in iOS Settings.")
        }
    }
}
