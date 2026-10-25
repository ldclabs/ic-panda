import SwiftUI

enum Theme {
    static let forest = Color(uiColor: UIColor { $0.userInterfaceStyle == .dark
        ? UIColor(red: 0.44, green: 0.78, blue: 0.64, alpha: 1)
        : UIColor(red: 0.078, green: 0.36, blue: 0.27, alpha: 1) })
    static let paper = Color(uiColor: UIColor { $0.userInterfaceStyle == .dark
        ? UIColor(red: 0.055, green: 0.082, blue: 0.071, alpha: 1)
        : UIColor(red: 0.97, green: 0.973, blue: 0.957, alpha: 1) })
    static let surface = Color(uiColor: .secondarySystemGroupedBackground)
}

struct ObjectIcon: View {
    let name: String
    var body: some View {
        Image(systemName: name).font(.system(size: 20, weight: .medium)).foregroundStyle(Theme.forest)
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

func vaultSymbol(_ kind: String) -> String {
    switch kind { case "login": "key.horizontal"; case "note": "note.text"; case "api": "terminal"; case "key", "ssh": "key"; default: "lock.doc" }
}
