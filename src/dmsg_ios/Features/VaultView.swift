import SwiftUI
@preconcurrency import DMsgCore

struct VaultView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var selected: String?
    @State private var editing: VaultEditorTarget?
    @State private var trash = false
    private var shown: [VaultItem] { model.vault }
    private var favorites: [VaultItem] { model.favorites }

    var body: some View {
        @Bindable var model = model
        WorkspaceNavigation(selection: $selected) { listSelection in
            List(selection: listSelection) {
                if model.session?.syncError != nil {
                    VStack(alignment: .leading, spacing: 4) {
                        NoticeBar(text: String(localized: "Showing saved content. The latest sync did not finish."))
                        if let timestamp = model.session?.lastSyncAtMs {
                            Text(Date(timeIntervalSince1970: Double(timestamp) / 1000), style: .relative).font(.caption).foregroundStyle(.secondary)
                        }
                    }.listRowBackground(Color.clear).listRowSeparator(.hidden)
                }
                if (model.session?.pendingSync ?? 0) > 0 {
                    NoticeBar(text: String(localized: "Saved on this device. Changes are waiting to sync."))
                        .listRowBackground(Color.clear).listRowSeparator(.hidden)
                }
                if (model.session?.conflicts ?? 0) > 0 {
                    NavigationLink { ConflictsView() } label: { Label("Review sync conflicts", systemImage: "arrow.triangle.branch") }
                }
                if !favorites.isEmpty && model.search.isEmpty {
                    Section("Favorites") {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: dynamicTypeSize.isAccessibilitySize ? 280 : 130), spacing: 10)], spacing: 10) {
                            ForEach(favorites, id: \.id) { item in
                                Button {
                                    model.activity()
                                    selected = item.id
                                } label: {
                                    HStack(spacing: 8) {
                                        AppGlyph.vaultItem(item.kind).image.resizable().scaledToFit().frame(width: 20, height: 20).foregroundStyle(Theme.forest)
                                        VStack(alignment: .leading, spacing: 3) {
                                            Text(item.title).font(.subheadline.weight(.semibold)).lineLimit(dynamicTypeSize.isAccessibilitySize ? 2 : 1)
                                            Text(item.subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                        }.frame(maxWidth: .infinity, alignment: .leading)
                                    }.padding(10).frame(minHeight: 44)
                                        .background(Theme.forest.opacity(0.07), in: RoundedRectangle(cornerRadius: 14))
                                }.buttonStyle(.plain).accessibilityIdentifier("vault.favorite.\(item.id)")
                            }
                        }.listRowInsets(EdgeInsets(top: 4, leading: 24, bottom: 8, trailing: 24)).listRowBackground(Color.clear)
                            .selectionDisabled()
                    }
                }
                Section {
                    Picker("Type", selection: $model.vaultFilter) {
                        Text("All").tag("all"); Text("Passwords").tag("login"); Text("Notes").tag("note"); Text("Keys").tag("key")
                    }.pickerStyle(.segmented).listRowBackground(Color.clear).listRowSeparator(.hidden)
                    ForEach(shown, id: \.id) { item in
                        NavigationLink(value: item.id) { VaultRow(item: item) }
                            .compactObjectRow().accessibilityIdentifier("vault.item.\(item.id)")
                    }
                    if model.vaultHasMore { Button("Load more") { Task { await model.loadVault(more: true) } }.disabled(model.loadingVault) }
                }
                if shown.isEmpty {
                    ContentUnavailableView.search(text: model.search)
                        .listRowBackground(Color.clear)
                }
            }.listStyle(.plain).privateScreen().navigationTitle("Vault")
                .searchable(text: $model.search, prompt: "Search your vault")
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { ApprovalButton() }
                    ToolbarItemGroup(placement: .primaryAction) {
                        Button("Add item", systemImage: "plus") { editing = .new }
                        Menu("Vault tools", systemImage: "ellipsis.circle") {
                            NavigationLink("Import passwords") { VaultImportView() }
                            NavigationLink("Import secret package") { SecretPackageImportView() }
                            NavigationLink("Shared snapshots") { SecretSharesView() }
                            NavigationLink("Local security review") { SecretHealthView() }
                            NavigationLink("Activity on this device") { SecretActivityView() }
                            Button("Trash", systemImage: "trash") { trash = true }
                        }
                    }
                }
                .refreshable { await model.synchronize() }
                .task(id: model.search + ":" + model.vaultFilter) {
                    try? await Task.sleep(for: .milliseconds(200))
                    if !Task.isCancelled { await model.loadVault() }
                }
        } detail: { id in
            if let id { VaultDetailView(id: id) }
            else { ContentUnavailableView("Select a secret", image: AppGlyph.vault.assetName, description: Text("Your hidden fields stay concealed until you choose to reveal them.")) }
        }
        .onChange(of: selected) { _, value in if value != nil { model.activity() } }
        .sheet(item: $editing) { target in NavigationStack { VaultEditor(target: target) } }
        .sheet(isPresented: $trash) { NavigationStack { VaultTrashView() } }
    }
}

struct VaultRow: View {
    let item: VaultItem
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        HStack(spacing: 10) {
            ObjectIcon(glyph: .vaultItem(item.kind))
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title).font(.body.weight(.medium)).lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 1)
                Text(item.subtitle.isEmpty ? item.kind : item.subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }.frame(maxWidth: .infinity, alignment: .leading)
            if !item.synced { Image(systemName: "arrow.triangle.2.circlepath").font(.caption).foregroundStyle(.secondary).accessibilityLabel("Waiting to sync") }
        }
    }
}

struct VaultEditorTarget: Identifiable {
    let item: VaultItem?
    var id: String { item?.id ?? "new" }
    static let new = VaultEditorTarget(item: nil)
}

struct VaultDetailView: View {
    private struct ReferenceTarget: Hashable { let itemID: String; let fieldID: String }
    let id: String
    var revision: String? = nil
    @Environment(SessionModel.self) private var model
    @State private var item: VaultItem?
    @State private var loadFailed = false
    @State private var reload = 0
    @State private var revealed: [String: String] = [:]
    @State private var pendingField: VaultField?
    @State private var copyPending = false
    @State private var editing: VaultEditorTarget?
    @State private var referenceTarget: ReferenceTarget?
    @State private var revealTask: Task<Void, Never>?
    @State private var revealRequest: UUID?

    var body: some View {
        List {
            if let item {
                HStack(spacing: 12) {
                    ObjectIcon(glyph: .vaultItem(item.kind))
                    VStack(alignment: .leading, spacing: 4) {
                        Text(item.title).font(.title2.bold()).accessibilityIdentifier("vault.detail.title")
                        Text(item.synced ? "Synced" : "Saved on this device").font(.caption).foregroundStyle(.secondary)
                    }
                }.padding(.vertical, 6).listRowBackground(Color.clear).listRowSeparator(.hidden)
                Section {
                    ForEach(item.fields, id: \.id) { (field: VaultField) in
                        VStack(alignment: .leading, spacing: 3) {
                            Text(field.label.isEmpty ? (field.kind == "otp" ? String(localized: "One-time code") : vaultFieldName(field.id)) : field.label).font(.caption).foregroundStyle(.secondary)
                            HStack(alignment: .center, spacing: 8) {
                                Text(revealed[field.id] ?? field.value).font(field.concealed ? .body.monospaced() : .body)
                                    .textSelection(.disabled)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                if field.concealed {
                                    Button(revealed[field.id] == nil ? "Reveal" : "Hide", systemImage: revealed[field.id] == nil ? "eye" : "eye.slash") {
                                        if revealed[field.id] != nil { revealed.removeValue(forKey: field.id) }
                                        else { pendingField = field; copyPending = false }
                                    }.labelStyle(.iconOnly).font(.system(size: 20)).frame(minWidth: 44, minHeight: 44).disabled(item.reserved)
                                        .accessibilityIdentifier("vault.reveal.\(field.id)")
                                }
                                Button("Copy", systemImage: "doc.on.doc") { pendingField = field; copyPending = true }
                                    .labelStyle(.iconOnly).font(.system(size: 20)).frame(minWidth: 44, minHeight: 44).disabled(item.reserved)
                                if revision == nil && !item.reserved {
                                    Button { referenceTarget = ReferenceTarget(itemID: item.id, fieldID: field.id) } label: {
                                        Image(systemName: "link").font(.system(size: 20)).frame(width: 44, height: 44)
                                    }.accessibilityLabel("Field reference and dependencies")
                                }
                            }.buttonStyle(.borderless)
                        }.padding(.vertical, 4)
                    }
                }
                if !item.notes.isEmpty { Section("Notes") { Text(item.notes).textSelection(.enabled) } }
                if !item.reserved && revision == nil {
                    Section {
                        NavigationLink("Version history") { VaultHistoryView(item: item) }
                        Button("Edit item", systemImage: "pencil") { editing = VaultEditorTarget(item: item) }.frame(minHeight: 44)
                        NavigationLink { SecretItemAttachmentsView(itemID: item.id) } label: { Label("Item attachments", systemImage: "paperclip") }
                        NavigationLink { SecretSnapshotComposer(item: item) } label: { Label("Share secret snapshot", systemImage: "square.and.arrow.up") }
                            .accessibilityIdentifier("vault.share")
                    }
                } else if item.reserved { NoticeBar(text: String(localized: "This key is managed by dMsg and cannot be copied or edited.")) }
                if revision != nil { NoticeBar(text: String(localized: "You are viewing an immutable historical version.")) }
            } else if loadFailed {
                NoticeBar(text: String(localized: "The operation could not be completed. Please try again."))
                Button("Retry") { reload &+= 1 }.accessibilityIdentifier("vault.detail.retry")
            } else { ProgressView().accessibilityIdentifier("vault.detail.loading") }
        }.privateScreen().navigationTitle(revision == nil ? "Secret" : "Historical version").navigationBarTitleDisplayMode(.inline)
            .task(id: "\(id):\(revision ?? "head"):\(model.epoch):\(model.concealed):\(reload)") {
                // Background suspension invalidates in-flight reads without
                // destroying the navigation column. Resume with a fresh handle.
                guard !model.concealed else { return }
                await load()
            }
            .onDisappear { clearRevealed() }
            .onChange(of: id) { _, _ in editing = nil; referenceTarget = nil }
            .onChange(of: model.concealed) { _, concealed in if concealed { clearRevealed() } }
            .sheet(item: $editing, onDismiss: { reload &+= 1 }) { target in NavigationStack { VaultEditor(target: target) } }
            .navigationDestination(item: $referenceTarget) { target in
                SecretFieldReferenceView(itemID: target.itemID, fieldID: target.fieldID)
            }
            .confirmationDialog(copyPending ? "Copy this field?" : "Reveal this field?", isPresented: Binding(get: { pendingField != nil }, set: { if !$0 { pendingField = nil } })) {
                Button(copyPending ? "Copy for 30 seconds" : "Reveal") { if let field = pendingField { revealTask?.cancel(); revealTask = Task { await reveal(field) } } }
            } message: { Text("Only do this when you are ready to use it. Copies stay on this device and expire after 30 seconds.") }
    }

    private func load() async {
        clearRevealed(); item = nil; loadFailed = false
        let epoch = model.epoch
        do {
            let loaded: VaultItem
            #if DEBUG
            if model.uiTesting {
                if ProcessInfo.processInfo.arguments.contains("--ui-slow-vault") {
                    try await Task.sleep(for: .seconds(2))
                }
                if ProcessInfo.processInfo.arguments.contains("--ui-vault-load-failure"), reload == 0 {
                    throw MobileError.Failure(code: "CANCELLED")
                }
                guard let fixture = model.vault.first(where: { $0.id == id }) else {
                    throw MobileError.Failure(code: "NOT_FOUND")
                }
                loaded = fixture
            } else {
                loaded = try await readItem()
            }
            #else
            loaded = try await readItem()
            #endif
            guard model.epoch == epoch, !model.concealed, !Task.isCancelled else { return }
            item = loaded
        } catch {
            guard model.epoch == epoch, !model.concealed, !Task.isCancelled else { return }
            loadFailed = true
            model.report(error)
        }
    }

    private func readItem() async throws -> VaultItem {
        guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        let result = try await engine.vaultItem(id: id, revision: revision)
        guard result.generation == model.session?.generation else { throw CancellationError() }
        return result.item
    }

    private func clearRevealed() {
        revealTask?.cancel(); revealTask = nil; revealRequest = nil
        revealed = [:]; pendingField = nil
    }

    private func reveal(_ field: VaultField) async {
        guard let item else { return }
        let copying = copyPending, epoch = model.epoch, request = UUID()
        revealRequest = request
        pendingField = nil; model.activity()
        do {
            guard let value = try await model.engine?.revealField(id: item.id, revision: item.revision, field: field.id), epoch == model.epoch, revealRequest == request, !model.concealed, !Task.isCancelled else { return }
            if copying { model.clipboard.copy(value.value); model.notice = String(localized: "Copied for 30 seconds") }
            else { revealed[field.id] = value.value }
            if let remaining = value.remainingMs {
                try? await Task.sleep(for: .milliseconds(Int64(remaining)))
                if epoch == model.epoch, revealRequest == request, !Task.isCancelled { revealed.removeValue(forKey: field.id) }
            }
        } catch { model.report(error) }
    }
}

struct VaultEditor: View {
    let target: VaultEditorTarget
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var kind = "login"
    @State private var notes = ""
    @State private var favorite = false
    @State private var tags = ""
    @State private var newFieldKind = "text"
    @State private var fields: [EditableField] = []
    @State private var saving = false
    @State private var deleting = false
    @State private var attachmentIDs: [String] = []
    @State private var attachmentFiles: [String: LibraryFile] = [:]
    @State private var attachmentsChanged = false
    @State private var choosingAttachment = false
    @State private var initialized = false
    struct EditableField: Identifiable { let id: String; let kind: String; var label: String; var section: String = ""; var value: String; let concealed: Bool; var changed = false }

    var body: some View {
        Form {
            Section {
                TextField("Title", text: $title).accessibilityIdentifier("vault.edit.title")
                if target.item == nil {
                    Picker("Type", selection: $kind) {
                        ForEach(vaultKinds(), id: \.self) { kind in Text(vaultKindName(kind)).tag(kind) }
                    }
                }
                Toggle("Favorite", isOn: $favorite)
                TextField("Tags, separated by commas", text: $tags).textInputAutocapitalization(.never)
            }
            Section("Fields") {
                ForEach($fields) { $field in
                    VStack(alignment: .leading) {
                        Text(field.label.isEmpty ? (field.kind == "otp" ? String(localized: "One-time code") : vaultFieldName(field.id)) : field.label).font(.caption).foregroundStyle(.secondary)
                        if field.concealed { SecureField(target.item == nil ? "Value" : "Leave empty to keep existing value", text: $field.value) }
                        else { TextField("Value", text: $field.value).textInputAutocapitalization(.never) }
                        DisclosureGroup("Field settings") {
                            TextField("Field name", text: $field.label)
                            TextField("Section", text: $field.section)
                            Button("Remove field", role: .destructive) { fields.removeAll { $0.id == field.id } }
                        }
                    }.onChange(of: field.value) { _, _ in field.changed = true; model.activity() }
                }
                Picker("New field type", selection: $newFieldKind) {
                    Text("Text").tag("text"); Text("Hidden value").tag("concealed")
                    Text("Website").tag("url"); Text("Email").tag("email")
                    Text("Phone").tag("phone"); Text("Date").tag("date")
                    Text("Month").tag("month"); Text("One-time code").tag("otp")
                }
                Button("Add field") {
                    fields.append(.init(id: "field_" + UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(16).lowercased(),
                        kind: newFieldKind, label: "", value: "", concealed: ["concealed", "otp"].contains(newFieldKind), changed: true))
                }.disabled(fields.count >= 64)
            }
            Section("Notes") { TextEditor(text: $notes).frame(minHeight: 100) }
            Section("Item attachments") {
                ForEach(attachmentIDs, id: \.self) { id in
                    HStack(alignment: .top, spacing: 10) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(attachmentFiles[id]?.name ?? id).font(.subheadline).lineLimit(2)
                            if attachmentFiles[id] == nil { Text("File not available on this device").font(.caption).foregroundStyle(.secondary) }
                        }.frame(maxWidth: .infinity, alignment: .leading)
                        Button("Remove attachment reference", systemImage: "minus.circle", role: .destructive) {
                            attachmentIDs.removeAll { $0 == id }; attachmentsChanged = true
                        }.labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44)
                    }
                }
                Button("Attach a file", systemImage: "paperclip") { choosingAttachment = true }.disabled(saving)
                Text("Removing a reference does not delete the file. Existing references are kept even when their files are missing on this device.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            NoticeBar(text: String(localized: "Saved locally first. Sync status is shown separately."))
            if target.item != nil { Button("Move to Trash", role: .destructive) { deleting = true } }
        }.privateScreen().navigationTitle(target.item == nil ? "New secret" : "Edit secret")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Save") { Task { await save() } }.disabled(title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || saving) }
            }
            .onAppear { setup() }
            .task(id: target.id) { await loadAttachments() }
            .sheet(isPresented: $choosingAttachment) {
                NavigationStack {
                    AttachmentPicker(folder: "root") { file in
                        if !attachmentIDs.contains(file.id) { attachmentIDs.append(file.id); attachmentsChanged = true }
                        attachmentFiles[file.id] = file; choosingAttachment = false
                    }
                }
            }
            .onChange(of: kind) { _, _ in if target.item == nil { template() } }
            .onChange(of: title) { _, _ in model.activity() }
            .onChange(of: notes) { _, _ in model.activity() }
            .confirmationDialog("Move this secret to Trash?", isPresented: $deleting) {
                Button("Move to Trash", role: .destructive) { Task {
                    guard let id = target.item?.id, let engine = model.engine else { return }
                    do { try await engine.deleteItem(id: id); await model.loadVault(); dismiss() }
                    catch { model.report(error) }
                } }
            }
    }

    private func setup() {
        guard !initialized else { return }; initialized = true
        if let item = target.item {
            title = item.title; kind = item.kind; notes = item.notes; favorite = item.favorite; tags = item.tags.joined(separator: ", ")
            attachmentIDs = item.files
            fields = item.fields.map { EditableField(id: $0.id, kind: $0.kind, label: $0.label, section: $0.section, value: $0.concealed ? "" : $0.value, concealed: $0.concealed) }
        } else {
            template()
        }
    }

    private func loadAttachments() async {
        guard let id = target.item?.id else { return }; let epoch = model.epoch
        do {
            guard let result = try await model.engine?.secretItemAttachments(itemId: id), epoch == model.epoch else { return }
            let listing: SecretAttachmentListing = try model.decode(result)
            for entry in listing.files { if let file = entry.details?.file { attachmentFiles[entry.id] = file } }
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func template() {
        fields = vaultTemplate(kind: kind).map { field in
            let old = fields.first { $0.id == field.id && $0.kind == field.kind }
            return EditableField(id: field.id, kind: field.kind, label: "",
                value: old?.value ?? "", concealed: field.concealed)
        }
    }

    private func save() async {
        saving = true; defer { saving = false }; let epoch = model.epoch
        do {
            let saved = try await model.engine?.saveItem(id: target.item?.id, revision: target.item?.revision,
                edit: VaultEdit(title: title, kind: kind, notes: notes, favorite: favorite, tags: tags.split(separator: ",").map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }, fields: fields.map {
                    FieldEdit(id: $0.id, kind: $0.kind, label: $0.label, section: $0.section, value: target.item != nil && !$0.changed ? nil : $0.value)
                }, files: target.item == nil || attachmentsChanged ? attachmentIDs : nil))
            guard epoch == model.epoch else { return }
            if saved?.item.conflict == true { model.notice = String(localized: "Saved as a separate version. Review sync conflicts.") }
            await model.updateSession(); dismiss()
        } catch { model.report(error) }
    }
}

func vaultKindName(_ kind: String) -> LocalizedStringKey {
    switch kind {
    case "login": "Password"; case "api": "API credential"; case "key": "Key"; case "ssh": "SSH key"
    case "wallet": "Wallet secret"; case "card": "Payment card"; case "identity": "Identity"
    case "server": "Server"; case "database": "Database"; case "license": "Software license"
    default: "Note"
    }
}

func vaultFieldName(_ id: String) -> String {
    switch id {
    case "username": String(localized: "Username")
    case "password": String(localized: "Password")
    case "url": String(localized: "Website")
    default: String(localized: String.LocalizationValue(id.replacingOccurrences(of: "_", with: " ").capitalized))
    }
}

struct VaultTrashView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var items: [VaultItem] = []
    @State private var more = false
    var body: some View {
        List {
            ForEach(items, id: \.id) { item in
                HStack { VaultRow(item: item); Button("Restore") { Task {
                    do { try await model.engine?.restoreItem(id: item.id); await load(reset: true); await model.loadVault() }
                    catch { model.report(error) }
                } }.buttonStyle(.bordered) }
            }
            if more { Button("Load more") { Task { await load(reset: false) } } }
            if items.isEmpty { ContentUnavailableView("Trash is empty", systemImage: "trash") }
        }.privateScreen().navigationTitle("Trash").task { await load(reset: true) }
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
    }
    private func load(reset: Bool) async {
        do {
            guard let page = try await model.engine?.vaultTrash(offset: reset ? 0 : UInt32(items.count), limit: 60), !Task.isCancelled else { return }
            items = reset ? page.items : items + page.items; more = page.hasMore
        } catch { model.report(error) }
    }
}
