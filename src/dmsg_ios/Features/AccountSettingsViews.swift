import SwiftUI

struct SyncSettingsView: View {
    @Environment(SessionModel.self) private var model
    @State private var local: LocalOnly?

    var body: some View {
        Form {
            Section {
                Toggle("Automatic foreground sync", isOn: Binding(get: { model.automaticSyncEnabled }, set: { model.setAutomaticSync($0) }))
                ContentSyncStatusView()
                SettingsAction(LocalizedStringKey(model.synchronizing ? "Syncing…" : "Sync now"), systemImage: "arrow.triangle.2.circlepath") {
                    Task { await model.synchronize(); local = await LocalOnly.load(model) }
                }.disabled(model.busy || model.syncCoolingDown).accessibilityIdentifier("account.sync")
            }
            LocalOnlySection(value: local, always: true)
            Section("Cloud backups") {
                NavigationLink("Cloud backups") { List { CloudBackupSection() }.privateScreen().navigationTitle("Cloud backups").task { await model.loadCloudBackups(refresh: true) } }
                if model.loadingCloudBackups { ProgressView() }
                else if model.cloudBackupsFailed { NoticeBar(text: String(localized: "Cloud backup list could not be refreshed. Retry in Files.")) }
                else if model.cloudBackupsKnown {
                    LabeledContent("Backups found", value: model.cloudBackups.count.formatted())
                }
                Text("Sync checks account content and cloud backups. Open conversations or the inbox to receive messages; restore files separately.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
        }.privateScreen().navigationTitle("Account synchronization")
            .task { local = await LocalOnly.load(model) }
    }
}

struct AdvancedAccountView: View {
    var body: some View {
        List {
            NavigationLink { AgentsView() } label: { Label("Agents", glyph: .agents) }
            NavigationLink { WalletsView() } label: { Label("Multisig wallets", glyph: .coins) }
            NavigationLink { BrowserConnectionsView() } label: { Label("Connected browsers", glyph: .browser) }
            NavigationLink { HardwareSecurityView() } label: { Label("Hardware security", systemImage: "cpu") }.accessibilityIdentifier("account.hardware")
            NavigationLink { SecretActivityView() } label: { Label("Activity on this device", glyph: .audit) }
        }.privateScreen().navigationTitle("Advanced")
    }
}

struct AboutDMsgView: View {
    var body: some View {
        List {
            LabeledContent("Version", value: "\(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "") (\(Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? ""))")
            NavigationLink("Privacy policy") { PrivacyPolicyView() }
            Link("Terms of service", destination: URL(string: "https://dmsg.net/_/terms")!)
            Link("Contact support", destination: URL(string: "https://dmsg.net/_/support")!)
        }.privateScreen().navigationTitle("About dMsg")
    }
}

struct LocalAccountRemovalView: View {
    @Environment(SessionModel.self) private var model
    @State private var local: LocalOnly?
    @State private var working = false
    @State private var confirming = false

    var body: some View {
        Form {
            Text("Remove this device's encrypted workspace and saved login. Your service account and other devices remain available. Unsynced changes and files without a backup will be lost.")
            LocalOnlySection(value: local, always: true)
            if local == nil { SettingsAction("Retry") { Task { local = await LocalOnly.load(model) } } }
            SettingsAction("Remove this account's local data", role: .destructive) { confirming = true }
                .disabled(working || local == nil).accessibilityIdentifier("account.remove-local")
            if working { ProgressView() }
        }.privateScreen().navigationTitle("Remove local data")
            .task { local = await LocalOnly.load(model) }
            .confirmationDialog("Remove this account's data from this device?", isPresented: $confirming) {
                Button("Remove local data", role: .destructive) { Task {
                    guard !working else { return }; working = true
                    await model.removeLocalAccount(); working = false
                } }
            } message: { Text("Anything that exists only in this copy, such as files that were never backed up, is lost for good.") }
    }
}
