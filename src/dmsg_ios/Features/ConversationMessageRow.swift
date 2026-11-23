import SwiftUI

/// Pending and acknowledged messages share the same alignment. One's own messages
/// sit on their side in solid brand bubbles without a portrait; others show theirs
/// once per group. Consecutive messages from one sender form a group: the corners
/// between them tighten, as in Messages.
struct ConversationMessageRow<Content: View, Actions: View>: View {
    let own: Bool
    let account: String
    let name: String
    let avatar: UIImage?
    var firstOfGroup = true
    var lastOfGroup = true
    @ViewBuilder let content: Content
    @ViewBuilder let actions: Actions
    @Environment(\.layoutDirection) private var textDirection
    @ScaledMetric private var portraitSize: CGFloat = 32

    var body: some View {
        let shape = bubbleShape
        HStack(alignment: .top, spacing: 8) {
            if own { Spacer(minLength: 48) }
            else if firstOfGroup { Avatar(name, seed: account, image: avatar, size: 32) }
            else { Color.clear.frame(width: portraitSize, height: 1) }
            content
                .foregroundStyle(own ? AnyShapeStyle(.white) : AnyShapeStyle(.primary),
                                 own ? AnyShapeStyle(.white.opacity(0.85)) : AnyShapeStyle(.secondary))
                .tint(own ? .white : Theme.forest)
                .padding(.horizontal, 12).padding(.vertical, 8)
                .background(own ? Theme.forestFill : Theme.card, in: shape)
                .contentShape(.contextMenuPreview, shape)
                .contextMenu { actions }
                .environment(\.layoutDirection, textDirection)
            if !own { Spacer(minLength: 36) }
        }.frame(maxWidth: .infinity).environment(\.layoutDirection, .leftToRight)
    }

    /// The side facing the sender's edge tightens where the group continues.
    private var bubbleShape: UnevenRoundedRectangle {
        let large = Theme.radiusLarge, tight = Theme.radiusSmall / 2
        let top = firstOfGroup ? large : tight, bottom = lastOfGroup ? large : tight
        // The bubble is laid out in the text's direction, but the sender's side is physical.
        let senderIsTrailing = own == (textDirection == .leftToRight)
        return UnevenRoundedRectangle(
            topLeadingRadius: senderIsTrailing ? large : top, bottomLeadingRadius: senderIsTrailing ? large : bottom,
            bottomTrailingRadius: senderIsTrailing ? bottom : large, topTrailingRadius: senderIsTrailing ? top : large,
            style: .continuous)
    }
}

extension ShapeStyle where Self == AnyShapeStyle {
    /// Quotes and attachments inside a bubble: a lighter layer on one's own green
    /// bubble, the quiet fill on everyone else's.
    static func bubbleInset(own: Bool) -> AnyShapeStyle { own ? AnyShapeStyle(.white.opacity(0.16)) : AnyShapeStyle(Theme.fill) }
}
