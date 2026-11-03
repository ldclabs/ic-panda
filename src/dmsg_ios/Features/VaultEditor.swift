import SwiftUI
@preconcurrency import DMsgCore

struct VaultDraft: Codable, Equatable {
    struct Field: Codable, Equatable, Identifiable {
        var id: String
        var kind: String
        var label: String
        var section: String
        var value: String
        var concealed: Bool
        var changed: Bool
    }
    var title = ""
    var kind = "login"
    var notes = ""
    var favorite = false
    var tags = ""
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

struct VaultEditor: View {
    let target: VaultEditorTarget
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var draft: VaultDraft
    @State private var initial: VaultDraft
    @State private var initialized = false
    @State private var openingEpoch: UInt64 = 0
    @State private var saving = false
    @State private var discard = false
    @State private var failure: String?
    @State private var newFieldKind = "text"
    @State private var choosingAttachment = false
    @State private var attachmentFiles: [String: LibraryFile] = [:]
    @State private var editMode: EditMode = .inactive
    private var dirty: Bool { draft != initial }
    private var draftKey: String { "vault:" + target.id }
    private var invalid: Bool { draft.fields.contains { vaultInputError($0, existing: target.item != nil) != nil } }

    init(target: VaultEditorTarget) {
        self.target = target
        let draft = VaultDraft(target: target)
        _draft = State(initialValue: draft); _initial = State(initialValue: draft)
    }

    var body: some View {
        Form {
            Section {
                VStack(alignment: .leading) {
                    Text("Title").font(.caption).foregroundStyle(.secondary)
                    TextField("Title", text: $draft.title).accessibilityIdentifier("vault.edit.title")
                }
                if target.item == nil {
                    Picker("Type", selection: $draft.kind) {
                        ForEach(vaultKinds(), id: \.self) { Text(vaultKindName($0)).tag($0) }
                    }.accessibilityIdentifier("vault.edit.type")
                }
                Toggle("Favorite", isOn: $draft.favorite)
                VStack(alignment: .leading) {
                    Text("Tags").font(.caption).foregroundStyle(.secondary)
                    TextField("Tags, separated by commas", text: $draft.tags).textInputAutocapitalization(.never).autocorrectionDisabled()
                }
            }
            Section {
                ForEach($draft.fields) { $field in
                    VaultEditorField(field: $field, existing: target.item != nil)
                }.onMove { draft.fields.move(fromOffsets: $0, toOffset: $1) }
                    .onDelete { draft.fields.remove(atOffsets: $0) }
                Picker("New field type", selection: $newFieldKind) {
                    Text("Text").tag("text"); Text("Hidden value").tag("concealed")
                    Text("Website").tag("url"); Text("Email").tag("email"); Text("Phone").tag("phone")
                    Text("Date").tag("date"); Text("Month").tag("month"); Text("One-time code").tag("otp")
                }
                Button("Add field") {
                    draft.fields.append(.init(id: "field_" + UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(16).lowercased(),
                        kind: newFieldKind, label: "", section: "", value: "", concealed: ["concealed", "otp"].contains(newFieldKind), changed: true))
                }.disabled(draft.fields.count >= 64).accessibilityIdentifier("vault.edit.add-field")
            } header: { HStack { Text("Fields"); Spacer(); EditButton().frame(minWidth: 44, minHeight: 44) } }
            Section("Notes") { TextEditor(text: $draft.notes).frame(minHeight: 120).accessibilityLabel("Notes") }
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
        }.environment(\.editMode, $editMode).disabled(saving)
            .accessibilityIdentifier("vault.edit.form")
            .safeAreaInset(edge: .top) {
                if let failure {
                    NoticeBar(text: failure).padding(.horizontal, 16).padding(.vertical, 4)
                        .accessibilityElement(children: .combine).accessibilityIdentifier("vault.edit.failure")
                }
            }.privateScreen()
            .navigationTitle(target.item == nil ? "New secret" : "Edit secret").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { if dirty { discard = true } else { dismiss() } }.disabled(saving).keyboardShortcut(.cancelAction) }
                ToolbarItem(placement: .confirmationAction) {
                    Button { Task { await save() } } label: { if saving { ProgressView() } else { Text("Save") } }
                        .disabled(draft.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || invalid || saving)
                        .accessibilityIdentifier("vault.edit.save")
                        .keyboardShortcut("s", modifiers: .command)
                }
            }
            .background(DraftDismissGuard(blocked: dirty || saving) { if !saving { discard = true } }.frame(width: 0, height: 0))
            .confirmationDialog("Discard unsaved changes?", isPresented: $discard, titleVisibility: .visible) {
                Button("Discard", role: .destructive) { model.settingsDrafts[draftKey] = nil; dismiss() }
                Button("Cancel", role: .cancel) {}
            }
            .onAppear {
                guard !initialized else { return }
                openingEpoch = model.epoch
                if let data = model.settingsDrafts[draftKey], let saved = try? JSONDecoder().decode(VaultDraft.self, from: data) { draft = saved }
                initialized = true
            }
            .onChange(of: draft.kind) { previous, _ in if target.item == nil { draft.addTemplate(from: previous) } }
            .onChange(of: draft) { _, _ in
                guard initialized, openingEpoch == model.epoch, model.unlocked else { return }
                model.activity(); model.settingsDrafts[draftKey] = try? JSONEncoder().encode(draft)
            }
            .task { await loadAttachments() }
            .sheet(isPresented: $choosingAttachment) {
                NavigationStack { AttachmentPicker(folder: "root") { file in
                    if !draft.files.contains(file.id) { draft.files.append(file.id) }
                    attachmentFiles[file.id] = file; choosingAttachment = false
                } }
            }
    }

    private func loadAttachments() async {
        guard let id = target.item?.id else { return }; let epoch = model.epoch
        do {
            guard let result = try await model.engine?.secretItemAttachments(itemId: id), epoch == model.epoch else { return }
            let listing: SecretAttachmentListing = try model.decode(result)
            for entry in listing.files { if let file = entry.details?.file { attachmentFiles[entry.id] = file } }
        } catch { if epoch == model.epoch { failure = vaultReadError(error) } }
    }

    private func save() async {
        guard !saving else { return }
        saving = true; failure = nil; defer { saving = false }; let epoch = model.epoch
        do {
            let edit = VaultEdit(title: draft.title.trimmingCharacters(in: .whitespacesAndNewlines), kind: draft.kind, notes: draft.notes,
                favorite: draft.favorite, tags: draft.tags.split(whereSeparator: { $0 == "," || $0 == "，" || $0 == "،" }).map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty },
                fields: draft.fields.map { field in
                    let value = ["date", "month", "otp", "url", "email", "phone"].contains(field.kind) ? field.value.trimmingCharacters(in: .whitespacesAndNewlines) : field.value
                    return FieldEdit(id: field.id, kind: field.kind, label: field.label, section: field.section, value: target.item != nil && !field.changed ? nil : value)
                },
                files: target.item == nil || draft.files != initial.files ? draft.files : nil)
            let conflict: Bool
            #if DEBUG
            if model.uiTesting { try model.saveVaultFixture(target: target, edit: edit); conflict = false }
            else {
                guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
                conflict = try await engine.saveItem(id: target.item?.id, revision: draft.baseRevision, edit: edit).item.conflict
            }
            #else
            guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
            conflict = try await engine.saveItem(id: target.item?.id, revision: draft.baseRevision, edit: edit).item.conflict
            #endif
            guard epoch == model.epoch else { return }
            model.settingsDrafts[draftKey] = nil; initial = draft
            model.notice = String(localized: conflict ? "Saved as a separate version. Review sync conflicts." : "Saved on this device")
            if model.uiTesting { await model.loadVault() } else { await model.updateSession() }
            dismiss()
        } catch { if epoch == model.epoch { failure = vaultReadError(error) } }
    }
}

/// Gives swipe-to-dismiss the same draft decision as the Cancel button.
private struct DraftDismissGuard: UIViewControllerRepresentable {
    let blocked: Bool
    let attempted: () -> Void
    func makeUIViewController(context: Context) -> Controller { Controller() }
    func updateUIViewController(_ controller: Controller, context: Context) {
        controller.blocked = blocked; controller.attempted = attempted
        controller.attach()
    }
    final class Controller: UIViewController, UIAdaptivePresentationControllerDelegate {
        var blocked = false
        var attempted: () -> Void = {}
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
    }
}
