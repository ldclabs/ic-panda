import SwiftUI

enum Theme {
    static let forest = Color(uiColor: NativeTheme.forest)
    static let paper = Color(uiColor: NativeTheme.paper)
    static let surface = Color(uiColor: .secondarySystemGroupedBackground)
}

struct ObjectIcon: View {
    private let image: Image

    init(name: String) { image = Image(systemName: name) }

    init(glyph: AppGlyph) { image = glyph.image }

    var body: some View {
        image.resizable().scaledToFit().frame(width: 18, height: 18).foregroundStyle(Theme.forest)
            .frame(width: 36, height: 36).background(Theme.forest.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
            .accessibilityHidden(true)
    }
}

struct NoticeBar: View {
    let text: String
    var body: some View { Label(text, systemImage: "info.circle").font(.footnote).foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading).padding(12).background(Theme.forest.opacity(0.06), in: RoundedRectangle(cornerRadius: 12)) }
}

extension View {
    func privateScreen() -> some View { modifier(PrivateScreen()) }

    /// Two-line object rows are 56 pt at the default text size and grow with
    /// their content. Keep the full row tappable, including its outer insets.
    func compactObjectRow() -> some View {
        frame(minHeight: 44)
            .listRowInsets(EdgeInsets(top: 6, leading: 20, bottom: 6, trailing: 20))
    }
}

private struct PrivateScreen: ViewModifier {
    @Environment(SessionModel.self) private var model
    func body(content: Content) -> some View {
        content.scrollContentBackground(.hidden).background(Theme.paper).privacySensitive()
            .listSectionSpacing(.compact)
            .onAppear { model.activity() }
    }
}
