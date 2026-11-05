import SwiftUI

extension View {
    /// The capsule of a browse bar tab, also for a tab that opens a menu.
    func browseTabShape(selected: Bool) -> some View {
        padding(.horizontal, 12).frame(minHeight: 44)
            .foregroundStyle(selected ? Theme.forest : Color.secondary)
            .background(selected ? Theme.forest.opacity(0.10) : .clear, in: Capsule())
            .contentShape(Capsule())
    }
}

/// One tab of the bar under the search field in Vault, Files and Messages.
struct BrowseTab<Content: View>: View {
    let selected: Bool
    let action: () -> Void
    @ViewBuilder let label: Content

    var body: some View {
        Button(action: action) { HStack(spacing: 5) { label }.browseTabShape(selected: selected) }
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
                BrowseTab(selected: selection == option.value) { selection = option.value } label: {
                    if icons { option.icon.resizable().scaledToFit().frame(width: 20, height: 17).accessibilityHidden(true) }
                    Text(option.title)
                }.accessibilityIdentifier(option.identifier)
            }
        }.padding(.leading, 16).fixedSize(horizontal: true, vertical: false)
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
                Image(systemName: "chevron.down").font(.system(size: 16)).accessibilityHidden(true)
            }.frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).padding(.horizontal, 16).padding(.vertical, 8)
        }.accessibilityIdentifier(current.identifier)
    }
}
