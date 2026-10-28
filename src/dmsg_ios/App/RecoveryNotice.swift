import SwiftUI

struct RecoveryObservation: Decodable, Equatable {
    let account: String
    let pending: Bool
    let digest: String?
    let observedAtMs: UInt64
    let stale: Bool

    func isStale(at date: Date = .now) -> Bool {
        stale || date.timeIntervalSince1970 * 1000 > Double(observedAtMs) + 120_000
    }
}

/// No account ID, recovery token or private recovery details on the lock view.
struct RecoveryNotice: View {
    @Environment(SessionModel.self) private var model
    @State private var reviewing = false
    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { timeline in
            let observation = model.recoveryWarning
            if model.session?.account != nil, !model.concealed,
               observation?.pending == true || observation == nil ||
                observation?.isStale(at: timeline.date) == true || model.recoveryRefreshFailed {
                HStack(alignment: .center, spacing: 10) {
                    Image(systemName: observation?.pending == true ? "exclamationmark.shield" : "shield.lefthalf.filled")
                        .foregroundStyle(observation?.pending == true ? .orange : Theme.forest)
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(LocalizedStringKey(observation?.pending == true ? "Account recovery is pending" : "Recovery status needs checking"))
                            .font(.subheadline.weight(.semibold))
                        if observation?.pending == true {
                            Text(LocalizedStringKey(model.unlocked ? "Review the request and dispute it if it is not yours." : "Unlock dMsg to review this request."))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        if model.recoveryRefreshFailed || observation?.isStale(at: timeline.date) == true {
                            Text("The last verified observation is kept. Current status could not be confirmed.")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }.frame(maxWidth: .infinity, alignment: .leading)
                    if model.unlocked, observation?.pending == true {
                        Button("Review") { reviewing = true }.frame(minWidth: 44, minHeight: 44)
                    } else {
                        Button { Task { await model.refreshRecoveryWarning() } } label: {
                            if model.checkingRecovery { ProgressView() }
                            else { Image(systemName: "arrow.clockwise") }
                        }.frame(minWidth: 44, minHeight: 44).accessibilityLabel("Check recovery status")
                            .disabled(model.checkingRecovery)
                    }
                }.padding(.horizontal, 16).padding(.vertical, 6).background(Theme.surface)
                    .accessibilityIdentifier("recovery.warning")
            }
        }
        .sheet(isPresented: $reviewing) { NavigationStack { AccountSecurityView() } }
        .onChange(of: model.epoch) { _, _ in reviewing = false }
    }
}
