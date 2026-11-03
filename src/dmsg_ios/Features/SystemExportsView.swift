import AuthenticationServices
import FileProvider
import SwiftUI
@preconcurrency import DMsgCore

struct SystemExportsView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @State private var selectingCredentials = false
    @State private var selectingFile = false
    @State private var credentials: [VaultItem] = []
    @State private var files: [LibraryFile] = []
    @State private var biometryOnly = false
    @State private var requiresBiometry = false
    @State private var credentialSnapshot: ExportHeader?
    @State private var fileSnapshot: ExportHeader?
    @State private var snapshotFailed = false
    @State private var autoFillEnabled = false
    @State private var working = false
    @State private var work: Task<Void, Never>?
    private let domain = NSFileProviderDomain(identifier: NSFileProviderDomainIdentifier("net.dmsg.ios.exports"), displayName: "dMsg")
    var body: some View {
        Form {
            if snapshotFailed { SettingsAction("Retry") { refreshSnapshots() } }
            Section("Password AutoFill") {
                LabeledContent("System permission", value: String(localized: autoFillEnabled ? "Allowed" : "Not enabled"))
                ExportSnapshotStatus(header: credentialSnapshot)

                Text("AutoFill receives only the login items you select. The snapshot expires after 24 hours and requires system authentication independently of the workspace. Usernames and website names may appear in system suggestions.").font(.footnote).foregroundStyle(.secondary)
                ForEach(credentials, id: \.id) { item in HStack { Text(item.title); Spacer(); Button("Remove", role: .destructive) { credentials.removeAll { $0.id == item.id } } } }
                SettingsAction("Select login items") { selectingCredentials = true }
                SettingsAction("Replace AutoFill snapshot", prominent: true) { work = Task { await exportCredentials() } }.disabled(credentials.isEmpty)
                Button("Enable dMsg in AutoFill settings") { Task { do { try await ASSettingsHelper.openCredentialProviderAppSettings() } catch { model.report(error) } } }
                SettingsAction("Remove AutoFill snapshot", role: .destructive) { work = Task { await removeSnapshot(.credentials) } }.disabled(credentialSnapshot == nil)
            }
            Section("Files app copies") {
                ExportSnapshotStatus(header: fileSnapshot)
                Text("Only selected, immutable copies appear in the Files app. Access requires system authentication; locking dMsg ends the current access session. Files may retain downloaded copies and filenames, which dMsg cannot recall.").font(.footnote).foregroundStyle(.secondary)
                ForEach(files) { file in VStack(alignment: .leading, spacing: 6) { FileRow(file: file); Button("Remove", role: .destructive) { files.removeAll { $0.id == file.id } } } }
                SettingsAction("Select a file") { selectingFile = true }.disabled(files.count >= 10)
                SettingsAction("Replace Files app snapshot", prominent: true) { work = Task { await exportFiles() } }.disabled(files.isEmpty)
                SettingsAction("Remove Files app snapshot", role: .destructive) { work = Task { await removeSnapshot(.files) } }.disabled(fileSnapshot == nil)
            }
            Toggle("Biometrics only for exported copies", isOn: $biometryOnly).disabled(requiresBiometry)
            Text("These exports never contain account root keys or device signing keys. Changes to the source items require a new snapshot.").font(.footnote).foregroundStyle(.secondary)
            if working { ProgressView() }
        }.disabled(working).privateScreen().navigationTitle("System integrations")
            .sheet(isPresented: $selectingCredentials) { NavigationStack { AutoFillSelectionView(initial: credentials) { values in credentials = values } } }
            .sheet(isPresented: $selectingFile) { NavigationStack { AttachmentPicker(folder: "root") { file in
                if !files.contains(where: { $0.id == file.id }) { files.append(file) }; selectingFile = false
            } } }
            .onDisappear { work?.cancel() }
            .task(id: scenePhase) {
                guard scenePhase == .active else { return }
                let epoch = model.epoch
                refreshSnapshots()
                let state = await ASCredentialIdentityStore.shared.state()
                guard epoch == model.epoch, !Task.isCancelled else { return }
                autoFillEnabled = state.isEnabled
                do { if let value = try await model.engine?.hardwareSecurity(), epoch == model.epoch, !Task.isCancelled { let state: HardwareSecurityStatus = try model.decode(value); requiresBiometry = state.boundBiometryOnly == true; biometryOnly = requiresBiometry } }
                catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
            }
    }
    private func refreshSnapshots() {
        if model.uiTesting { return }
        snapshotFailed = false
        func read(_ purpose: ExportPurpose) -> ExportHeader? {
            do {
                let header = try SystemExports(purpose).header(checkExpiry: false)
                return header.accountHash == model.session?.account.map(IncomingShares.accountHash) ? header : nil
            } catch let error as CocoaError where error.code == .fileReadNoSuchFile { return nil }
            catch { snapshotFailed = true; model.report(error); return nil }
        }
        credentialSnapshot = read(.credentials); fileSnapshot = read(.files)
    }

    private func removeSnapshot(_ purpose: ExportPurpose) async {
        guard !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            try SystemExports(purpose).invalidate()
            if purpose == .credentials { try await ASCredentialIdentityStore.shared.removeAllCredentialIdentities() }
            else { try await NSFileProviderManager.remove(domain) }
            guard epoch == model.epoch, !Task.isCancelled else { return }
            refreshSnapshots(); model.notice = String(localized: "Saved")
        } catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
    }

    private func exportCredentials() async {
        struct Entry: Decodable { let title: String; let username: String; let password: String; let url: String }
        guard let engine = model.engine, let account = model.session?.account, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do {
            await model.integrationReset?.value
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let snapshot = try await engine.exportAutofill(items: credentials.map { CredentialSelection(id: $0.id, revision: $0.revision) })
            let entries: [Entry] = try model.decode(snapshot)
            let selected = entries.map { entry in CredentialInput(title: entry.title, username: entry.username, password: entry.password, origin: Self.origin(entry.url)) }
            let store = try SystemExports(.credentials)
            let prepared = try await Task.detached { try store.prepareCredentials(selected, account: account, expires: UInt64(Date.now.timeIntervalSince1970 * 1000) + 86_400_000) }.value
            guard epoch == model.epoch, !Task.isCancelled else { store.discard(prepared); return }
            do { try store.publish(prepared, biometryOnly: biometryOnly) } catch { store.discard(prepared); throw error }
            let metadata = try store.credentialMetadata(OpenExport(header: prepared.header, key: prepared.key))
            let identities = metadata.filter { !$0.origin.isEmpty }.map { ASPasswordCredentialIdentity(serviceIdentifier: ASCredentialServiceIdentifier(identifier: $0.origin, type: .URL), user: $0.username, recordIdentifier: prepared.header.id + ":" + $0.id) }
            try await ASCredentialIdentityStore.shared.replaceCredentialIdentities(identities)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            refreshSnapshots(); model.notice = String(localized: "AutoFill snapshot updated")
        } catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
    }
    private static func origin(_ text: String) -> String {
        guard var url = URLComponents(string: text), ["https", "http"].contains(url.scheme?.lowercased()), url.host != nil else { return "" }
        url.user = nil; url.password = nil; url.path = ""; url.query = nil; url.fragment = nil
        return url.string ?? ""
    }
    private func exportFiles() async {
        struct Opened: Decodable { let path: String; let nonce: String }
        guard let engine = model.engine, let account = model.session?.account, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        var nonces: [String] = []
        defer { let cleanup = nonces; Task { for nonce in cleanup { try? await engine.closeFile(nonce: nonce) } } }
        do {
            await model.integrationReset?.value
            guard epoch == model.epoch, !Task.isCancelled else { return }
            guard files.count <= 10, files.allSatisfy({ $0.size <= 100 * 1024 * 1024 }),
                  files.reduce(UInt64(0), { $0 + $1.size }) <= 200 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
            var inputs: [FileExportInput] = []
            for file in files {
                let snapshot = try await coreOperation { try await engine.openFile(id: file.id, version: file.current, operation: $0) }
                let opened: Opened = try model.decode(snapshot); nonces.append(opened.nonce)
                inputs.append(FileExportInput(url: URL(fileURLWithPath: opened.path), name: file.name, sha256: file.sha256, size: file.size))
            }
            let store = try SystemExports(.files), values = inputs
            let prepared = try await Task.detached { try store.prepareFiles(values, account: account, expires: UInt64(Date.now.timeIntervalSince1970 * 1000) + 86_400_000) }.value
            guard epoch == model.epoch, !Task.isCancelled else { store.discard(prepared); return }
            do { try store.publish(prepared, biometryOnly: biometryOnly); try store.unlockFiles(OpenExport(header: prepared.header, key: prepared.key)) }
            catch { store.discard(prepared); throw error }
            try await NSFileProviderManager.add(domain)
            await signalFiles()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            refreshSnapshots(); model.notice = String(localized: "Files app snapshot updated")
        } catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
    }
    private func signalFiles() async {
        try? await NSFileProviderManager(for: domain)?.signalEnumerator(for: .rootContainer)
        try? await NSFileProviderManager(for: domain)?.signalEnumerator(for: .workingSet)
    }
}

struct AutoFillSelectionView: View {
    let initial: [VaultItem]
    let selected: ([VaultItem]) -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var items: [VaultItem] = []
    @State private var chosen: [String: VaultItem] = [:]
    @State private var query = ""
    @State private var more = false
    @State private var working = false
    @State private var failed = false
    @State private var request = UUID()

    var body: some View {
        List {
            LabeledContent("Selected", value: chosen.count.formatted()).accessibilityIdentifier("autofill.selected")
            SettingsLoadStatus(loading: working, failed: failed) { Task { await load(reset: true) } }
            ForEach(items, id: \.id) { item in
                Toggle(item.title, isOn: Binding(get: { chosen[item.id] != nil }, set: {
                    if $0 { chosen[item.id] = item } else { chosen.removeValue(forKey: item.id) }
                })).disabled(item.reserved || (chosen[item.id] == nil && chosen.count >= 50))
            }
            if !working && !failed && items.isEmpty { ContentUnavailableView.search(text: query) }
            if more { SettingsAction("Load more") { Task { await load(reset: false) } }.disabled(working) }
        }.privateScreen().navigationTitle("Select login items")
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always))
            .safeAreaInset(edge: .bottom) {
                Button { selected(chosen.values.sorted { $0.title < $1.title }); dismiss() } label: { Text("Done").frame(maxWidth: .infinity) }
                    .buttonStyle(.borderedProminent).controlSize(.large).frame(maxWidth: .infinity)
                    .padding().background(Theme.paper).accessibilityIdentifier("autofill.done")
            }
            .onAppear { chosen = Dictionary(uniqueKeysWithValues: initial.map { ($0.id, $0) }) }
            .task(id: query) { try? await Task.sleep(for: .milliseconds(200)); if !Task.isCancelled { await load(reset: true) } }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
    }

    private func load(reset: Bool) async {
        let token = UUID(), epoch = model.epoch, query = query; request = token
        working = true; failed = false; defer { if request == token { working = false } }
        #if DEBUG
        if model.uiTesting {
            items = model.vault.filter { $0.kind == "login" && (query.isEmpty || $0.title.localizedCaseInsensitiveContains(query)) }
            more = false; return
        }
        #endif
        guard let engine = model.engine else { failed = true; return }
        do {
            let page = try await engine.vaultPage(query: query, kind: "login", favoritesOnly: false, offset: reset ? 0 : UInt32(items.count), limit: 60)
            guard !Task.isCancelled, epoch == model.epoch, request == token else { return }
            items = reset ? page.items : items + page.items; more = page.hasMore
        } catch { if epoch == model.epoch, !Task.isCancelled, request == token { failed = true } }
    }
}

private struct ExportSnapshotStatus: View {
    let header: ExportHeader?
    var body: some View {
        if let header {
            LabeledContent("Status", value: String(localized: Double(header.expires) > Date.now.timeIntervalSince1970 * 1000 ? "Active" : "Expired"))
            if let count = header.count { LabeledContent("Items", value: count.formatted()) }
            if let created = header.createdAt { LabeledContent("Updated") { Text(Date(timeIntervalSince1970: Double(created) / 1000), style: .relative) } }
            LabeledContent("Expires") { Text(Date(timeIntervalSince1970: Double(header.expires) / 1000), style: .relative) }
        } else { Text("No snapshot").foregroundStyle(.secondary) }
    }
}
