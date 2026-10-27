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
        image.resizable().scaledToFit().frame(width: 20, height: 20).foregroundStyle(Theme.forest)
            .frame(width: 42, height: 44).background(Theme.forest.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
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
}

private struct PrivateScreen: ViewModifier {
    @Environment(SessionModel.self) private var model
    func body(content: Content) -> some View {
        content.scrollContentBackground(.hidden).background(Theme.paper).privacySensitive()
            .listSectionSpacing(.compact)
            .onAppear { model.activity() }
    }
}
