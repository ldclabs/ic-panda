import SwiftUI
@preconcurrency import DMsgCore
import StoreKit

struct ReportTarget: Identifiable {
    let kind: String; let account: String; let resource: String; var sequence: UInt64? = nil
    var id: String { kind + ":" + resource + ":" + String(sequence ?? 0) }
}

struct ReportContentView: View {
    let target: ReportTarget
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var reason = "spam"
    @State private var note = ""
    @State private var disclose = false
    @State private var reportID: String?
    @State private var working = false
    @State private var received = false
    var body: some View {
        Form {
            FingerprintRow(title: "Reported account", value: target.account)
            Picker("Reason", selection: $reason) {
                Text("Spam").tag("spam"); Text("Harassment").tag("harassment")
                Text("Illegal content").tag("illegal"); Text("Other").tag("other")
            }
            TextField("Describe the issue", text: $note, axis: .vertical).lineLimit(3...8)
            if target.kind != "account" { Toggle("Include this message's text", isOn: $disclose) }
            NoticeBar(text: String(localized: "Your report and any text you explicitly include will be sent to the moderation team. Other private content and keys are not included."))
            if received { Label("Report received for review", systemImage: "checkmark.circle") }
            else { SettingsAction(LocalizedStringKey(reportID == nil ? "Submit report" : "Retry this report"), prominent: true) { Task { await submit() } } }
            SettingsAction("Block this account", role: .destructive) { Task { await model.blockAccount(target.account, blocked: true) } }
            Link("Contact support", destination: URL(string: "https://dmsg.net/_/support")!)
        }.disabled(working).privateScreen().navigationTitle("Report content")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .onChange(of: reason) { _, _ in reportID = nil; received = false }
            .onChange(of: note) { _, _ in reportID = nil; received = false }
            .onChange(of: disclose) { _, _ in reportID = nil; received = false }
    }
    private func submit() async {
        struct Prepared: Decodable { let id: String }
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; model.activity(); defer { working = false }
        do {
            if reportID == nil {
                let value = try await engine.prepareReport(input: ReportInput(kind: target.kind, target: target.account, resource: target.resource,
                    sequence: target.sequence, reason: reason, note: note, disclose: disclose))
                guard epoch == model.epoch else { return }
                let prepared: Prepared = try model.decode(value); reportID = prepared.id
            }
            guard let reportID else { return }
            _ = try await coreOperation { try await engine.submitReport(id: reportID, operation: $0) }
            guard epoch == model.epoch else { return }; received = true
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct MessageSettingsView: View {
    var body: some View {
        List {
            NavigationLink("Inbox settings") { InboxSettingsView() }
            NavigationLink("Blocked accounts") { BlockedAccountsView() }.accessibilityIdentifier("messages.blocked")
            NavigationLink("Your reports") { ReportsView() }.accessibilityIdentifier("messages.reports")
        }.privateScreen().navigationTitle("Message settings")
    }
}

struct BlockedAccountsView: View {
    @Environment(SessionModel.self) private var model
    @State private var loading = false
    @State private var failed = false
    @State private var working = false

    var body: some View {
        List {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            ForEach(model.blockedAccounts.sorted(), id: \.self) { account in
                VStack(alignment: .leading, spacing: 8) {
                    FingerprintRow(title: "Account ID", value: account)
                    Button("Unblock") { Task { await unblock(account) } }.disabled(working)
                }
            }
            if !loading && !failed && model.blockedAccounts.isEmpty { ContentUnavailableView("No blocked accounts", systemImage: "person.slash") }
            Text("Blocked senders are hidden on this device. Online blocks also prevent new direct contact and invitations; existing shared copies are not recalled.")
                .font(.footnote).foregroundStyle(.secondary)
        }.privateScreen().navigationTitle("Blocked accounts").task { await load() }.refreshable { await load() }
    }

    private func load() async {
        guard !loading else { return }; loading = true
        failed = !(await model.loadBlockedAccounts()); loading = false
    }

    private func unblock(_ account: String) async {
        guard !working else { return }; working = true; defer { working = false }
        if await model.blockAccount(account, blocked: false) { model.notice = String(localized: "Saved") }
    }
}

struct ReportsView: View {
    @Environment(SessionModel.self) private var model
    @State private var reports: [Report] = []
    @State private var loading = false
    @State private var failed = false
    @State private var working = false
    struct Report: Decodable, Identifiable { let id: String; let state: String; let resolution: String? }

    var body: some View {
        List {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            ForEach(reports) { report in
                VStack(alignment: .leading, spacing: 6) {
                    Text(LocalizedStringKey(report.state == "reviewed" ? "Reviewed" : report.state == "pending" ? "Waiting to submit" : "Received for review"))
                    FingerprintRow(title: "Request ID", value: report.id)
                    if let resolution = report.resolution { Text(resolution).font(.footnote) }
                    if report.state == "pending" { Button("Retry this report") { Task { await retry(report) } }.disabled(working) }
                }
            }
            if !loading && !failed && reports.isEmpty { ContentUnavailableView("No reports", systemImage: "flag") }
        }.privateScreen().navigationTitle("Your reports").task { await load() }.refreshable { await load() }
    }

    private func load() async {
        guard !loading else { return }; loading = true; failed = false
        let epoch = model.epoch; defer { loading = false }
        if model.uiTesting { return }
        do {
            guard let engine = model.engine else { throw CancellationError() }
            let local: [Report] = try model.decode(try await engine.reportHistory(refresh: false))
            guard epoch == model.epoch, !Task.isCancelled else { return }; reports = local
            let remote: [Report] = try model.decode(try await engine.reportHistory(refresh: true))
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let ids = Set(remote.map(\.id)); reports = remote + reports.filter { !ids.contains($0.id) }
        } catch { if epoch == model.epoch, !Task.isCancelled { failed = true } }
    }

    private func retry(_ report: Report) async {
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; defer { working = false }
        do {
            _ = try await coreOperation { try await engine.submitReport(id: report.id, operation: $0) }
            if epoch == model.epoch { await load() }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct AccountDeletionView: View {
    @Environment(SessionModel.self) private var model
    @State private var preview: Preview?
    @State private var job: Job?
    @State private var confirmation = ""
    @State private var reviewedWallets = false
    @State private var confirming = false
    @State private var removingLocal = false
    @State private var managingSubscriptions = false
    @State private var working = false
    @State private var requestID: String?
    struct Preview: Decodable {
        struct Details: Decodable { let account: String; let pendingSync: UInt64; let pendingAccountOperation: Bool; let ownedChannels: [Channel]; let wallets: [Wallet]; let walletsAvailable: Bool }
        struct Channel: Decodable, Identifiable { let id: String; let name: String }
        struct Wallet: Decodable, Identifiable { let wallet: String; let name: String; var id: String { wallet } }
        let digest: String; let details: Details
    }
    struct Job: Decodable { let id: String; let state: String }
    struct Status: Decodable { let deletion: Job? }
    var body: some View {
        Form {
            NoticeBar(text: String(localized: "Deleting your dMsg service account permanently removes its cloud ciphertext, private service data, device notification registrations and public avatar origin copies. It cannot erase blockchain records, other recipients' copies, or data held by other services."))
            Button("Manage Apple subscriptions") { managingSubscriptions = true }
            Text("Deleting your dMsg account does not cancel an Apple subscription. Cancel it in your Apple account settings to stop renewal.").font(.footnote).foregroundStyle(.secondary)
            if let job {
                Section("Deletion status") {
                    Text(LocalizedStringKey(job.state == "deleted" ? "Cloud deletion completed" : "Deletion is in progress"))
                    FingerprintRow(title: "Request ID", value: job.id)
                    SettingsAction("Check deletion status") { Task { await checkStatus() } }
                }
                if job.state == "deleted" { SettingsAction("Remove this account's local data", role: .destructive) { removingLocal = true } }
            } else if let preview {
                FingerprintRow(title: "Account ID", value: preview.details.account)
                if preview.details.pendingSync > 0 { NoticeBar(text: String(localized: "This device has changes that have not synced. Save anything you need before deleting the account.")) }
                if preview.details.pendingAccountOperation { NoticeBar(text: String(localized: "Resolve the pending account operation before deleting this account.")) }
                if !preview.details.ownedChannels.isEmpty {
                    Section("Transfer or archive owned conversations first") {
                        ForEach(preview.details.ownedChannels) { channel in NavigationLink(channel.name) { ChannelSettingsView(channel: channel.id) } }
                    }
                }
                if !preview.details.wallets.isEmpty || !preview.details.walletsAvailable {
                    Section("Wallet access") {
                        if !preview.details.walletsAvailable { NoticeBar(text: String(localized: "Wallet access could not be checked. Verify it independently before deleting.")) }
                        ForEach(preview.details.wallets) { wallet in Text(wallet.name) }
                        Toggle("I have secured my wallet access and external credentials", isOn: $reviewedWallets)
                        Text("Deletion does not transfer funds. Removing local keys and cloud copies can permanently remove this device's access to wallets and external credentials.").font(.footnote).foregroundStyle(.secondary)
                    }
                }
                TextField("Type this account ID to confirm", text: $confirmation).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight)
                SettingsAction("Request account deletion", role: .destructive) { confirming = true }
                    .disabled(working || confirmation != preview.details.account || preview.details.pendingAccountOperation || !preview.details.ownedChannels.isEmpty || ((!preview.details.wallets.isEmpty || !preview.details.walletsAvailable) && !reviewedWallets))
            } else if working { ProgressView() }
            else { SettingsAction("Review deletion") { Task { await load() } } }
            NavigationLink("Privacy policy") { PrivacyPolicyView() }
        }.privateScreen().navigationTitle("Delete account").task { await load() }
            .refreshable { await load() }.manageSubscriptionsSheet(isPresented: $managingSubscriptions)
            .confirmationDialog("Permanently delete this dMsg service account?", isPresented: $confirming) {
                Button("Delete account", role: .destructive) { Task { await request() } }
            } message: { Text("This cannot be undone. Signing out alone does not delete an account.") }
            .confirmationDialog("Remove this account's data from this device?", isPresented: $removingLocal) {
                Button("Remove local data", role: .destructive) { Task { await model.removeLocalAccount() } }
            }
    }
    private func checkStatus() async {
        do { if let value = try await model.engine?.accountDeletionStatus() { let status: Status = try model.decode(value); job = status.deletion } }
        catch { model.report(error) }
    }
    private func load() async {
        guard !working else { return }; working = true; defer { working = false }
        await checkStatus()
        guard job == nil else { return }
        do { if let value = try await model.engine?.accountDeletionPreview() { preview = try model.decode(value) } }
        catch { model.report(error) }
    }
    private func request() async {
        guard let preview, let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; model.activity(); defer { working = false }
        if requestID == nil { requestID = NativeApple.requestID() }
        guard let requestID else { return }
        do {
            let result = try await engine.requestAccountDeletion(digest: preview.digest, walletAccessReviewed: reviewedWallets, requestId: requestID)
            guard epoch == model.epoch else { return }; job = try model.decode(result)
        } catch { if epoch == model.epoch { await checkStatus(); model.report(error) } }
    }
}
