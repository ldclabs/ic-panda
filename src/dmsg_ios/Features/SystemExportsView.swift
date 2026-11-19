import AuthenticationServices
import FileProvider
import SwiftUI
@preconcurrency import DMsgCore

struct SystemIntegrationsView: View {
    var body: some View {
        List {
            NavigationLink { SystemExportsView(section: .autoFill) } label: { Label("Password AutoFill", systemImage: "key") }
                .accessibilityIdentifier("system.autofill")
            NavigationLink { SystemExportsView(section: .files) } label: { Label("Files app copies", systemImage: "folder") }
                .accessibilityIdentifier("system.files")
        }.privateScreen().navigationTitle("System integrations")
    }
}

enum SystemExportSection { case autoFill, files }

struct SystemExportsView: View {
    var section: SystemExportSection
    @Environment(SessionModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @State private var selectingFile = false
    @State private var files: [LibraryFile] = []
    @State private var biometryOnly = false
    @State private var requiresBiometry = false
    @State private var fileSnapshot: ExportHeader?
    @State private var snapshotFailed = false
    @State private var autoFillEnabled = false
    @State private var working = false
    @State private var work: Task<Void, Never>?
    private let domain = NSFileProviderDomain(identifier: NSFileProviderDomainIdentifier("net.dmsg.ios.exports"), displayName: "dMsg")
    var body: some View {
        Form {
            if snapshotFailed { SettingsAction("Retry") { refreshSnapshots() } }
            switch section {
            case .autoFill: autoFill
            case .files: filesApp
            }
            Toggle("Biometrics only for exported copies", isOn: Binding(get: { biometryOnly }, set: { value in
                if section == .files || model.uiTesting { biometryOnly = value }
                else { work = Task { await setAutoFillBiometry(value) } }
            })).disabled(requiresBiometry)
            if section == .files { Text("These exports never contain account root keys or device signing keys. Files app copies do not follow changes to the source files; those need a new snapshot.").font(.footnote).foregroundStyle(.secondary) }
            if working { ProgressView() }
        }.disabled(working).privateScreen().navigationTitle(section == .autoFill ? "Password AutoFill" : "Files app copies").navigationBarTitleDisplayMode(.inline)
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
                do {
                    let policy = try await exportPolicy()
                    let required = policy.biometryOnly
                    guard epoch == model.epoch, !Task.isCancelled else { return }
                    requiresBiometry = required
                    // A policy refresh can strengthen the user's choice but
                    // cannot relax it after Settings or system authentication.
                    if required { biometryOnly = true }
                    else if section == .autoFill, !model.uiTesting { biometryOnly = policy.autofillBiometryOnly }
                }
                catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
            }
    }

    /// System permission and the automatically maintained login catalog.
    @ViewBuilder private var autoFill: some View {
        Section {
            if autoFillEnabled { LabeledContent("System permission", value: String(localized: "Allowed")) }
            else {
                LabeledContent("System permission", value: String(localized: "Not enabled"))
                SettingsAction("Enable dMsg in AutoFill settings", prominent: true) { Task { do { try await ASSettingsHelper.openCredentialProviderAppSettings() } catch { model.report(error) } } }
            }
        }
        Section {
            Text("Choose a login when an app or website asks for a password.")
            if let header = model.autoFillHeader, let count = header.count { LabeledContent("Items", value: count.formatted()) }
            if let issue = model.autoFillIssue {
                Text(issue).foregroundStyle(.secondary)
                SettingsAction("Retry") { work = Task { await model.refreshAutoFill() } }
            }
        }
    }

    private func setAutoFillBiometry(_ value: Bool) async {
        guard let engine = model.engine else { return }
        working = true
        defer { working = false }
        do {
            try await engine.setAutofillBiometry(biometryOnly: value)
            biometryOnly = value
            await model.refreshAutoFill()
        } catch { model.report(error) }
    }

    @ViewBuilder private var filesApp: some View {
        Section {
            ForEach(files) { file in VStack(alignment: .leading, spacing: 6) { FileRow(file: file); Button("Remove", role: .destructive) { files.removeAll { $0.id == file.id } }.buttonStyle(.borderless) } }
            SettingsAction("Select a file") { selectingFile = true }.disabled(files.count >= 10)
        } footer: {
            Text("Only selected, immutable copies appear in the Files app. Access requires system authentication; locking dMsg ends the current access session. Files may retain downloaded copies and filenames, which dMsg cannot recall.")
        }
        Section {
            ExportSnapshotStatus(header: fileSnapshot)
            SettingsAction("Update Files app copies", prominent: !files.isEmpty) { work = Task { await exportFiles() } }.disabled(files.isEmpty)
            SettingsAction("Remove Files app snapshot", role: .destructive) { work = Task { await removeFileSnapshot() } }.disabled(fileSnapshot == nil)
        }
    }

    private struct ExportPolicy: Decodable { let biometryOnly: Bool; let autofillBiometryOnly: Bool }

    private func exportPolicy() async throws -> ExportPolicy {
        #if DEBUG
        if model.uiTesting { return ExportPolicy(biometryOnly: false, autofillBiometryOnly: false) }
        #endif
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        return try model.decode(try await engine.systemExportPolicy())
    }

    private func refreshSnapshots() {
        if model.uiTesting || section != .files { return }
        snapshotFailed = false
        func read(_ purpose: ExportPurpose) -> ExportHeader? {
            do {
                let header = try SystemExports(purpose).header(checkExpiry: false)
                return header.accountHash == model.session?.account.map(IncomingShares.accountHash) ? header : nil
            } catch ExportAccessError.notConfigured { return nil }
            catch let error as CocoaError where error.code == .fileReadNoSuchFile { return nil }
            catch { snapshotFailed = true; model.report(error); return nil }
        }
        fileSnapshot = read(.files)
    }

    private func removeFileSnapshot() async {
        guard !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            try SystemExports(.files).invalidate()
            try await NSFileProviderManager.remove(domain)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            refreshSnapshots(); model.notice = String(localized: "Saved")
        } catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
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
            try store.publish(prepared, biometryOnly: biometryOnly)
            refreshSnapshots()
            // Files can still authenticate the published snapshot if the short session failed.
            let access = Result { try store.unlockFiles(OpenExport(header: prepared.header, key: prepared.key)) }
            try await NSFileProviderManager.add(domain)
            await signalFiles()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            try access.get()
            model.notice = String(localized: "Files app snapshot updated")
        } catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
    }
    private func signalFiles() async {
        try? await NSFileProviderManager(for: domain)?.signalEnumerator(for: .rootContainer)
        try? await NSFileProviderManager(for: domain)?.signalEnumerator(for: .workingSet)
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
