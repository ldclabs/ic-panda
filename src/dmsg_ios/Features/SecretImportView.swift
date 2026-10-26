import SwiftUI
@preconcurrency import DMsgCore

struct SecretPackageImportView: View {
    struct ChannelSource: Identifiable { let channel: String; let sequence: UInt64; var id: String { channel + ":" + String(sequence) } }
    var source: ChannelSource? = nil
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var link = ""
    @State private var password = ""
    @State private var preview: SecretPackagePreview?
    @State private var fields: Set<String> = []
    @State private var notes = false
    @State private var attachments: Set<Int> = []
    @State private var folder = "root"
    @State private var folderName = String(localized: "Files")
    @State private var choosingFolder = false
    @State private var working = false
    @State private var request: ApprovalView?
    @State private var completed: [String]?
    @State private var previewTask: Task<Void, Never>?
    private var hasSelection: Bool { !fields.isEmpty || notes || !attachments.isEmpty }

    var body: some View {
        Form {
            if source == nil {
                Section("Secret package link") {
                    TextField("Paste secret package link", text: $link, axis: .vertical)
                        .lineLimit(2...4).textInputAutocapitalization(.never).autocorrectionDisabled()
                    SecureField("Password, if required", text: $password).textInputAutocapitalization(.never)
                    Text("Preview downloads and verifies the package on this device and uses a download session. Choose the receiving device before continuing.")
                        .font(.footnote).foregroundStyle(.secondary)
                    Button("Preview package") { previewTask?.cancel(); previewTask = Task { await inspect() } }
                        .disabled(link.isEmpty || working)
                }
            }
            if working { ProgressView() }
            if source != nil && preview == nil && !working { Button("Retry preview") { previewTask = Task { await inspect() } } }
            if let preview {
                Section {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(preview.preview.title).font(.title3.bold())
                        Text(Date(timeIntervalSince1970: Double(preview.preview.createdAt) / 1000), style: .date)
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    if let origin = preview.channelSource {
                        FingerprintRow(title: "Sender account", value: origin.senderAccount)
                        FingerprintRow(title: "Channel ID", value: origin.channelId)
                    }
                    if !(preview.alreadyImported ?? []).isEmpty { NoticeBar(text: String(localized: "This package has already been imported on this device. Existing copies remain independent.")) }
                }
                Section("Select fields to import") {
                    ForEach(preview.preview.fields) { field in
                        Toggle(field.label.isEmpty ? vaultFieldName(field.id) : field.label,
                               isOn: Binding(get: { fields.contains(field.id) }, set: { if $0 { fields.insert(field.id) } else { fields.remove(field.id) } }))
                    }
                    if preview.preview.notes { Toggle("Notes", isOn: $notes) }
                    if preview.preview.fields.contains(where: { fields.contains($0.id) && $0.type == "otp" }) {
                        NoticeBar(text: String(localized: "Selected OTP fields include the reusable seed, not just a current code."))
                    }
                }
                if !preview.preview.attachments.isEmpty {
                    Section("Select attachments to import") {
                        ForEach(Array(preview.preview.attachments.enumerated()), id: \.offset) { index, file in
                            Toggle(isOn: Binding(get: { attachments.contains(index) }, set: { if $0 { attachments.insert(index) } else { attachments.remove(index) } })) {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(file.name).lineLimit(2)
                                    Text(ByteCountFormatter.string(fromByteCount: Int64(clamping: file.size), countStyle: .file)).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                        Button { choosingFolder = true } label: { LabeledContent("Destination folder", value: folderName) }
                    }
                }
                NoticeBar(text: String(localized: "Import creates new independent items. Files stay on this device and inherit the selected folder's access grants. Nothing is selected automatically."))
                Button("Review selected import") { Task { await importRequest() } }.disabled(!hasSelection || working)
                DisclosureGroup("Verified package fingerprint") { FingerprintRow(title: "SHA-256", value: preview.digest) }
            }
            if let completed {
                Section("Already imported") {
                    ForEach(completed, id: \.self) { Text($0).font(.caption.monospaced()).textSelection(.enabled) }
                }
            }
            Text("The mobile package limit is 8 MiB. Preview checks the package before you choose what to import.")
                .font(.footnote).foregroundStyle(.secondary)
        }.disabled(working).privateScreen().navigationTitle("Import secret package").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if source != nil { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            }
            .task { if source != nil { await inspect() } }
            .onChange(of: link) { _, _ in clearPreview() }
            .onChange(of: password) { _, _ in if preview != nil { clearPreview() } }
            .onDisappear { previewTask?.cancel() }
            .sheet(isPresented: $choosingFolder) {
                NavigationStack { SecretFolderPicker { id, name in folder = id; folderName = name } }
            }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
    }

    private func clearPreview() { previewTask?.cancel(); preview = nil; fields = []; notes = false; attachments = []; completed = nil }

    private func inspect() async {
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch, selectedLink = link, selectedPassword = password
        working = true; model.activity(); defer { working = false }
        do {
            let result: ObjectSnapshot
            if let source {
                result = try await coreOperation { try await engine.secretChannelPackagePreview(channel: source.channel, sequence: source.sequence, operation: $0) }
            } else {
                result = try await coreOperation { try await engine.secretPackagePreview(link: selectedLink, password: selectedPassword.isEmpty ? nil : selectedPassword, operation: $0) }
            }
            guard epoch == model.epoch, !Task.isCancelled, source != nil || (link == selectedLink && password == selectedPassword) else { return }
            let next: SecretPackagePreview = try model.decode(result)
            preview = next; fields = []; notes = false; attachments = []; completed = nil
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func importRequest() async {
        struct Completion: Decodable { let state: String; let ids: [String]? }
        guard let preview, let engine = model.engine, !working, hasSelection else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do {
            let selection = SecretPackageSelection(packageId: preview.packageId, digest: preview.digest,
                fields: fields.sorted(), includeNotes: notes, attachments: attachments.sorted().map(UInt32.init), folder: folder)
            let result = try await engine.secretPackageImportRequest(selection: selection)
            guard epoch == model.epoch else { return }
            let done: Completion? = try? model.decode(result)
            if let done, done.state == "done" {
                completed = done.ids ?? []; await model.refresh()
            } else { request = try model.decode(result) }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct SecretFolderPicker: View {
    let selected: (String, String) -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var folder = "root"
    @State private var listing: LibraryListing?
    var body: some View {
        List {
            if let listing {
                if let parent = listing.folder {
                    Button("Parent folder", systemImage: "arrow.up") { folder = parent.parent ?? "root" }
                }
                ForEach(listing.folders) { child in
                    Button { folder = child.id } label: { Label(child.name, systemImage: "folder") }
                }
                Button("Use this folder") { selected(folder, listing.folder?.name ?? String(localized: "Files")); dismiss() }
                    .buttonStyle(.borderedProminent)
            } else { ProgressView() }
        }.navigationTitle(listing?.folder?.name ?? String(localized: "Files"))
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task(id: folder) {
                listing = nil; let target = folder, epoch = model.epoch
                do {
                    guard let result = try await model.engine?.files(folder: target), epoch == model.epoch, target == folder, !Task.isCancelled else { return }
                    listing = try model.decode(result)
                } catch { if epoch == model.epoch { model.report(error) } }
            }
    }
}
