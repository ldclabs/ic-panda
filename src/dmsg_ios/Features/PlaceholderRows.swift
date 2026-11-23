import SwiftUI

/// A first read shows the shape of the rows to come instead of a centred spinner.
/// VoiceOver hears one "Loading…" for the group.
struct PlaceholderRows: View {
    let identifier: String
    var count = 4
    @ScaledMetric private var tile: CGFloat = 36

    var body: some View {
        ForEach(0..<count, id: \.self) { index in
            HStack(spacing: 10) {
                RoundedRectangle(cornerRadius: tile * 0.28, style: .continuous).fill(Theme.fill).frame(width: tile, height: tile)
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: String(repeating: "x", count: 14 + index * 5)).font(.body.weight(.medium))
                    Text(verbatim: String(repeating: "x", count: 22 - index * 3)).rowSubtitle()
                }
            }.redacted(reason: .placeholder)
                .accessibilityElement(children: .ignore).accessibilityLabel("Loading…")
                .accessibilityHidden(index > 0)
                .accessibilityIdentifier(index == 0 ? identifier : "")
        }
    }
}
