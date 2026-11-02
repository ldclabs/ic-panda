import SwiftUI

/// Pending and acknowledged messages share the same author and alignment.
struct ConversationMessageRow<Content: View>: View {
    let own: Bool
    let name: String
    let avatar: UIImage?
    @ViewBuilder let content: Content
    @Environment(\.layoutDirection) private var textDirection

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            if own { Spacer(minLength: 36) } else { portrait }
            content
                .padding(12)
                .background(own ? Theme.forest.opacity(0.12) : Theme.surface,
                            in: RoundedRectangle(cornerRadius: 16))
                .environment(\.layoutDirection, textDirection)
            if own { portrait } else { Spacer(minLength: 36) }
        }.frame(maxWidth: .infinity).environment(\.layoutDirection, .leftToRight)
    }

    private var portrait: some View {
        Group {
            if let avatar { Image(uiImage: avatar).resizable().scaledToFill() }
            else { Text(String(name.prefix(1)).uppercased()).font(.caption.weight(.semibold)) }
        }.frame(width: 32, height: 32)
            .background(Theme.forest.opacity(0.08))
            .clipShape(RoundedRectangle(cornerRadius: 8)).accessibilityHidden(true)
    }
}
