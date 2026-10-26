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
            else { Button(reportID == nil ? "Submit report" : "Retry this report") { Task { await submit() } } }
            Button("Block this account", role: .destructive) { Task { await model.blockAccount(target.account, blocked: true) } }
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

struct SafetySettingsView: View {
    @Environment(SessionModel.self) private var model
    @State private var reports: [Report] = []
    struct Report: Decodable, Identifiable { let id: String; let state: String; let resolution: String? }
    var body: some View {
        List {
            Section("Blocked accounts") {
                ForEach(model.blockedAccounts.sorted(), id: \.self) { account in
                    VStack(alignment: .leading, spacing: 8) {
                        FingerprintRow(title: "Account ID", value: account)
                        Button("Unblock") { Task { await model.blockAccount(account, blocked: false) } }
                    }
                }
                if model.blockedAccounts.isEmpty { Text("No blocked accounts").foregroundStyle(.secondary) }
                Text("Blocked senders are hidden on this device. Online blocks also prevent new direct contact and invitations; existing shared copies are not recalled.").font(.footnote).foregroundStyle(.secondary)
            }
            Section("Your reports") {
                ForEach(reports) { report in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(report.state == "reviewed" ? "Reviewed" : report.state == "pending" ? "Waiting to submit" : "Received for review")
                        if let resolution = report.resolution { Text(resolution).font(.footnote) }
                        if report.state == "pending" { Button("Retry this report") { Task {
                            do { if let engine = model.engine { _ = try await coreOperation { try await engine.submitReport(id: report.id, operation: $0) } }; await load() }
                            catch { model.report(error) }
                        } } }
                    }
                }
            }
            Link("Contact support", destination: URL(string: "https://dmsg.net/_/support")!)
        }.privateScreen().navigationTitle("Safety and support").task { await load() }.refreshable { await load() }
    }
    private func load() async {
        await model.loadBlockedAccounts()
        do {
            if let local = try await model.engine?.reportHistory(refresh: false) { reports = try model.decode(local) }
            if let remote = try await model.engine?.reportHistory(refresh: true) {
                let updates: [Report] = try model.decode(remote)
                let ids = Set(updates.map(\.id)); reports = updates + reports.filter { !ids.contains($0.id) }
            }
        } catch { model.report(error) }
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
                    Text(job.state == "deleted" ? "Cloud deletion completed" : "Deletion is in progress")
                    FingerprintRow(title: "Request ID", value: job.id)
                    Button("Check deletion status") { Task { await checkStatus() } }
                }
                if job.state == "deleted" { Button("Remove this account's local data", role: .destructive) { removingLocal = true } }
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
                Button("Request account deletion", role: .destructive) { confirming = true }
                    .disabled(working || confirmation != preview.details.account || preview.details.pendingAccountOperation || !preview.details.ownedChannels.isEmpty || ((!preview.details.wallets.isEmpty || !preview.details.walletsAvailable) && !reviewedWallets))
            } else if working { ProgressView() }
            else { Button("Review deletion") { Task { await load() } } }
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
