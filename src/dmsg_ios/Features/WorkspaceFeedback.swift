import SwiftUI

struct WorkspaceStatus {
    let key: String
    let title: LocalizedStringKey
    let symbol: String
    var urgent = false
    var count: UInt64 = 0
    var presentsFeedback = true

    @MainActor static func current(_ model: SessionModel, at date: Date) -> WorkspaceStatus? {
        guard model.session?.account != nil else { return nil }
        if model.recoveryWarning?.pending == true {
            return WorkspaceStatus(key: "recovery:" + (model.recoveryWarning?.digest ?? "pending"), title: "Account recovery is pending", symbol: "exclamationmark.shield", urgent: true)
        }
        if let count = model.session?.conflicts, count > 0 {
            return WorkspaceStatus(key: "conflicts:\(count)", title: "Review sync conflicts", symbol: "arrow.triangle.branch", urgent: true, count: count)
        }
        if ContentSyncPolicy.needsAttention(model.contentSync), let error = model.contentSync?.error {
            return WorkspaceStatus(key: "sync-error:" + error, title: "Sync paused", symbol: "exclamationmark.icloud", urgent: true, count: model.session?.pendingSync ?? 0, presentsFeedback: false)
        }
        if model.unlocked && model.contentSync?.initialized != true {
            return WorkspaceStatus(key: "initial-sync", title: model.contentSync?.running == true ? "First sync" : "Waiting for first sync", symbol: "arrow.triangle.2.circlepath")
        }
        // With automatic sync on, waiting versions leave by themselves; a failure
        // that lasts is reported above, with their count.
        if !model.automaticSyncEnabled, let count = model.session?.pendingSync, count > 0 {
            return WorkspaceStatus(key: "pending:\(count)", title: model.contentSync?.running == true ? "Syncing…" : "Waiting to sync", symbol: "arrow.triangle.2.circlepath", count: count)
        }
        if model.recoveryWarning == nil || model.recoveryWarning?.isStale(at: date) == true || model.recoveryRefreshFailed {
            return WorkspaceStatus(key: "recovery-check", title: "Recovery status needs checking", symbol: "shield.lefthalf.filled")
        }
        return nil
    }
}

/// Always occupies the same toolbar space; status transitions never resize lists.
struct WorkspaceStatusButton: View {
    @Environment(SessionModel.self) private var model
    @State private var showing = false
    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { timeline in
            let status = WorkspaceStatus.current(model, at: timeline.date)
            Button { showing = true } label: {
                Image(systemName: status?.symbol ?? "checkmark.icloud")
                    .foregroundStyle(status?.urgent == true ? .orange : Theme.forest)
                    .frame(minWidth: 28, minHeight: 44)
                    .overlay(alignment: .topTrailing) {
                        if let count = status?.count, count > 0 {
                            Text(count > 99 ? "99+" : count.formatted()).font(.system(size: 9, weight: .semibold))
                                .padding(.horizontal, 3).padding(.vertical, 1).foregroundStyle(Theme.paper)
                                .background(Theme.forest, in: Capsule()).accessibilityHidden(true)
                        } else if status != nil { Circle().fill(.orange).frame(width: 6, height: 6).padding(.top, 6) }
                    }
            }.accessibilityLabel(status?.title ?? "Account synchronization")
                .accessibilityValue((status?.count ?? 0) > 0 ? (status?.count ?? 0).formatted() : "")
                .accessibilityIdentifier("workspace.status")
        }.sheet(isPresented: $showing) { NavigationStack { WorkspaceStatusView() } }
    }
}

struct WorkspaceStatusView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { timeline in
            List {
                if RecoveryNotice.isVisible(model, at: timeline.date) { RecoveryNotice() }
                if model.unlocked {
                    ContentSyncStatusView()
                    Button("Sync now") { Task { await model.synchronize() } }.disabled(model.busy || model.syncCoolingDown).accessibilityIdentifier("sync.retry")
                    if (model.session?.conflicts ?? 0) > 0 { NavigationLink("Review sync conflicts") { ConflictsView() } }
                }
            }
        }.privateScreen().navigationTitle("Account synchronization").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() }.accessibilityIdentifier("workspace.status.done") } }
    }
}

/// Brief feedback uses an overlay, while a persistent toolbar entry keeps all
/// pending work reachable. Sync failures stay in that entry's details instead
/// of interrupting browsing with an automatic notice or VoiceOver announcement.
struct WorkspaceFeedback: View {
    @Environment(SessionModel.self) private var model
    @State private var active: String?
    @State private var dismissed: String?
    @State private var showing = false
    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { timeline in
            let status = WorkspaceStatus.current(model, at: timeline.date).flatMap { $0.presentsFeedback ? $0 : nil }
            Group {
                if let status, active == status.key, dismissed != status.key,
                   !model.concealed, model.notice == nil, model.notifications.presented == nil {
                    HStack(spacing: 10) {
                        Image(systemName: status.symbol).foregroundStyle(status.urgent ? .orange : Theme.forest).accessibilityHidden(true)
                        Button { showing = true; dismissed = status.key } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(status.title).font(.subheadline.weight(.medium))
                                if status.count > 0 { Text(status.count.formatted()).font(.caption.monospacedDigit()) }
                            }.frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                        }.buttonStyle(.plain)
                        Button("Dismiss", systemImage: "xmark") { dismissed = status.key }
                            .labelStyle(.iconOnly).frame(width: 44, height: 44)
                    }.padding(.leading, 14).padding(.trailing, 4).padding(.vertical, 4)
                        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 14))
                        .shadow(color: .black.opacity(0.08), radius: 8, y: 2)
                        .padding(.horizontal, 16).padding(.bottom, model.unlocked ? 82 : 16)
                        .accessibilityIdentifier("workspace.feedback")
                }
            }.task(id: status?.key) {
                active = status?.key
                if status?.urgent == true {
                    let message = model.recoveryWarning?.pending == true ? String(localized: "Account recovery is pending")
                        : String(localized: "Review sync conflicts")
                    UIAccessibility.post(notification: .announcement, argument: message)
                }
                if status?.urgent != true {
                    try? await Task.sleep(for: .seconds(4))
                    if !Task.isCancelled { active = nil }
                }
            }
        }.sheet(isPresented: $showing) { NavigationStack { WorkspaceStatusView() } }
    }
}

/// Placed in the active screen, including presented sheets, so copies and saves
/// cannot report success underneath the sheet where the action took place.
struct TransientNotice: View {
    @Environment(SessionModel.self) private var model
    var body: some View {
        let revision = model.noticeRevision
        if let notice = model.notice, !model.concealed, model.notifications.presented == nil {
            NoticeBar(text: notice)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 12))
                .shadow(color: .black.opacity(0.08), radius: 8, y: 2)
                .padding(12).allowsHitTesting(false)
                .accessibilityElement(children: .ignore).accessibilityLabel(notice).accessibilityAddTraits(.isStaticText)
                .accessibilityIdentifier("workspace.notice")
                .onChange(of: revision, initial: true) { _, _ in UIAccessibility.post(notification: .announcement, argument: notice) }
                .task(id: revision) {
                    try? await Task.sleep(for: .seconds(4))
                    if !Task.isCancelled && model.noticeRevision == revision { model.notice = nil }
                }
        }
    }
}
