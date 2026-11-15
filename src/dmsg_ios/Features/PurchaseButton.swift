import SwiftUI

/// Keep feedback beside the selected price; the bottom of the catalogue may
/// be offscreen when someone starts their first subscription.
struct PurchaseButton: View {
    let price: String
    let phase: PurchaseModel.PurchasePhase?
    let disabled: Bool
    let identifier: String
    let action: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Button(action: action) {
                    Text(verbatim: price)
                        .frame(minWidth: 44, minHeight: 44, alignment: .leading).contentShape(Rectangle())
                }.disabled(disabled || phase != nil)
                    .accessibilityIdentifier(identifier)
                    .accessibilityValue(phase?.message ?? "")
                if phase != nil {
                    // The activity indicator stays legible while the price
                    // button is disabled to prevent duplicate purchases.
                    ProgressView().controlSize(.small).tint(Color.primary)
                        .accessibilityHidden(true).transition(.opacity)
                }
            }
            if let phase {
                Text(phase.message).font(.subheadline).foregroundStyle(.primary)
                    .accessibilityIdentifier("plan.purchase-phase")
                    .transition(.opacity)
                if phase == .preparing {
                    Text("You'll confirm with Apple before paying.").font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier("plan.purchase-preparing-hint")
                        .transition(.opacity)
                }
            }
        }.animation(reduceMotion ? nil : .easeInOut(duration: 0.18), value: phase)
    }
}
