import SwiftUI
@preconcurrency import DMsgCore

/// Detailed status reached through the toolbar or account settings.
struct ContentSyncStatusView: View {
    @Environment(SessionModel.self) private var model

    var body: some View {
        let status = model.contentSync
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                if status?.running == true { ProgressView().controlSize(.small) }
                Text(title(status)).font(.subheadline.weight(.semibold))
            }
            if let status, status.running {
                Text(stage(status.stage)).font(.footnote).foregroundStyle(.secondary)
            }
            if let status, status.running || !status.initialized {
                if status.total > 0 {
                    ProgressView(value: Double(status.entries), total: Double(status.total))
                    Text(verbatim: "\(status.entries.formatted()) / \(status.total.formatted())")
                        .font(.caption.monospacedDigit()).accessibilityIdentifier("sync.entries")
                }
                if status.downloadedBytes > 0 {
                    Label(ByteCountFormatter.string(fromByteCount: Int64(clamping: status.downloadedBytes), countStyle: .file), systemImage: "arrow.down")
                        .font(.caption).accessibilityLabel(Text("Downloaded") + Text(verbatim: " ") + Text(ByteCountFormatter.string(fromByteCount: Int64(clamping: status.downloadedBytes), countStyle: .file)))
                }
            }
            if let error = status?.error, status?.running != true {
                Text(failure(error)).font(.footnote).foregroundStyle(.secondary)
            }
            if let pending = model.session?.pendingSync, pending > 0 {
                LabeledContent("Waiting to sync", value: pending.formatted()).font(.footnote)
            }
            if let retry = status?.retryAtMs {
                dateRow("Next retry", retry)
            }
            if let success = status?.lastSuccessAtMs {
                dateRow("Last successful sync", success)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 4)
        .accessibilityElement(children: .contain).accessibilityIdentifier("sync.status")
    }

    private func dateRow(_ label: LocalizedStringKey, _ value: UInt64) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
            Text(Date(timeIntervalSince1970: Double(value) / 1000), style: .relative)
        }.font(.caption).foregroundStyle(.secondary)
    }

    private func title(_ status: ContentSyncStatus?) -> LocalizedStringKey {
        if status?.running == true { return status?.initialized == true ? "Syncing…" : "First sync" }
        if status?.error != nil { return "Sync paused" }
        return status?.initialized == true ? "Account synchronization" : "Waiting for first sync"
    }

    private func stage(_ stage: String) -> LocalizedStringKey {
        switch stage {
        case "security": "Checking permission…"
        case "export_start", "export_page", "delta_page": "Reading account inventory…"
        case "download": "Downloading account content…"
        case "apply": "Verifying account content…"
        default: "Syncing…"
        }
    }

    private func failure(_ error: String) -> LocalizedStringKey {
        switch error {
        case "SYNC_BUSY": "Sync is waiting for the server."
        case "QUOTA_EXCEEDED": "Uploads are paused. Free cloud storage and retry; receiving continues."
        case "NETWORK_UNAVAILABLE", "UNAVAILABLE": "Connection unavailable. Your saved content stays on this device."
        case "INTEGRITY_FAILED", "RECOVERY_INCOMPLETE": "A record or device key could not be verified. Your workspace has not been erased."
        default: "Showing saved content. The latest sync did not finish."
        }
    }
}
