import SwiftUI
@preconcurrency import DMsgCore

enum VaultEditorFocus: Hashable {
    case title, tags, notes
    case name(String), value(String), section(String)
}

struct VaultDraft: Codable, Equatable {
    struct Field: Codable, Equatable, Identifiable {
        var id: String
        var kind: String
        var label: String
        var section: String
        var value: String
        var concealed: Bool
        var changed: Bool

        var title: String {
            if !label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return label }
            return id.hasPrefix("field_") || kind == "otp" ? vaultFieldKindTitle(kind) : vaultFieldName(id)
        }
    }
    var title = ""
    var kind = "login"
    var notes = ""
    var favorite = false
    var tags = ""
    var tagEntry = ""
    var fields: [Field] = []
    var files: [String] = []
    var baseRevision: String?

    init(target: VaultEditorTarget) {
        if let item = target.item {
            title = item.title; kind = item.kind; notes = item.notes; favorite = item.favorite
            tags = item.tags.joined(separator: ", "); files = item.files
            baseRevision = item.revision
            fields = item.fields.map { Field(id: $0.id, kind: $0.kind, label: $0.label, section: $0.section,
                value: $0.concealed ? "" : $0.value, concealed: $0.concealed, changed: false) }
        } else { kind = target.kind; favorite = target.favorite; addTemplate() }
    }

    /// Preserve edits and custom fields; replace only untouched, empty defaults.
    mutating func addTemplate(from previousKind: String? = nil) {
        if let previousKind {
            let previous = Set(vaultTemplate(kind: previousKind).map(\.id))
            let next = Set(vaultTemplate(kind: kind).map(\.id))
            fields.removeAll { previous.contains($0.id) && !next.contains($0.id) && !$0.changed && $0.value.isEmpty && $0.label.isEmpty && $0.section.isEmpty }
        }
        for field in vaultTemplate(kind: kind) where !fields.contains(where: { $0.id == field.id }) {
            fields.append(Field(id: field.id, kind: field.kind, label: "", section: "", value: "", concealed: field.concealed, changed: false))
        }
    }
}

/// Draft ownership stays in the session; an explicit edit opens above its source
/// screen, including when that screen is already in a sheet. Restored drafts use
/// the root presenter after the old navigation hierarchy has been removed.
struct VaultEditorPresentation: ViewModifier {
    var owner: UUID? = nil
    @Environment(SessionModel.self) private var model

    func body(content: Content) -> some View {
        content.sheet(item: Binding(
            get: { model.vaultEditing.presenter == owner ? model.vaultEditing.presented : nil },
            set: { if model.vaultEditing.presenter == owner { model.vaultEditing.presented = $0 } }
        )) { entry in
            NavigationStack { VaultEditor(entry: entry).id(model.viewGeneration) }
        }
    }
}

struct VaultEditor: View {
    let entry: VaultEditorSession
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var initial: VaultDraft
    @State private var openingGeneration: UInt64 = 0
    @State private var saving = false
    @State private var discard = false
    @State private var failure: String?
    @State private var choosingAttachment = false
    @State private var attachmentFiles: [String: LibraryFile] = [:]
    @State private var editMode: EditMode = .inactive
    @State private var addedField: String?
    @State private var customizingFields = false
    /// Each save attempt reveals every error and scrolls to the first one, even after an earlier attempt.
    @State private var saveAttempts = 0
    @FocusState private var focus: VaultEditorFocus?
    private var draft: VaultDraft {
        get { model.vaultEditing.form(id: entry.id) ?? entry.draft }
        nonmutating set {
            guard openingGeneration == model.viewGeneration else { return }
            model.vaultEditing.update(newValue, id: entry.id, model: model)
        }
    }
    private var form: Binding<VaultDraft> { Binding(get: { draft }, set: { draft = $0 }) }
    private var dirty: Bool { draft != initial }
    private var quickLogin: Bool {
        entry.itemID == nil && draft.kind == "login" && !customizingFields
    }

    init(entry: VaultEditorSession) {
        self.entry = entry
        _initial = State(initialValue: entry.initial)
        let template = vaultTemplate(kind: "login").map(\.id)
        _customizingFields = State(initialValue: entry.draft.fields.map(\.id) != template || entry.draft.fields.contains {
            !$0.label.isEmpty || !$0.section.isEmpty || (!["username", "password"].contains($0.id) && !$0.value.isEmpty)
        })
    }

    var body: some View {
        ScrollViewReader { proxy in
            Form {
                Section {
                    VStack(alignment: .leading) {
                        Text("Title").font(.caption).foregroundStyle(.secondary)
                        TextField("Title", text: form.title).focused($focus, equals: .title).accessibilityIdentifier("vault.edit.title")
                            .submitLabel(quickLogin ? .next : .done).onSubmit { focus = quickLogin ? .value("username") : nil }
                    }
                    if quickLogin {
                        ForEach(form.fields) { $field in
                            if field.id == "username" || field.id == "password" {
                                VaultEditorField(field: $field, existing: false, validationRequested: saveAttempts > 0,
                                                 addedField: $addedField, focus: $focus, compactLogin: true, submitLogin: { Task { await save() } })
                            }
                        }
                    }
                    if entry.itemID == nil {
                        Picker("Type", selection: form.kind) {
                            ForEach(vaultKinds(), id: \.self) { Text(vaultKindName($0)).tag($0) }
                        }.accessibilityIdentifier("vault.edit.type")
                    }
                    Toggle("Favorite", isOn: form.favorite)
                    VaultTagEditor(tags: form.tags, entry: form.tagEntry).focused($focus, equals: .tags)
                }
                Section {
                    if quickLogin {
                        Button("Customize fields") { customizingFields = true }.accessibilityIdentifier("vault.edit.customize")
                    } else {
                        ForEach(form.fields) { $field in
                            Group {
                                if editMode.isEditing {
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(field.title)
                                        Text(vaultFieldKindTitle(field.kind)).font(.caption).foregroundStyle(.secondary)
                                    }.padding(.vertical, 4)
                                } else {
                                    VaultEditorField(field: $field, existing: entry.itemID != nil, validationRequested: saveAttempts > 0,
                                                     addedField: $addedField, focus: $focus)
                                }
                            }.id(field.id)
                        }.onMove { draft.fields.move(fromOffsets: $0, toOffset: $1) }
                            .onDelete { draft.fields.remove(atOffsets: $0) }
                    }
                    if !editMode.isEditing {
                        Menu {
                            addField("Text", kind: "text"); addField("Hidden value", kind: "concealed")
                            addField("Website", kind: "url"); addField("Email", kind: "email"); addField("Phone", kind: "phone")
                            addField("Date", kind: "date"); addField("Month", kind: "month"); addField("One-time code", kind: "otp")
                        } label: {
                            Label("Add field", systemImage: "plus")
                                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).contentShape(Rectangle())
                        }.buttonStyle(.borderless).disabled(draft.fields.count >= 64).accessibilityIdentifier("vault.edit.add-field")
                    }
                } header: {
                    HStack {
                        Text("Fields")
                        Spacer()
                        Button(editMode.isEditing ? "Done" : "Reorder fields") {
                            focus = nil
                            customizingFields = true
                            withAnimation { editMode = editMode.isEditing ? .inactive : .active }
                        }.frame(minWidth: 44, minHeight: 44).disabled(draft.fields.isEmpty)
                            .accessibilityIdentifier("vault.edit.reorder")
                    }
                }
                Section("Notes") { TextEditor(text: form.notes).focused($focus, equals: .notes).frame(minHeight: 120).accessibilityLabel("Notes") }
                Section("Item attachments") {
                    ForEach(draft.files, id: \.self) { id in
                        HStack {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(attachmentFiles[id]?.name ?? id).lineLimit(2)
                                if attachmentFiles[id] == nil { Text("File not available on this device").font(.caption).foregroundStyle(.secondary) }
                            }
                            Spacer()
                            Button("Remove attachment reference", systemImage: "minus.circle", role: .destructive) { draft.files.removeAll { $0 == id } }
                                .labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44)
                        }
                    }
                    Button("Attach a file", systemImage: "paperclip") { choosingAttachment = true }.disabled(draft.files.count >= 20)
                    Text("Removing a reference does not delete the file. Existing references are kept even when their files are missing on this device.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                Text("Saved locally first. Sync status is shown separately.").font(.footnote).foregroundStyle(.secondary)
            }.environment(\.editMode, $editMode).disabled(saving || !model.vaultEditing.ready)
                .accessibilityIdentifier("vault.edit.form")
                .safeAreaInset(edge: .top) {
                    if let failure = failure ?? model.vaultEditing.failure {
                        NoticeBar(text: failure).padding(.horizontal, 16).padding(.vertical, 4)
                            .accessibilityElement(children: .combine).accessibilityIdentifier("vault.edit.failure")
                        if model.vaultEditing.failure != nil {
                            Button("Retry") { Task { await model.vaultEditing.flush(model); await model.vaultEditing.load(model) } }
                        }
                    }
                }.privateScreen()
                .navigationTitle(entry.itemID == nil ? "New secret" : "Edit secret").navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { if dirty { discard = true } else { Task { await discardDraft() } } }.disabled(saving).keyboardShortcut(.cancelAction) }
                    ToolbarItem(placement: .confirmationAction) {
                        Button { Task { await save() } } label: { if saving { ProgressView() } else { Text("Save") } }
                            .disabled(draft.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || saving || !model.vaultEditing.ready)
                            .accessibilityIdentifier("vault.edit.save")
                            .keyboardShortcut("s", modifiers: .command)
                    }
                }
                .keyboardDone(visible: focus != nil, identifier: "vault.edit.keyboard-done") { focus = nil }
                .background(DraftDismissGuard(blocked: dirty || saving,
                    attempted: { if !saving { discard = true } },
                    dismissed: { Task { await discardDraft() } }).frame(width: 0, height: 0))
                .confirmationDialog("Discard unsaved changes?", isPresented: $discard, titleVisibility: .visible) {
                    Button("Discard", role: .destructive) { Task { await discardDraft() } }
                    Button("Cancel", role: .cancel) {}
                }
                .onAppear {
                    openingGeneration = model.viewGeneration
                }
                .onChange(of: draft.kind) { previous, _ in if entry.itemID == nil { draft.addTemplate(from: previous) } }
                .onChange(of: addedField) { _, id in
                    if let id { withAnimation { proxy.scrollTo(id, anchor: .top) } }
                }
                .onChange(of: saveAttempts) { _, _ in
                    if let field = draft.fields.first(where: { vaultInputError($0, existing: entry.itemID != nil) != nil }) {
                        withAnimation { proxy.scrollTo(field.id, anchor: .top) }
                    }
                }
                .task { await loadAttachments() }
                .sheet(isPresented: $choosingAttachment) {
                    NavigationStack { AttachmentPicker(folder: "root") { file in
                        if !draft.files.contains(file.id) { draft.files.append(file.id) }
                        attachmentFiles[file.id] = file; choosingAttachment = false
                    } }
                }
        }
    }

    private func addField(_ title: LocalizedStringKey, kind: String) -> some View {
        Button(title) {
            customizingFields = true
            let id = "field_" + UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(16).lowercased()
            draft.fields.append(.init(id: id,
                kind: kind, label: "", section: "", value: "", concealed: ["concealed", "otp"].contains(kind), changed: true))
            addedField = id
        }.accessibilityIdentifier("vault.edit.add-field.\(kind)")
    }

    private func loadAttachments() async {
        let epoch = model.epoch
        do {
            if let id = entry.itemID, let result = try await model.engine?.secretItemAttachments(itemId: id), epoch == model.epoch {
                let listing: SecretAttachmentListing = try model.decode(result)
                for entry in listing.files { if let file = entry.details?.file { attachmentFiles[entry.id] = file } }
            }
            for id in draft.files where attachmentFiles[id] == nil {
                let details = try? await model.fileDetails(id)
                guard epoch == model.epoch, !Task.isCancelled else { return }
                attachmentFiles[id] = details?.file
            }
        } catch { if epoch == model.epoch { failure = vaultReadError(error) } }
    }

    private func save() async {
        guard !saving, model.vaultEditing.ready else { return }
        guard !draft.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { focus = .title; return }
        focus = nil
        editMode = .inactive
        saveAttempts += 1
        if draft.fields.contains(where: { vaultInputError($0, existing: entry.itemID != nil) != nil }) {
            customizingFields = true
            return
        }
        saving = true; failure = nil; defer { saving = false }; let scope = model.viewGeneration
        // A tag still being typed counts.
        draft.tags = (vaultTags(draft.tags) + vaultTags(draft.tagEntry)).joined(separator: ", "); draft.tagEntry = ""
        model.vaultEditing.update(draft, id: entry.id, model: model)
        do {
            let edit = VaultEdit(title: draft.title.trimmingCharacters(in: .whitespacesAndNewlines), kind: draft.kind, notes: draft.notes,
                favorite: draft.favorite, tags: vaultTags(draft.tags),
                fields: draft.fields.map { field in
                    let value = ["date", "month", "otp", "url", "email", "phone"].contains(field.kind) ? field.value.trimmingCharacters(in: .whitespacesAndNewlines) : field.value
                    return FieldEdit(id: field.id, kind: field.kind, label: field.label, section: field.section, value: entry.itemID != nil && !field.changed ? nil : value)
                },
                files: entry.itemID == nil || draft.files != initial.files ? draft.files : nil)
            let conflict: Bool
            #if DEBUG
            if model.uiTesting {
                try model.saveVaultFixture(target: VaultEditorTarget(item: model.fixtureVaultItems.first { $0.id == entry.itemID }), edit: edit); conflict = false
            }
            else {
                guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
                conflict = try await engine.commitVaultDraft(draft: model.vaultEditing.input(id: entry.id), edit: edit).item.conflict
            }
            #else
            guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
            conflict = try await engine.commitVaultDraft(draft: model.vaultEditing.input(id: entry.id), edit: edit).item.conflict
            #endif
            guard scope == model.viewGeneration else { return }
            model.vaultEditing.finished(entry.id); initial = draft
            model.notice = String(localized: conflict ? "Saved as a separate version. Review sync conflicts." : "Saved on this device")
            if model.uiTesting { await model.loadVault() } else { await model.updateSession() }
            dismiss()
        } catch { if scope == model.viewGeneration { failure = vaultReadError(error) } }
    }

    private func discardDraft() async {
        guard !saving, openingGeneration == model.viewGeneration, model.unlocked else { return }
        saving = true; defer { saving = false }
        do { try await model.vaultEditing.discard(entry.id, model: model); dismiss() }
        catch { failure = vaultReadError(error) }
    }
}

/// Gives swipe-to-dismiss the same draft decision as the Cancel button.
struct DraftDismissGuard: UIViewControllerRepresentable {
    let blocked: Bool
    let attempted: () -> Void
    var dismissed: () -> Void = {}
    func makeUIViewController(context: Context) -> Controller { Controller() }
    func updateUIViewController(_ controller: Controller, context: Context) {
        controller.blocked = blocked; controller.attempted = attempted; controller.dismissed = dismissed
        controller.attach()
    }
    final class Controller: UIViewController, UIAdaptivePresentationControllerDelegate {
        var blocked = false
        var attempted: () -> Void = {}
        var dismissed: () -> Void = {}
        override func viewDidAppear(_ animated: Bool) { super.viewDidAppear(animated); attach() }
        func attach() {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                var parent: UIViewController = self
                while let next = parent.parent { parent = next }
                parent.isModalInPresentation = self.blocked
                parent.presentationController?.delegate = self
            }
        }
        func presentationControllerShouldDismiss(_ presentationController: UIPresentationController) -> Bool { !blocked }
        func presentationControllerDidAttemptToDismiss(_ presentationController: UIPresentationController) { attempted() }
        func presentationControllerDidDismiss(_ presentationController: UIPresentationController) { dismissed() }
    }
}
