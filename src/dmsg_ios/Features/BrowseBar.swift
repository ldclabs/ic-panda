import SwiftUI

extension View {
    /// Puts a workspace's scope bar under the search field, with the list scrolling beneath it.
    func scopeBar<Bar: View>(@ViewBuilder _ bar: () -> Bar) -> some View { edgeBar(.top, fallback: Theme.paper, bar) }

    /// Fades a horizontally scrolling row at both ends, so a clipped tab reads as more to scroll.
    func edgeFade() -> some View {
        mask(LinearGradient(stops: [.init(color: .clear, location: 0), .init(color: .black, location: 0.04),
                                    .init(color: .black, location: 0.96), .init(color: .clear, location: 1)],
                            startPoint: .leading, endPoint: .trailing))
    }

    /// The capsule of a browse bar tab, also for a tab that opens a menu. A `sliding` tab leaves
    /// its capsule to `slidingTabCapsule()` on the row, so a new selection slides there.
    func browseTabShape(selected: Bool, sliding: Bool = false) -> some View {
        padding(.horizontal, 12).frame(minHeight: 44)
            .foregroundStyle(selected ? Theme.forest : Color.secondary)
            .background(selected && !sliding ? Theme.fill : .clear, in: Capsule())
            .anchorPreference(key: SelectedTabBounds.self, value: .bounds) { selected && sliding ? $0 : nil }
            .contentShape(Capsule())
    }

    /// Draws the selected sliding tab's capsule behind a row of exclusive tabs. Animate the
    /// row on its selection (`Motion.move`) and the capsule glides from tab to tab.
    func slidingTabCapsule() -> some View {
        backgroundPreferenceValue(SelectedTabBounds.self) { anchor in
            // Anchors resolve in unmirrored coordinates, so the capsule is placed left to right
            // even in a right-to-left language, where the reader would mirror its origin and offset.
            GeometryReader { proxy in
                if let anchor {
                    let frame = proxy[anchor]
                    Capsule().fill(Theme.fill).frame(width: frame.width, height: frame.height).offset(x: frame.minX, y: frame.minY)
                }
            }.environment(\.layoutDirection, .leftToRight)
        }
    }
}

private struct SelectedTabBounds: PreferenceKey {
    static var defaultValue: Anchor<CGRect>? { nil }
    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) { value = value ?? nextValue() }
}

/// One tab of the bar under the search field in Vault, Files and Messages.
struct BrowseTab<Content: View>: View {
    let selected: Bool
    /// One of a row of exclusive tabs whose capsule slides (`slidingTabCapsule()`).
    var sliding = false
    let action: () -> Void
    @ViewBuilder let label: Content

    var body: some View {
        Button(action: action) { HStack(spacing: 5) { label }.browseTabShape(selected: selected, sliding: sliding) }
            .buttonStyle(.plain).accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// Tabs that switch a workspace between its scopes. Where they do not fit they
/// drop their icons, and then become a menu.
struct BrowseScopes<Value: Hashable>: View {
    struct Option {
        let value: Value
        let title: LocalizedStringKey
        let icon: Image
        let identifier: String
    }

    let title: LocalizedStringKey
    @Binding var selection: Value
    let options: [Option]
    @Environment(\.dynamicTypeSize) private var textSize

    var body: some View {
        if textSize.isAccessibilitySize { menu }
        else {
            ViewThatFits(in: .horizontal) {
                tabs(icons: true)
                tabs(icons: false)
                menu
            }.frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func tabs(icons: Bool) -> some View {
        HStack(spacing: 6) {
            ForEach(options, id: \.value) { option in
                BrowseTab(selected: selection == option.value, sliding: true) { selection = option.value } label: {
                    if icons { option.icon.accessibilityHidden(true) }
                    Text(option.title)
                }.accessibilityIdentifier(option.identifier)
            }
        }.slidingTabCapsule().motion(Motion.move, value: selection)
            .padding(.leading, 16).fixedSize(horizontal: true, vertical: false)
    }

    private var menu: some View {
        let current = options.first { $0.value == selection } ?? options[0]
        return Menu {
            Picker(title, selection: $selection) {
                ForEach(options, id: \.value) { Text($0.title).tag($0.value) }
            }
        } label: {
            HStack(spacing: 8) {
                Text(current.title).fixedSize(horizontal: false, vertical: true)
                Image(systemName: "chevron.down").font(.callout).accessibilityHidden(true)
            }.frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).padding(.horizontal, 16).padding(.vertical, 8)
        }.accessibilityIdentifier(current.identifier)
    }
}
