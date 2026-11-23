import SwiftUI

/// Pending and acknowledged messages share the same alignment. One's own messages
/// sit on their side without a portrait; others show theirs once per group.
struct ConversationMessageRow<Content: View, Actions: View>: View {
    let own: Bool
    let name: String
    let avatar: UIImage?
    let showsPortrait: Bool
    @ViewBuilder let content: Content
    @ViewBuilder let actions: Actions
    @Environment(\.layoutDirection) private var textDirection

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            if own { Spacer(minLength: 48) }
            else if showsPortrait { portrait }
            else { Color.clear.frame(width: 32, height: 1) }
            content
                .padding(.horizontal, 12).padding(.vertical, 8)
                .background(own ? Theme.fill : Theme.card, in: RoundedRectangle(cornerRadius: Theme.radiusLarge))
                .contentShape(.contextMenuPreview, RoundedRectangle(cornerRadius: Theme.radiusLarge))
                .contextMenu { actions }
                .environment(\.layoutDirection, textDirection)
            if !own { Spacer(minLength: 36) }
        }.frame(maxWidth: .infinity).environment(\.layoutDirection, .leftToRight)
    }

    private var portrait: some View {
        Group {
            if let avatar { Image(uiImage: avatar).resizable().scaledToFill() }
            else { Text(String(name.prefix(1)).uppercased()).font(.footnote.weight(.semibold)) }
        }.frame(width: 32, height: 32)
            .background(Theme.fill)
            .clipShape(RoundedRectangle(cornerRadius: Theme.radiusSmall)).accessibilityHidden(true)
    }
}
