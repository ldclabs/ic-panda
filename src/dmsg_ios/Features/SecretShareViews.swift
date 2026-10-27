import SwiftUI
@preconcurrency import DMsgCore

struct SecretSnapshotComposer: View {
    let item: VaultItem
    var initialChannel: String? = nil
    @Environment(SessionModel.self) private var model
    @State private var fields: Set<String> = []
    @State private var notes = false
    @State private var attachmentIDs: Set<String> = []
    @State private var attachments: SecretAttachmentListing?
    @State private var destination = "link"
    @State private var initialized = false
    @State private var duration: UInt64 = 86_400_000
    @State private var sessions = 1
    @State private var password = ""
    @State private var working = false
    @State private var job: SecretShareJob?
    @State private var work: Task<Void, Never>?
    private var hasSelection: Bool { !fields.isEmpty || notes || !attachmentIDs.isEmpty }

    var body: some View {
        Form {
            Section {
                Text(item.title).font(.title3.bold())
                Text("Choose exactly what this independent snapshot contains. Source edits will not change copies already sent.")
                    .font(.footnote).foregroundStyle(.secondary)
                Picker("Destination", selection: $destination) {
                    Text("Share link").tag("link")
                    ForEach(model.channels, id: \.id) { channel in Text(channel.name).tag(channel.id) }
                }.accessibilityIdentifier("secret.snapshot.destination")
            }
            Section("Select fields to share") {
                ForEach(item.fields, id: \.id) { field in
                    Toggle(field.label.isEmpty ? vaultFieldName(field.id) : field.label,
                           isOn: Binding(get: { fields.contains(field.id) }, set: { if $0 { fields.insert(field.id) } else { fields.remove(field.id) } }))
                        .accessibilityIdentifier("secret.snapshot.field.\(field.id)")
                }
                if !item.notes.isEmpty { Toggle("Include notes", isOn: $notes).accessibilityIdentifier("secret.snapshot.notes") }
                if item.fields.contains(where: { fields.contains($0.id) && $0.kind == "otp" }) {
                    NoticeBar(text: String(localized: "Selected OTP fields include the reusable seed, not just a current code."))
                }
            }
            if let attachments, !attachments.files.isEmpty {
                Section("Select attachments to share") {
                    ForEach(attachments.files) { entry in
                        if let file = entry.details?.file {
                            Toggle(isOn: Binding(get: { attachmentIDs.contains(entry.id) }, set: { if $0 { attachmentIDs.insert(entry.id) } else { attachmentIDs.remove(entry.id) } })) {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(file.name).lineLimit(2)
                                    Text(ByteCountFormatter.string(fromByteCount: Int64(clamping: file.size), countStyle: .file)).font(.caption).foregroundStyle(.secondary)
                                }
                            }.disabled(!entry.canShare)
                        } else { Label("Attachment unavailable on this device", systemImage: "doc.badge.ellipsis").foregroundStyle(.secondary) }
                    }
                }
            }
            if destination == "link" {
                Section("Link limits") {
                    Picker("Expires after", selection: $duration) {
                        Text("1 hour").tag(UInt64(3_600_000)); Text("1 day").tag(UInt64(86_400_000)); Text("7 days").tag(UInt64(604_800_000))
                    }
                    Stepper("\(sessions) download sessions", value: $sessions, in: 1...100)
                    SecureField("Optional password", text: $password).textInputAutocapitalization(.never)
                    Text("A download session is a retrieval allowance, not a count of file reads. Expiry or revocation cannot erase copies already saved.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
            } else {
                NoticeBar(text: String(localized: "A channel snapshot is available to members with access to this epoch, including later history grants. It has no link expiry, download-session limit or recall."))
            }
            Text("Each mobile package is limited to 8 MiB, including selected attachments. The final approval shows the exact immutable selection and destination.")
                .font(.footnote).foregroundStyle(.secondary)
            Button { work = Task { await prepare() } } label: {
                if working { ProgressView() } else { Text("Prepare selected snapshot") }
            }.disabled(working || !hasSelection || item.reserved).accessibilityIdentifier("secret.snapshot.prepare")
        }.disabled(working).privateScreen().navigationTitle("Share secret snapshot").navigationBarTitleDisplayMode(.inline)
            .task {
                if !initialized { destination = initialChannel ?? "link"; initialized = true }
                await model.loadChannels()
                let epoch = model.epoch
                do {
                    guard let snapshot = try await model.engine?.secretItemAttachments(itemId: item.id), epoch == model.epoch else { return }
                    attachments = try model.decode(snapshot)
                } catch { if epoch == model.epoch { model.report(error) } }
            }
            .onChange(of: destination) { _, destination in if destination != "link" { password = "" } }
            .onDisappear { work?.cancel() }
            .navigationDestination(isPresented: Binding(get: { job != nil }, set: { if !$0 { job = nil } })) {
                if let job { SecretShareDetailView(id: job.id) }
            }
    }

    private func prepare() async {
        guard let engine = model.engine, !working, hasSelection else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do {
            let selected = (attachments?.files ?? []).filter { attachmentIDs.contains($0.id) }.compactMap { entry -> SecretAttachmentSelection? in
                guard entry.canShare, let file = entry.details?.file else { return nil }
                return SecretAttachmentSelection(fileId: file.id, versionId: file.current)
            }
            guard selected.count == attachmentIDs.count else { throw CocoaError(.fileReadNoSuchFile) }
            let isLink = destination == "link"
            let input = SecretSnapshotInput(itemId: item.id, revision: item.revision, fieldIds: fields.sorted(), includeNotes: notes,
                attachments: selected, expiresInMs: isLink ? duration : nil, maxDownloadSessions: isLink ? UInt64(sessions) : nil)
            let result: ObjectSnapshot
            if isLink {
                result = try await coreOperation { try await engine.secretSharePrepare(input: input, password: password.isEmpty ? nil : password, operation: $0) }
            } else {
                result = try await coreOperation { try await engine.secretChannelSharePrepare(channel: destination, input: input, operation: $0) }
            }
            guard epoch == model.epoch, !Task.isCancelled else { return }
            job = try model.decode(result); password = ""
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct SecretSharesView: View {
    @Environment(SessionModel.self) private var model
    @State private var shares: [SecretShareJob]?
    @State private var loading = true
    var body: some View {
        List {
            NoticeBar(text: String(localized: "These are snapshot jobs created on this device. Desktop-local approval requests are not synchronized here."))
            if let shares {
                ForEach(shares) { share in
                    NavigationLink { SecretShareDetailView(id: share.id) } label: {
                        HStack(spacing: 12) {
                            ObjectIcon(glyph: share.plan.channel == nil ? .link : .channels)
                            VStack(alignment: .leading, spacing: 4) {
                                Text(share.plan.title).font(.headline)
                                Text(secretStateTitle(share.state)).font(.caption).foregroundStyle(.secondary)
                            }.padding(.vertical, 4)
                        }
                    }
                }
                if shares.isEmpty { ContentUnavailableView("No snapshots on this device", systemImage: "square.on.square") }
            } else if loading { ProgressView() }
            else { Button("Retry") { Task { await load() } } }
        }.privateScreen().navigationTitle("Shared snapshots").navigationBarTitleDisplayMode(.inline).task { await load() }.refreshable { await load() }
    }
    private func load() async {
        loading = true; defer { loading = false }
        let epoch = model.epoch
        do {
            guard let result = try await model.engine?.secretShares(), epoch == model.epoch else { return }
            let listing: SecretShareListing = try model.decode(result); shares = listing.shares
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct SecretShareDetailView: View {
    let id: String
    @Environment(SessionModel.self) private var model
    @State private var share: SecretShareJob?
    @State private var request: ApprovalView?
    @State private var working = false
    @State private var confirmRevoke = false
    @State private var link: String?
    @State private var loading = true
    @State private var actionTask: Task<Void, Never>?
    var body: some View {
        List {
            if let share {
                Section {
                    Text(share.plan.title).font(.title3.bold())
                    LabeledContent("Status", value: secretStateTitle(share.state))
                    LabeledContent("Package size", value: ByteCountFormatter.string(fromByteCount: Int64(clamping: share.plan.plainBytes), countStyle: .file))
                    if share.totalChunks > 0 { ProgressView(value: Double(share.uploadedChunks), total: Double(share.totalChunks)) }
                    if let channel = share.plan.channel {
                        LabeledContent("Channel", value: channel.name)
                        FingerprintRow(title: "Channel ID", value: channel.channelId)
                        NoticeBar(text: String(localized: "Channel copies have no expiry, retrieval limit or recall. Recipients can keep independent copies."))
                    } else if let expiry = share.plan.expiresAt {
                        LabeledContent("Expires") { Text(Date(timeIntervalSince1970: Double(expiry) / 1000), style: .date) }
                        if let sessions = share.plan.maxDownloadSessions { LabeledContent("Download sessions", value: sessions.formatted()) }
                        if share.plan.password == true { Label("Password protected", systemImage: "lock") }
                    }
                }
                if let error = share.error {
                    NoticeBar(text: String(localized: "This snapshot needs attention. Check its status before continuing."))
                    DisclosureGroup("Error details") { Text(error).font(.caption.monospaced()).textSelection(.enabled) }
                }
                Section {
                    if let requestID = share.requestId {
                        Button("Open approval record") { Task { await approval(id: requestID) } }.disabled(working)
                    }
                    if ["prepared", "awaiting_user", "interrupted", "failed", "cancelled"].contains(share.state) && !share.cancelRequested {
                        Button("Review publication or resume") { Task { await requestApproval() } }.disabled(working)
                    }
                    if ["unknown", "publishing", "uploading", "encrypting", "revoke_pending"].contains(share.state) {
                        NoticeBar(text: String(localized: "The outcome is uncertain. Check the original job before attempting anything again."))
                        Button("Reconcile original snapshot") { actionTask = Task { await reconcile() } }.disabled(working)
                    }
                    if share.plan.channel == nil && share.state == "published" && !share.cancelRequested {
                        Button("Show share link", systemImage: "link") { Task { await load(includeLink: true) } }.disabled(working)
                    }
                    if share.plan.channel == nil && !["revoked", "cancelled"].contains(share.state) {
                        Button("Revoke or cancel this link", role: .destructive) { confirmRevoke = true }.disabled(working)
                    }
                }
                if let link {
                    Section("Share link") {
                        Text(link).font(.caption.monospaced()).lineLimit(3).environment(\.layoutDirection, .leftToRight)
                        ShareLink(item: link) { Label("Share link", systemImage: "square.and.arrow.up") }
                        Button("Copy link") { model.clipboard.copy(link) }
                        Text("Anyone with the complete link and required password can retrieve it within its limits.").font(.footnote).foregroundStyle(.secondary)
                    }
                }
            } else if loading { ProgressView() }
            else { Button("Retry") { Task { await load() } } }
        }.privateScreen().navigationTitle("Snapshot").navigationBarTitleDisplayMode(.inline).task(id: id) { await load() }.refreshable { await load() }
            .onDisappear { link = nil; actionTask?.cancel() }
            .onChange(of: model.concealed) { _, concealed in if concealed { link = nil } }
            .confirmationDialog("Revoke this snapshot link?", isPresented: $confirmRevoke) {
                Button("Revoke or cancel", role: .destructive) { actionTask = Task { await revoke() } }
            } message: { Text("This requests an end to future retrieval. Already saved copies remain usable. An uncertain publication must still be reconciled.") }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
    }
    private func load(includeLink: Bool = false) async {
        loading = true; defer { loading = false }
        let epoch = model.epoch
        do {
            guard let result = try await model.engine?.secretShare(id: id, includeLink: includeLink), epoch == model.epoch else { return }
            let next: SecretShareJob = try model.decode(result); share = next; link = includeLink ? next.link?.url : nil
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func approval(id: String) async {
        let epoch = model.epoch
        do { if let result = try await model.engine?.approval(id: id), epoch == model.epoch { request = try model.decode(result) } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
    private func requestApproval() async {
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do {
            let result = try await engine.secretShareRequest(id: id)
            guard epoch == model.epoch else { return }; request = try model.decode(result); await load()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func reconcile() async {
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do { _ = try await coreOperation { try await engine.secretShareReconcile(id: id, operation: $0) }; if epoch == model.epoch { await load() } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
    private func revoke() async {
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do { _ = try await coreOperation { try await engine.secretShareRevoke(id: id, operation: $0) }; if epoch == model.epoch { link = nil; await load() } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct SecretSnapshotItemPicker: View {
    let channel: String
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var selected: VaultItem?
    @State private var search = ""
    @State private var items: [VaultItem] = []
    @State private var hasMore = false
    @State private var reading = false
    var body: some View {
        List {
            ForEach(items, id: \.id) { item in
                Button { Task { await select(item.id) } } label: { VaultRow(item: item) }.disabled(item.reserved)
            }
            if hasMore { Button("Load more") { Task { await load(reset: false) } }.disabled(reading) }
            if reading { ProgressView() }
        }.privateScreen().navigationTitle("Choose a secret").navigationBarTitleDisplayMode(.inline).searchable(text: $search)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task(id: search) {
                do {
                    try await Task.sleep(for: .milliseconds(200))
                    await load(reset: true)
                } catch { model.report(error) }
            }
            .navigationDestination(isPresented: Binding(get: { selected != nil }, set: { if !$0 { selected = nil } })) {
                if let selected { SecretSnapshotComposer(item: selected, initialChannel: channel) }
            }
    }
    private func load(reset: Bool) async {
        let epoch = model.epoch, query = search; reading = true; defer { reading = false }
        do {
            guard let page = try await model.engine?.vaultPage(query: query, kind: nil, favoritesOnly: false, offset: reset ? 0 : UInt32(items.count), limit: 100),
                  epoch == model.epoch, query == search, !Task.isCancelled else { return }
            items = reset ? page.items : items + page.items; hasMore = page.hasMore
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func select(_ id: String) async {
        let epoch = model.epoch
        do { if let result = try await model.engine?.vaultItem(id: id, revision: nil), epoch == model.epoch { selected = result.item } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
}
