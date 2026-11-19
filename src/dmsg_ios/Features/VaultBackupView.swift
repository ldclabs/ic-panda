@preconcurrency import DMsgCore
import SwiftUI
import UniformTypeIdentifiers

private struct BackupRow: Decodable, Identifiable {
    let id, title, kind: String
    let files: [String]
    let synced: Bool
    var saveState: String? = nil
}
private struct BackupInventory: Decodable {
    struct Inventory: Decodable {
        struct FileInfo: Decodable {
            struct Metadata: Decodable { let size: UInt64 }
            let meta: Metadata
            let available: Bool
        }
        let items: [BackupRow]
        let files: [String: FileInfo?]
        let managed, trash, conflicts: Int
    }
    let inventory: Inventory
    let at: UInt64
}
private struct BackupPreview: Decodable {
    struct Row: Decodable, Identifiable {
        let index: UInt32
        var id: UInt32 { index }
        let title, kind, status: String
        let files: [Int?]
    }
    let digest, source, account: String
    let at: UInt64
    let items: [Row]
    let files, omitted: Int
}
private struct BackupResult: Decodable {
    let items, files, omitted: Int
    let skipped: Int?
}
private struct BackupProgress: Decodable { let bytes, items: Int? }
private struct BackupReceipt: Decodable { let receipt: BackupResult? }

struct VaultBackupView: View {
    @Environment(SessionModel.self) private var model
    private enum Step { case home, export, pick, preview, waiting, ready, done }
    @State private var step = Step.home
    @State private var id = ""
    @State private var generation = 0
    @State private var task: Task<Void, Never>?
    @State private var working = false
    @State private var inventory: BackupInventory?
    @State private var preview: BackupPreview?
    @State private var selected = Set<String>()
    @State private var picks = Set<UInt32>()
    @State private var attachments = true
    @State private var password = ""
    @State private var confirmation = ""
    @State private var showPassword = false
    @State private var staged: URL?
    @State private var exported: URL?
    @State private var choosing = false
    @State private var saving = false
    @State private var pendingApproval: ApprovalView?
    @State private var showApproval = false
    @State private var result: BackupResult?
    @State private var folders: [FolderItem] = []
    @State private var folder = "root"
    @State private var error = ""
    @State private var processed = ""
    private var owner: String { model.session?.account ?? "" }
    private var receiptKey: String { "vault-backup-receipt-" + owner }
    private var rows: [BackupRow] {
        inventory?.inventory.items.filter { selected.contains($0.id) } ?? []
    }
    private var fileIDs: Set<String> { Set(rows.flatMap(\.files)) }
    private var unavailable: Bool {
        fileIDs.contains { (inventory?.inventory.files[$0] ?? nil)?.available != true }
    }
    private var readyPassword: Bool {
        password.count >= 6 && password.precomposedStringWithCanonicalMapping.utf8.count <= 256
            && password == confirmation
    }
    var body: some View {
        List {
            LabeledContent("Account", value: owner)
            switch step {
            case .home:
                Button("Export encrypted backup") { begin(false) }
                Button("Import from backup") { begin(true) }
                if let saved = UserDefaults.standard.string(forKey: receiptKey) {
                    Button("Check status") {
                        run { engine, turn in
                            let value: BackupReceipt = try model.decode(
                                try await engine.vaultBackupStatus(id: saved))
                            try check(turn)
                            if let receipt = value.receipt {
                                result = receipt
                                step = .done
                            } else {
                                error = String(
                                    localized: "No import was committed. Choose the backup again.")
                            }
                            UserDefaults.standard.removeObject(forKey: receiptKey)
                        }
                    }
                }
            case .export:
                exportForm
            case .pick:
                Button("Choose a file") { choosing = true }
                if let staged {
                    Text(staged.lastPathComponent).lineLimit(3)
                    passphrase(false)
                    Button("Review") { inspect(staged) }.disabled(password.isEmpty)
                }
            case .preview:
                if let preview { importForm(preview) }
            case .waiting:
                if pendingApproval != nil {
                    Button("Review") { showApproval = true }
                    Button("Continue") { claimed() }
                }
            case .ready:
                if let result { counts(result) }
                Button("Save") { if exported != nil { saving = true } else { prepareSave() } }
            case .done:
                if let result {
                    Section("Saved locally") { counts(result) }
                    if result.skipped != nil {
                        Text(
                            "Vault entries follow normal sync. Attachments are saved in Files on this device."
                        )
                    }
                }
            }
            if working {
                ProgressView()
                if !processed.isEmpty { Text(processed).font(.footnote) }
            }
            if !error.isEmpty { NoticeBar(text: error) }
        }
        .disabled(working)
        .privateScreen().navigationTitle("Backup and restore").navigationBarTitleDisplayMode(
            .inline
        )
        .task(id: working) {
            processed = ""
            guard working, let engine = model.engine else { return }
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard working, !Task.isCancelled else { return }
                if !id.isEmpty, let snap = try? await engine.vaultBackupStatus(id: id),
                    let value: BackupProgress = try? model.decode(snap), let bytes = value.bytes,
                    bytes > 0
                {
                    processed = ByteCountFormatter.string(
                        fromByteCount: Int64(bytes), countStyle: .file)
                }
            }
        }
        .fileImporter(isPresented: $choosing, allowedContentTypes: [.data]) { selection in
            if case .success(let url) = selection { stage(url) }
        }
        .sheet(isPresented: $showApproval, onDismiss: { if step == .waiting { claimed() } }) {
            if let pendingApproval {
                NavigationStack { ApprovalDetailView(request: pendingApproval) }
            }
        }
        .sheet(isPresented: $saving) {
            if let exported {
                BackupSavePicker(file: exported) { success in
                    saving = false
                    if success {
                        step = .done
                        cleanOutput()
                    }
                }
            }
        }
        .onChange(of: model.epoch) { _, _ in reset() }
        .onChange(of: model.lifecycle) { _, phase in
            if phase == .background || phase == .protectedDataUnavailable { reset() }
        }
        .onDisappear { reset() }
    }

    @ViewBuilder private var exportForm: some View {
        if let inventory {
            let list = inventory.inventory
            Section {
                Text(
                    "Only current items are included. History, trash, drafts, conflict copies and dMsg-managed keys are excluded."
                )
                Text(Date(timeIntervalSince1970: Double(inventory.at) / 1000), format: .dateTime)
                LabeledContent("Trash", value: list.trash.formatted())
                LabeledContent("Conflicts", value: list.conflicts.formatted())
                LabeledContent("Managed by dMsg", value: list.managed.formatted())
                LabeledContent(
                    "Waiting to sync", value: rows.filter { $0.saveState == "pending" || ($0.saveState == nil && !$0.synced) }.count.formatted())
                Button("Select all") { selected = Set(list.items.map(\.id)) }
                Button("Clear selection") { selected.removeAll() }
                ForEach(list.items) { row in
                    Toggle(
                        isOn: Binding(
                            get: { selected.contains(row.id) },
                            set: {
                                if $0 { selected.insert(row.id) } else { selected.remove(row.id) }
                            })
                    ) {
                        VStack(alignment: .leading) {
                            Text(verbatim: row.title)
                            Text(vaultKindName(row.kind)).font(.footnote).foregroundStyle(
                                .secondary)
                        }
                    }
                }
            }
            if !fileIDs.isEmpty {
                Toggle("Include linked files", isOn: $attachments)
                let bytes = fileIDs.compactMap { (list.files[$0] ?? nil)?.meta.size }.reduce(0, +)
                LabeledContent(
                    "Attachments",
                    value: fileIDs.count.formatted() + " · "
                        + ByteCountFormatter.string(
                            fromByteCount: Int64(clamping: bytes), countStyle: .file))
            }
            if unavailable && attachments {
                NoticeBar(
                    text: String(
                        localized:
                            "Some linked files are unavailable. Download them or export items only."
                    ))
            }
            if rows.contains(where: { $0.kind == "wallet" }) {
                NoticeBar(text: String(localized: "Includes secrets saved in wallet items."))
            }
            passphrase(true)
            Button("Generate password") {
                if let engine = model.engine { password = engine.vaultBackupPassword() }
            }
            SecureField("Confirm backup password", text: $confirmation).textInputAutocapitalization(
                .never
            ).autocorrectionDisabled()
            if !confirmation.isEmpty && confirmation != password {
                Text("The two backup passwords do not match.").font(.footnote).foregroundStyle(.red)
            }
            Button("Export encrypted backup") { request() }.disabled(
                selected.isEmpty || !readyPassword || (attachments && unavailable))
        }
    }

    @ViewBuilder private func passphrase(_ creating: Bool) -> some View {
        HStack {
            Group {
                if showPassword {
                    TextField("Backup password", text: $password)
                } else {
                    SecureField("Backup password", text: $password)
                }
            }.textInputAutocapitalization(.never).autocorrectionDisabled()
            Button { showPassword.toggle() } label: {
                Image(systemName: showPassword ? "eye.slash" : "eye")
            }
            .buttonStyle(.borderless)
            .accessibilityLabel(showPassword ? Text("Hide") : Text("Show"))
        }
        if creating {
            Text("At least 6 characters. Keep this password separately. dMsg cannot recover it.")
                .font(
                    .footnote)
        }
    }

    @ViewBuilder private func importForm(_ data: BackupPreview) -> some View {
        LabeledContent("Source", value: data.source)
        LabeledContent("Destination", value: data.account)
        Text(Date(timeIntervalSince1970: Double(data.at) / 1000), format: .dateTime)
        LabeledContent("Omitted attachments", value: data.omitted.formatted())
        LabeledContent(
            "Add as new", value: data.items.filter { $0.status == "new" }.count.formatted())
        LabeledContent(
            "Already present", value: data.items.filter { $0.status == "same" }.count.formatted())
        LabeledContent(
            "Review",
            value: data.items.filter { $0.status == "changed" || $0.status == "deleted" }.count
                .formatted())
        Button("Add as new") { picks = Set(data.items.filter { $0.status == "new" }.map(\.index)) }
        Button("Clear selection") { picks.removeAll() }
        ForEach(data.items) { row in
            Toggle(
                isOn: Binding(
                    get: { picks.contains(row.index) },
                    set: { if $0 { picks.insert(row.index) } else { picks.remove(row.index) } })
            ) {
                VStack(alignment: .leading) {
                    Text(verbatim: row.title)
                    HStack {
                        Text(vaultKindName(row.kind))
                        Text("Attachments")
                        Text(row.files.compactMap { $0 }.count, format: .number)
                    }.font(.footnote).foregroundStyle(.secondary)
                    let omitted = row.files.filter { $0 == nil }.count
                    if omitted > 0 {
                        LabeledContent("Omitted attachments", value: omitted.formatted()).font(.footnote)
                    }
                    Text(reason(row.status)).font(.footnote).foregroundStyle(.secondary)
                }
            }.disabled(row.status == "same")
        }
        let files = Set(
            data.items.filter { picks.contains($0.index) }.flatMap { $0.files.compactMap { $0 } })
        if !files.isEmpty {
            Picker("Folder", selection: $folder) {
                Text("Files").tag("root")
                ForEach(folders.filter { $0.id != "root" }) { Text($0.name).tag($0.id) }
            }
        }
        LabeledContent("Selected", value: picks.count.formatted())
        LabeledContent("Attachments", value: files.count.formatted())
        if picks.isEmpty {
            Button("Done") { reset() }
        } else {
            Button("Import from backup") { commit(data.digest) }
        }
    }

    private func reason(_ status: String) -> String {
        switch status {
        case "same": String(localized: "Already present")
        case "changed": String(localized: "Different content · save a copy")
        case "deleted": String(localized: "Deleted here · save a copy")
        default: String(localized: "Add as new")
        }
    }

    @ViewBuilder private func counts(_ value: BackupResult) -> some View {
        LabeledContent("Items", value: value.items.formatted())
        LabeledContent("Attachments", value: value.files.formatted())
        LabeledContent("Omitted attachments", value: value.omitted.formatted())
    }

    private func check(_ turn: Int) throws {
        guard turn == generation, !Task.isCancelled, model.session?.state == "unlocked" else {
            throw CancellationError()
        }
    }

    private func run(_ action: @escaping @MainActor (MobileEngine, Int) async throws -> Void) {
        guard !working, let engine = model.engine else { return }
        let turn = generation
        working = true
        error = ""
        task = Task { @MainActor in
            defer { if generation == turn { working = false } }
            do { try await action(engine, turn) } catch is CancellationError {} catch {
                if generation == turn {
                    if step == .pick, case MobileError.Failure(let code) = error,
                        code == "INTEGRITY_FAILED"
                    {
                        password = ""
                        self.error = String(
                            localized:
                                "Cannot unlock this backup. Check the password or choose a complete backup file."
                        )
                    } else {
                        self.error = vaultReadError(error)
                        model.report(error)
                    }
                }
            }
        }
    }

    private func begin(_ importing: Bool) {
        run { engine, turn in
            let token = try await engine.vaultBackupBegin(importing: importing)
            guard turn == generation else {
                try? await engine.vaultBackupCancel(id: token)
                return
            }
            id = token
            if importing {
                step = .pick
                choosing = true
            } else {
                let view: BackupInventory = try model.decode(
                    try await engine.vaultBackupInventory(id: token))
                try check(turn)
                inventory = view
                selected = Set(view.inventory.items.map(\.id))
                step = .export
            }
        }
    }

    private func request() {
        run { engine, turn in
            let raw = try await engine.vaultBackupRequest(
                id: id, items: selected.sorted(), attachments: attachments, password: password,
                confirmation: confirmation)
            let approval: ApprovalView = try model.decode(raw)
            let current: ApprovalView = try model.decode(try await engine.approval(id: approval.id))
            try check(turn)
            password = ""
            confirmation = ""
            pendingApproval = current
            step = .waiting
            showApproval = true
        }
    }

    private func claimed() {
        guard let approval = pendingApproval else { return }
        run { engine, turn in
            let current: ApprovalView = try model.decode(try await engine.approval(id: approval.id))
            try check(turn)
            guard current.state == "done" else {
                if !["awaiting_user", "executing"].contains(current.state) { reset() }
                return
            }
            let summary: BackupResult
            do {
                summary = try model.decode(
                    try await engine.vaultBackupExport(id: id, approval: approval.id))
            } catch {
                try check(turn)
                step = .home
                throw error
            }
            try check(turn)
            result = summary
            step = .ready
        }
    }

    private func prepareSave() {
        run { engine, turn in
            let path = try await engine.vaultBackupOutput(id: id)
            try check(turn)
            exported = URL(fileURLWithPath: path)
            saving = true
        }
    }

    private func stage(_ url: URL) {
        guard let files = model.files else { return }
        let account = owner
        run { _, turn in
            let copy = try await Task.detached {
                try files.importDocument(url, account: account, purpose: "vault-backup")
            }.value
            do { try check(turn) } catch {
                try? files.finishImport(copy)
                throw error
            }
            if let old = staged { try? files.finishImport(old) }
            staged = copy
        }
    }

    private func inspect(_ copy: URL) {
        run { engine, turn in
            let handle =
                owner + "/" + copy.deletingLastPathComponent().lastPathComponent + "/"
                + copy.lastPathComponent
            let data: BackupPreview = try model.decode(
                try await engine.vaultBackupInspect(id: id, name: handle, password: password))
            let available: [FolderItem] = try model.decode(try await engine.fileFolders())
            try check(turn)
            preview = data
            folders = available
            picks = Set(data.items.filter { $0.status == "new" }.map(\.index))
            password = ""
            step = .preview
            try? model.files?.finishImport(copy)
            staged = nil
        }
    }

    private func commit(_ digest: String) {
        run { engine, turn in
            UserDefaults.standard.set(id, forKey: receiptKey)
            let value: BackupResult = try model.decode(
                try await engine.vaultBackupCommit(
                    id: id, digest: digest, items: picks.sorted(), folder: folder))
            try check(turn)
            result = value
            step = .done
            UserDefaults.standard.removeObject(forKey: receiptKey)
            _ = await model.updateSession()
            await model.loadVault()
            if value.files > 0 { await model.loadFiles(quiet: true) }
            _ = model.refreshAutoFill(force: true)
        }
    }

    private func cleanOutput() {
        if !id.isEmpty, let engine = model.engine {
            let token = id
            Task { try? await engine.vaultBackupCancel(id: token) }
        }
        exported = nil
    }

    private func reset() {
        generation += 1
        task?.cancel()
        task = nil
        cleanOutput()
        if let staged { try? model.files?.finishImport(staged) }
        staged = nil
        id = ""
        password = ""
        confirmation = ""
        showPassword = false
        approvalReset()
        inventory = nil
        preview = nil
        result = nil
        selected = []
        picks = []
        folders = []
        attachments = true
        folder = "root"
        working = false
        error = ""
        step = .home
    }

    private func approvalReset() {
        pendingApproval = nil
        showApproval = false
        saving = false
        choosing = false
    }
}

private struct BackupSavePicker: UIViewControllerRepresentable {
    let file: URL
    let finished: (Bool) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(finished) }
    func makeUIViewController(context: Context) -> UIDocumentPickerViewController {
        let picker = UIDocumentPickerViewController(forExporting: [file], asCopy: true)
        picker.delegate = context.coordinator
        return picker
    }
    func updateUIViewController(_ controller: UIDocumentPickerViewController, context: Context) {}
    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        let finished: (Bool) -> Void
        init(_ finished: @escaping (Bool) -> Void) { self.finished = finished }
        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            finished(false)
        }
        func documentPicker(
            _ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]
        ) { finished(!urls.isEmpty) }
    }
}
