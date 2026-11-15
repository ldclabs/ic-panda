import SwiftUI

/// Keep feedback beside the selected price; the bottom of the catalogue may
/// be offscreen when someone starts their first subscription.
struct PurchaseButton: View {
    let price: String
    var title: String? = nil
    let phase: PurchaseModel.PurchasePhase?
    let disabled: Bool
    let identifier: String
    let action: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Button(action: action) {
                Text(verbatim: title ?? price)
                    .font(.headline).frame(maxWidth: .infinity, minHeight: 32)
                    .fixedSize(horizontal: false, vertical: true)
            }.buttonStyle(.borderedProminent).tint(Theme.forestFill).controlSize(.large)
                .disabled(disabled || phase != nil)
                .accessibilityIdentifier(identifier)
                .accessibilityValue(phase?.message ?? price)
            if let phase {
                HStack(alignment: .top, spacing: 8) {
                    ProgressView().controlSize(.small).tint(Color.primary).accessibilityHidden(true)
                    Text(phase.message).font(.subheadline).foregroundStyle(.primary)
                        .accessibilityIdentifier("plan.purchase-phase")
                }.padding(.top, 4).transition(.opacity)
                if phase == .preparing {
                    Text("You'll confirm with Apple before paying.").font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier("plan.purchase-preparing-hint")
                        .transition(.opacity)
                }
            }
        }.animation(reduceMotion ? nil : .easeInOut(duration: 0.18), value: phase)
    }
}
