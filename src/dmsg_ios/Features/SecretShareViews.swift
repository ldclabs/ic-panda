import SwiftUI
@preconcurrency import DMsgCore

struct SecretSnapshotComposer: View {
    let item: VaultItem
    var initialChannel: String? = nil
    @Environment(SessionModel.self) private var model
    @State private var fields: Set<String> = []
    @State private var notes = false
    @State private var attachmentIDs: Set<String> = []
    @State private var attachmentsTouched = false
    @State private var attachments: SecretAttachmentListing?
    @State private var destination = "link"
    @State private var initialized = false
    @State private var duration: UInt64 = 86_400_000
    @State private var sessions = 1
    @State private var password = ""
    @State private var working = false
    @State private var job: SecretShareJob?
    @State private var work: Task<Void, Never>?
    @State private var offerText = ""
    @State private var shareIdentity = false
    @State private var scanning = false
    @State private var cameraDenied = false
    @State private var previousSend: UInt64?
    private var hasSelection: Bool { !fields.isEmpty || notes || !attachmentIDs.isEmpty }
    private var nearby: NearbyShareModel { model.nearbyShare }
    private var nearbySend: NearbyShareModel.SendStatus? {
        guard let send = nearby.status?.send, send.id != previousSend else { return nil }
        return send
    }
    private var showingNearbyProgress: Bool {
        guard destination == "nearby", let send = nearbySend else { return false }
        return send.active || send.state == "done" || send.state == "failed"
    }

    var body: some View {
        Form {
            if destination == "nearby", let err = nearby.error {
                NoticeBar(text: err)
            }
            if showingNearbyProgress, let send = nearbySend {
                NearbySendProgressSection(send: send)
                if send.state == "failed" {
                    SettingsAction("Start again", prominent: true) { previousSend = send.id }
                }
            } else {
                Section {
                    Text(item.title).font(.title3.bold())
                    Text("Choose exactly what this independent snapshot contains. Source edits will not change copies already sent.")
                        .font(.footnote).foregroundStyle(.secondary)
                    if model.localAccount {
                        LabeledContent("Destination", value: String(localized: "Share nearby"))
                    } else {
                        Picker("Destination", selection: $destination) {
                            Text("Share link").tag("link")
                            Text("Share nearby").tag("nearby")
                            ForEach(model.channels.filter(\.canPublish), id: \.id) { channel in Text(channel.name).tag(channel.id) }
                        }.accessibilityIdentifier("secret.snapshot.destination")
                    }
                    if destination == "link" {
                        LabeledContent("Recipient", value: String(localized: password.isEmpty ? "Anyone with the link" : "Anyone with the link and password"))
                        LabeledContent("Expires after", value: String(localized: duration == 3_600_000 ? "1 hour" : duration == 86_400_000 ? "1 day" : "7 days"))
                        LabeledContent("Download sessions", value: sessions.formatted())
                        Text("Revoking stops future downloads. Saved copies remain.").font(.footnote).foregroundStyle(.secondary)
                    } else if destination != "nearby" {
                        Text("A channel snapshot is available to members with access to this epoch, including later history grants. It has no link expiry, download-session limit or recall.")
                            .font(.footnote).foregroundStyle(.secondary)
                    }
                }
                Section("Select fields to share") {
                    ForEach(item.fields.filter { !$0.value.isEmpty }, id: \.id) { field in
                        Toggle(vaultFieldTitle(field),
                               isOn: Binding(get: { fields.contains(field.id) }, set: { if $0 { fields.insert(field.id) } else { fields.remove(field.id) } }))
                            .accessibilityIdentifier("secret.snapshot.field.\(field.id)")
                    }
                    if !item.fields.contains(where: { !$0.value.isEmpty }) && item.notes.isEmpty {
                        Text("No fields or notes").foregroundStyle(.secondary)
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
                                Toggle(isOn: Binding(get: { attachmentIDs.contains(entry.id) }, set: { attachmentsTouched = true; if $0 { attachmentIDs.insert(entry.id) } else { attachmentIDs.remove(entry.id) } })) {
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(file.name).lineLimit(2)
                                        Text(ByteCountFormatter.string(fromByteCount: Int64(clamping: file.size), countStyle: .file)).font(.footnote).foregroundStyle(.secondary)
                                    }
                                }.disabled(!entry.canShare)
                            } else { Label("Attachment unavailable on this device", systemImage: "doc.badge.ellipsis").foregroundStyle(.secondary) }
                        }
                    }
                }
                if destination == "nearby" {
                    if hasSelection {
                        NearbyTargetPickerSection(
                            offerText: $offerText,
                            shareIdentity: $shareIdentity,
                            scanning: $scanning,
                            cameraDenied: $cameraDenied,
                            onSendOffer: { text in
                                guard let input = makeNearbyInput() else { return }
                                Task { await nearby.sendVault(model, offer: text, peer: nil, input: input, shareIdentity: shareIdentity) }
                            },
                            onSendPeer: { peer in
                                guard let input = makeNearbyInput() else { return }
                                Task { await nearby.sendVault(model, offer: nil, peer: peer, input: input, shareIdentity: shareIdentity) }
                            }
                        )
                    } else {
                        Section {
                            Text("Choose at least one field, note or attachment before selecting a nearby receiver.")
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                    }
                } else if destination == "link" {
                    DisclosureGroup("Link limits") {
                        Picker("Expires after", selection: $duration) {
                            Text("1 hour").tag(UInt64(3_600_000)); Text("1 day").tag(UInt64(86_400_000)); Text("7 days").tag(UInt64(604_800_000))
                        }
                        Stepper("\(sessions) download sessions", value: $sessions, in: 1...100)
                        SecureField("Optional password", text: $password).textInputAutocapitalization(.never)
                        Text("A download session is a retrieval allowance, not a count of file reads. Expiry or revocation cannot erase copies already saved.")
                            .font(.footnote).foregroundStyle(.secondary)
                    }
                }
            }
        }.toggleStyle(SelectionRowToggleStyle()).disabled(working).privateScreen().navigationTitle("Share secret snapshot").navigationBarTitleDisplayMode(.inline)
            .interactiveDismissDisabled(destination == "nearby" && (nearbySend?.active == true || nearby.working))
            .safeAreaInset(edge: .bottom) {
                if destination != "nearby" {
                    Button { work = Task { await prepare() } } label: {
                        HStack { if working { ProgressView() }; Text(destination == "link" ? "Create link" : "Send") }.frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(working || !hasSelection || item.reserved || item.deleted || item.conflict)
                    .accessibilityIdentifier("secret.snapshot.prepare")
                    .padding(.horizontal).padding(.vertical, 8).background(.bar)
                }
            }
            .task {
                if previousSend == nil { previousSend = nearbySend.flatMap { $0.active ? nil : $0.id } ?? 0 }
                if !initialized {
                    destination = initialChannel ?? (model.localAccount ? "nearby" : "link")
                    fields = Set(item.fields.filter { !$0.value.isEmpty }.map(\.id))
                    notes = !item.notes.isEmpty
                    initialized = true
                }
                if destination == "nearby" { nearby.startBrowse() }
                if !model.localAccount { await model.loadChannels() }
                let epoch = model.epoch
                do {
                    guard let snapshot = try await model.engine?.secretItemAttachments(itemId: item.id), epoch == model.epoch else { return }
                    let listing: SecretAttachmentListing = try model.decode(snapshot)
                    attachments = listing
                    if !attachmentsTouched {
                        attachmentIDs = Set(listing.files.filter(\.canShare).map(\.id))
                    }
                } catch { if epoch == model.epoch { model.report(error) } }
            }
            .onChange(of: destination) { _, next in
                if next != "link" { password = "" }
                if next == "nearby" { nearby.startBrowse() } else { nearby.stopBrowse() }
            }
            .onDisappear {
                work?.cancel()
                if nearbySend?.active == true { nearby.cancelSend(model) }
                nearby.stopBrowse()
            }
            .sheet(isPresented: $scanning) {
                QRCodeScanner { text in
                    offerText = text
                    scanning = false
                    if hasSelection, let input = makeNearbyInput() {
                        Task { await nearby.sendVault(model, offer: text, peer: nil, input: input, shareIdentity: shareIdentity) }
                    }
                }
            }
            .navigationDestination(isPresented: Binding(get: { job != nil }, set: { if !$0 { job = nil } })) {
                if let job { SecretShareDetailView(id: job.id) }
            }
    }

    private func makeNearbyInput() -> SecretSnapshotInput? {
        guard hasSelection, !item.reserved, !item.deleted, !item.conflict else { return nil }
        let selected = (attachments?.files ?? []).filter { attachmentIDs.contains($0.id) }.compactMap { entry -> SecretAttachmentSelection? in
            guard entry.canShare, let file = entry.details?.file else { return nil }
            return SecretAttachmentSelection(fileId: file.id, versionId: file.current)
        }
        guard selected.count == attachmentIDs.count else { return nil }
        return SecretSnapshotInput(
            itemId: item.id,
            revision: item.revision,
            fieldIds: item.fields.map(\.id).filter(fields.contains),
            includeNotes: notes,
            attachments: selected,
            expiresInMs: nil,
            maxDownloadSessions: nil
        )
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
            let input = SecretSnapshotInput(itemId: item.id, revision: item.revision, fieldIds: item.fields.map(\.id).filter(fields.contains), includeNotes: notes,
                attachments: selected, expiresInMs: isLink ? duration : nil, maxDownloadSessions: isLink ? UInt64(sessions) : nil)
            let result: ObjectSnapshot
            if isLink {
                result = try await coreOperation { try await engine.secretSharePrepare(input: input, password: password.isEmpty ? nil : password, operation: $0) }
            } else {
                result = try await coreOperation { try await engine.secretChannelSharePrepare(channel: destination, input: input, operation: $0) }
            }
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let prepared: SecretShareJob = try model.decode(result)
            password = ""
            do {
                let published = try await coreOperation { try await engine.secretSharePublish(id: prepared.id, operation: $0) }
                guard epoch == model.epoch, !Task.isCancelled else { return }
                job = (try? model.decode(published)) ?? prepared
            } catch {
                if epoch == model.epoch {
                    model.report(error)
                    job = prepared
                }
            }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct SecretSharesView: View {
    @Environment(SessionModel.self) private var model
    @State private var shares: [SecretShareJob]?
    @State private var loading = true
    var body: some View {
        List {
            NavigationLink { FileLinksView(scope: .snapshots(item: nil)) } label: { Label("Share links", systemImage: "link") }
            if let shares {
                ForEach(shares) { share in
                    NavigationLink { SecretShareDetailView(id: share.id) } label: {
                        HStack(spacing: 12) {
                            ObjectIcon(glyph: share.plan.channel == nil ? .link : .channels)
                            VStack(alignment: .leading, spacing: 4) {
                                Text(share.plan.title).font(.headline)
                                Text(secretStateTitle(share.state)).font(.footnote).foregroundStyle(.secondary)
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
                    if share.totalChunks > 0 { ProgressView(value: Double(share.uploadedChunks), total: Double(share.totalChunks)).motion(Motion.move, value: share.uploadedChunks) }
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
                    DisclosureGroup("Error details") { Text(error).font(.footnote.monospaced()).textSelection(.enabled) }
                }
                Section {
                    if ["prepared", "needs_approval", "awaiting_user", "interrupted", "failed"].contains(share.state) && !share.cancelRequested {
                        SettingsAction(share.state == "failed" ? "Retry" : "Publish", prominent: true, working: working) { actionTask = Task { await publish() } }.disabled(working)
                    }
                    if ["unknown", "publishing", "uploading", "encrypting", "revoke_pending"].contains(share.state) {
                        NoticeBar(text: String(localized: "The outcome is uncertain. Check the original job before attempting anything again."))
                        SettingsAction("Reconcile original snapshot", prominent: true, working: working) { actionTask = Task { await reconcile() } }.disabled(working)
                    }
                    if share.plan.channel == nil && share.plan.nearby == nil && share.state == "published" && !share.cancelRequested {
                        SettingsAction("Show share link", systemImage: "link", working: working) { actionTask?.cancel(); actionTask = Task { await load(includeLink: true) } }.disabled(working)
                    }
                    if share.plan.channel == nil && share.plan.nearby == nil && !["revoked", "cancelled"].contains(share.state) {
                        SettingsAction("Revoke or cancel this link", role: .destructive) { confirmRevoke = true }.disabled(working)
                    }
                }
                if let link {
                    Section("Share link") {
                        Text(link).font(.footnote.monospaced()).lineLimit(3).environment(\.layoutDirection, .leftToRight)
                        ShareLink(item: link) { Label("Share link", systemImage: "square.and.arrow.up") }
                        CopyButton("Copy link", value: link)
                        Text("Anyone with the complete link and required password can retrieve it within its limits.").font(.footnote).foregroundStyle(.secondary)
                    }
                }
            } else if loading { ProgressView() }
            else { Button("Retry") { Task { await load() } } }
        }.privateScreen().navigationTitle("Snapshot").navigationBarTitleDisplayMode(.inline).task(id: id) {
            await load()
        }.refreshable { await load() }
            .onDisappear { link = nil; actionTask?.cancel() }
            .onChange(of: model.concealed) { _, concealed in if concealed { actionTask?.cancel(); link = nil } }
            .confirmationDialog("Revoke this snapshot link?", isPresented: $confirmRevoke) {
                Button("Revoke or cancel", role: .destructive) { actionTask = Task { await revoke() } }
            } message: { Text("This requests an end to future retrieval. Already saved copies remain usable. An uncertain publication must still be reconciled.") }
    }
    private func load(includeLink: Bool = false) async {
        loading = true; defer { loading = false }
        let epoch = model.epoch
        do {
            guard let result = try await model.engine?.secretShare(id: id, includeLink: includeLink), epoch == model.epoch, !Task.isCancelled, !model.concealed else { return }
            let next: SecretShareJob = try model.decode(result)
            share = next; link = includeLink ? next.link?.url : nil
            if includeLink && link == nil { model.notice = String(localized: "The full link is not available on this device.") }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func publish() async {
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do {
            _ = try await coreOperation { try await engine.secretSharePublish(id: id, operation: $0) }
            if epoch == model.epoch { await load() }
        } catch {
            if epoch == model.epoch {
                model.report(error)
                await load()
            }
        }
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
    @State private var search = VaultPickerSearch()
    @State private var opening: String?
    var body: some View {
        VaultItemPicker(search: search, kind: nil, limit: 100) {
            if opening != nil { ProgressView("Loading…") }
        } row: { item in
            Button { opening = item.id } label: { VaultRow(item: item) }
                .disabled(item.reserved || item.deleted || item.conflict || opening != nil).accessibilityIdentifier("secret.picker.\(item.id)")
        }.privateScreen().navigationTitle("Choose a secret").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .onChange(of: search.query) { _, _ in opening = nil }
            .onDisappear { opening = nil }
            .task(id: opening) { if let id = opening { await select(id) } }
            .navigationDestination(isPresented: Binding(get: { selected != nil }, set: { if !$0 { selected = nil } })) {
                if let selected { SecretSnapshotComposer(item: selected, initialChannel: channel) }
            }
    }
    private func select(_ id: String) async {
        let epoch = model.epoch, query = search.query
        defer { if opening == id && !Task.isCancelled { opening = nil } }
        do {
            let item = try await model.readVaultItem(id: id)
            guard epoch == model.epoch, !model.concealed, !Task.isCancelled, opening == id, search.query == query else { return }
            selected = item
        } catch { if epoch == model.epoch, !model.concealed, opening == id, search.query == query { model.report(error) } }
    }
}
