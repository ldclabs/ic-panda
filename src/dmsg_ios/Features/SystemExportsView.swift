import AuthenticationServices
import FileProvider
import SwiftUI
@preconcurrency import DMsgCore

struct SystemExportsView: View {
    @Environment(SessionModel.self) private var model
    @State private var selectingCredentials = false
    @State private var selectingFile = false
    @State private var credentials: [VaultItem] = []
    @State private var files: [LibraryFile] = []
    @State private var biometryOnly = false
    @State private var requiresBiometry = false
    @State private var working = false
    @State private var work: Task<Void, Never>?
    private let domain = NSFileProviderDomain(identifier: NSFileProviderDomainIdentifier("net.dmsg.ios.exports"), displayName: "dMsg")
    var body: some View {
        Form {
            Section("Password AutoFill") {
                Text("AutoFill receives only the login items you select. The snapshot expires after 24 hours and requires system authentication independently of the workspace. Usernames and website names may appear in system suggestions.").font(.footnote).foregroundStyle(.secondary)
                ForEach(credentials, id: \.id) { item in Text(item.title) }
                Button("Select login items") { selectingCredentials = true }
                Button("Replace AutoFill snapshot") { work = Task { await exportCredentials() } }.disabled(credentials.isEmpty)
                Button("Enable dMsg in AutoFill settings") { Task { do { try await ASSettingsHelper.openCredentialProviderAppSettings() } catch { model.report(error) } } }
                Button("Remove AutoFill snapshot", role: .destructive) { Task {
                    do { try SystemExports(.credentials).invalidate(); try await ASCredentialIdentityStore.shared.removeAllCredentialIdentities() }
                    catch { model.report(error) }
                } }
            }
            Section("Files app copies") {
                Text("Only selected, immutable copies appear in the Files app. Access requires system authentication; locking dMsg ends the current access session. Files may retain downloaded copies and filenames, which dMsg cannot recall.").font(.footnote).foregroundStyle(.secondary)
                ForEach(files) { file in FileRow(file: file) }
                Button("Select a file") { selectingFile = true }.disabled(files.count >= 10)
                Button("Replace Files app snapshot") { work = Task { await exportFiles() } }.disabled(files.isEmpty)
                Button("Remove Files app snapshot", role: .destructive) { Task {
                    do { try SystemExports(.files).invalidate(); try await NSFileProviderManager.remove(domain) } catch { model.report(error) }
                } }
            }
            Toggle("Biometrics only for exported copies", isOn: $biometryOnly).disabled(requiresBiometry)
            Text("These exports never contain account root keys or device signing keys. Changes to the source items require a new snapshot.").font(.footnote).foregroundStyle(.secondary)
            if working { ProgressView() }
        }.disabled(working).privateScreen().navigationTitle("System integrations")
            .sheet(isPresented: $selectingCredentials) { NavigationStack { AutoFillSelectionView { values in credentials = values } } }
            .sheet(isPresented: $selectingFile) { NavigationStack { AttachmentPicker(folder: "root") { file in
                if !files.contains(where: { $0.id == file.id }) { files.append(file) }; selectingFile = false
            } } }
            .onDisappear { work?.cancel() }
            .task {
                do { if let value = try await model.engine?.hardwareSecurity() { let state: HardwareSecurityStatus = try model.decode(value); requiresBiometry = state.boundBiometryOnly == true; biometryOnly = requiresBiometry } }
                catch { model.report(error) }
            }
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
            model.notice = String(localized: "AutoFill snapshot updated")
        } catch { if epoch == model.epoch { model.report(error) } }
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
            await signalFiles(); model.notice = String(localized: "Files app snapshot updated")
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func signalFiles() async {
        try? await NSFileProviderManager(for: domain)?.signalEnumerator(for: .rootContainer)
        try? await NSFileProviderManager(for: domain)?.signalEnumerator(for: .workingSet)
    }
}

struct AutoFillSelectionView: View {
    let selected: ([VaultItem]) -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var items: [VaultItem] = []
    @State private var chosen: Set<String> = []
    @State private var query = ""
    @State private var more = false
    @State private var working = false
    var body: some View {
        List {
            ForEach(items, id: \.id) { item in
                Toggle(item.title, isOn: Binding(get: { chosen.contains(item.id) }, set: { if $0 { chosen.insert(item.id) } else { chosen.remove(item.id) } })).disabled(item.reserved || (!chosen.contains(item.id) && chosen.count >= 50))
            }
            if more { Button("Load more") { Task { await load(reset: false) } }.disabled(working) }
        }.navigationTitle("Select login items").searchable(text: $query)
            .task(id: query) { try? await Task.sleep(for: .milliseconds(200)); if !Task.isCancelled { await load(reset: true) } }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Done") { selected(items.filter { chosen.contains($0.id) }); dismiss() }.disabled(chosen.isEmpty) }
            }
    }
    private func load(reset: Bool) async {
        guard let engine = model.engine else { return }; working = true; defer { working = false }; let query = query
        do {
            let page = try await engine.vaultPage(query: query, kind: "login", favoritesOnly: false, offset: reset ? 0 : UInt32(items.count), limit: 60)
            guard !Task.isCancelled, self.query == query else { return }; items = reset ? page.items : items + page.items; more = page.hasMore
            if reset { chosen = [] }
        } catch { model.report(error) }
    }
}
