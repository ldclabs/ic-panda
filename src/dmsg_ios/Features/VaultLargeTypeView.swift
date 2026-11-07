import SwiftUI
@preconcurrency import DMsgCore

/// The presentation target contains only the already-masked item, never revealed text.
struct VaultLargeTypeTarget: Identifiable {
    let item: VaultItem
    let field: VaultField
    let historical: Bool
    var id: String { "\(item.id):\(item.revision):\(field.id)" }
}

struct VaultLargeTypeView: View {
    let target: VaultLargeTypeTarget
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.layoutDirection) private var direction
    @ScaledMetric(relativeTo: .largeTitle) private var fontSize = 36.0
    @State private var value: RevealedVaultValue?
    @State private var failure: String?
    @State private var reload = 0
    @State private var copiedAt: Date?

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { timeline in
            let text = model.concealed ? nil : value?.value(at: timeline.date)
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    Text(target.item.title).font(.subheadline).foregroundStyle(.secondary)
                    if let text {
                        Text(verbatim: text).font(.system(size: fontSize, weight: .medium, design: .monospaced))
                            .lineSpacing(8).fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .environment(\.layoutDirection, vaultFieldIsLTR(target.field, value: text) ? .leftToRight : direction)
                            .accessibilityIdentifier("vault.large.value")
                        if let remaining = value?.remaining(at: timeline.date) { VaultCodeCountdown(remaining: remaining) }
                    } else if let failure {
                        NoticeBar(text: failure)
                        Button("Retry") { reload &+= 1 }.accessibilityIdentifier("vault.large.retry")
                    } else if value != nil { Text("Expired").foregroundStyle(.secondary) }
                    else { ProgressView().accessibilityIdentifier("vault.large.loading") }
                }.frame(maxWidth: .infinity, alignment: .leading).padding(24)
            }.accessibilityIdentifier("vault.large.scroll").toolbar {
                ToolbarItem(placement: .bottomBar) {
                    Button(copiedAt == nil ? "Copy" : "Copied") { copy() }
                        .disabled(text == nil).accessibilityIdentifier("vault.large.copy")
                }
            }
        }.privateScreen().navigationTitle(vaultFieldTitle(target.field)).navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() }.accessibilityIdentifier("vault.large.done") } }
            .presentationDetents([.large]).presentationDragIndicator(.visible)
            .task(id: "\(model.epoch):\(model.concealed):\(reload)") { await load() }
            .task(id: copiedAt) {
                guard copiedAt != nil else { return }
                try? await Task.sleep(for: .seconds(2))
                if !Task.isCancelled { copiedAt = nil }
            }
            .onChange(of: model.concealed) { _, concealed in if concealed { value = nil; dismiss() } }
            .onChange(of: model.epoch) { _, _ in value = nil; dismiss() }
            .onDisappear { value = nil }
    }

    private func load() async {
        value = nil; failure = nil
        guard !model.concealed, !target.item.reserved, !target.item.deleted else { return }
        let epoch = model.epoch
        let live = target.field.kind == "otp" && !target.historical
        repeat {
            do {
                let requestedAt = Date.now
                let result = try await model.vaultFieldValue(target.item, field: target.field)
                guard epoch == model.epoch, result.generation == model.session?.generation, !model.concealed, !Task.isCancelled else { return }
                let expiry = result.remainingMs.map { requestedAt.addingTimeInterval(Double($0) / 1000) }
                value = RevealedVaultValue(text: result.value, expires: expiry)
                if !live { return }
                try await Task.sleep(for: .seconds(max(0.5, expiry?.timeIntervalSinceNow ?? 30)))
            } catch {
                if epoch == model.epoch && !model.concealed && !Task.isCancelled { failure = vaultReadError(error); value = nil }
                return
            }
        } while !Task.isCancelled
    }

    private func copy() {
        let now = Date.now
        guard !model.concealed, let value, let text = value.value(at: now) else { return }
        model.activity()
        model.clipboard.copy(text, lifetime: min(30, value.expires?.timeIntervalSince(now) ?? 30))
        copiedAt = now
        model.notice = String(localized: value.expires == nil ? "Copied for 30 seconds" : "Copied. The code expires shortly.")
    }
}
