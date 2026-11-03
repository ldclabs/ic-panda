import SwiftUI
@preconcurrency import DMsgCore

struct VaultDetailView: View {
    private struct ReferenceTarget: Hashable { let itemID: String; let fieldID: String }
    let id: String
    var revision: String? = nil
    var versionDate: UInt64? = nil
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var item: VaultItem?
    @State private var failure: String?
    @State private var reload = 0
    @State private var revealed: [String: RevealedVaultValue] = [:]
    @State private var pendingField: VaultField?
    @State private var copying = false
    @State private var workingField: String?
    @State private var working = false
    @State private var editing: VaultEditorTarget?
    @State private var referenceTarget: ReferenceTarget?
    @State private var revealTask: Task<Void, Never>?
    @State private var revealRequest: UUID?
    @State private var deleting = false

    private var readKey: String { "\(id):\(revision ?? "head"):\(model.epoch):\(model.concealed):\(model.contentRevision):\(model.vault.first { $0.id == id }?.changeToken ?? "unlisted"):\(reload)" }

    var body: some View {
        List {
            if let failure, item != nil {
                NoticeBar(text: failure)
                Button("Retry") { reload &+= 1 }
            }
            if let item {
                header(item)
                if item.deleted {
                    NoticeBar(text: String(localized: "In Trash"))
                    if revision == nil { Button("Restore") { Task { await restore(item) } }.disabled(working) }
                }
                if item.reserved { NoticeBar(text: String(localized: "This key is managed by dMsg and cannot be copied or edited.")) }
                if revision != nil {
                    NoticeBar(text: String(localized: "You are viewing an immutable historical version."))
                    Text(item.revision).font(.caption.monospaced()).textSelection(.enabled)
                }
                if item.conflict { NavigationLink("Review sync conflicts") { ConflictsView() } }
                ForEach(groups(item), id: \.self) { group in
                    Section {
                        ForEach(item.fields.filter { $0.section == group }, id: \.id) { field in
                            TimelineView(.periodic(from: .now, by: 1)) { timeline in
                                VaultDetailField(field: field, value: revealed[field.id]?.value(at: timeline.date),
                                    remaining: revealed[field.id]?.remaining(at: timeline.date),
                                    busy: workingField == field.id, readOnly: item.reserved || item.deleted,
                                    showReferences: revision == nil,
                                    reveal: {
                                        if revealed[field.id]?.value(at: .now) != nil { revealed.removeValue(forKey: field.id) }
                                        else { pendingField = field; copying = false }
                                    }, copy: { pendingField = field; copying = true },
                                    reference: { referenceTarget = ReferenceTarget(itemID: item.id, fieldID: field.id) })
                            }
                        }
                    } header: { if !group.isEmpty { Text(group) } }
                }
                if !item.notes.isEmpty { Section("Notes") { Text(item.notes).textSelection(.enabled) } }
                if item.fields.isEmpty && item.notes.isEmpty { Text("No fields or notes").foregroundStyle(.secondary) }
                if !item.reserved {
                    Section {
                        NavigationLink("Version history") { VaultHistoryView(item: item) }
                        if revision == nil && !item.deleted {
                            NavigationLink { SecretItemAttachmentsView(itemID: item.id) } label: {
                                Label { Text("Item attachments") + Text(verbatim: " · \(item.files.count)") } icon: { Image(systemName: "paperclip") }
                            }
                            NavigationLink { SecretSnapshotComposer(item: item) } label: { Label("Share secret snapshot", systemImage: "square.and.arrow.up") }
                                .accessibilityIdentifier("vault.share")
                            if item.kind == "login" { NavigationLink("Password AutoFill") { SystemExportsView(initialCredential: item) } }
                            NavigationLink("Local security review") { SecretHealthView() }
                            Button("Move to Trash", systemImage: "trash", role: .destructive) { deleting = true }
                                .accessibilityIdentifier("vault.detail.trash")
                        }
                    }
                }
            } else if let failure {
                NoticeBar(text: failure)
                Button("Retry") { reload &+= 1 }.accessibilityIdentifier("vault.detail.retry")
            } else { ProgressView().accessibilityIdentifier("vault.detail.loading") }
        }.privateScreen().navigationTitle(revision == nil ? "Secret" : "Historical version").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItemGroup(placement: .primaryAction) {
                    WorkspaceStatusButton()
                    if let item, item.canEdit, revision == nil {
                        Button { Task { await favorite(item) } } label: { Image(systemName: item.favorite ? "star.fill" : "star") }
                            .accessibilityLabel(item.favorite ? "Remove favorite" : "Favorite").accessibilityIdentifier("vault.detail.favorite").disabled(working)
                        Button("Edit item", systemImage: "pencil") { clearRevealed(); editing = VaultEditorTarget(item: item) }
                            .accessibilityIdentifier("vault.edit.open")
                    }
                }
            }
            .task(id: readKey) { if !model.concealed && editing == nil { await load() } }
            .onDisappear { clearRevealed() }
            .onChange(of: id) { _, _ in editing = nil; referenceTarget = nil }
            .onChange(of: model.concealed) { _, value in if value { clearRevealed() } }
            .sheet(item: $editing, onDismiss: { reload &+= 1 }) { target in NavigationStack { VaultEditor(target: target) } }
            .navigationDestination(item: $referenceTarget) { target in SecretFieldReferenceView(itemID: target.itemID, fieldID: target.fieldID) }
            .confirmationDialog(copying ? "Copy this field?" : "Reveal this field?", isPresented: Binding(get: { pendingField != nil }, set: { if !$0 { pendingField = nil } }), titleVisibility: .visible) {
                Button(copying ? "Copy for 30 seconds" : "Reveal") {
                    if let field = pendingField { revealTask?.cancel(); revealTask = Task { await reveal(field) } }
                }
            } message: {
                if let field = pendingField, let item {
                    Text(item.title + " · " + vaultFieldTitle(field))
                    Text(copying ? "Copies stay on this device and expire within 30 seconds." : "This field will be visible until you hide it or leave this screen.")
                }
            }
            .confirmationDialog("Move this secret to Trash?", isPresented: $deleting, titleVisibility: .visible) {
                Button("Move to Trash", role: .destructive) { if let item { Task { await remove(item) } } }
            } message: { Text(item?.title ?? "") }
    }

    private func header(_ item: VaultItem) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 12) {
                ObjectIcon(glyph: .vaultItem(item.kind))
                VStack(alignment: .leading, spacing: 4) {
                    Text(item.title).font(.title2.bold()).accessibilityIdentifier("vault.detail.title")
                    HStack(spacing: 8) {
                        Text(vaultKindName(item.kind))
                        Label(item.deleted ? "In Trash" : item.conflict ? "Sync conflicts" : item.synced ? "Synced" : "Waiting to sync",
                            systemImage: item.deleted ? "trash" : item.conflict ? "arrow.triangle.branch" : item.synced ? "checkmark.icloud" : "arrow.triangle.2.circlepath")
                    }.font(.caption).foregroundStyle(.secondary)
                }
            }
            if (versionDate ?? item.updatedAt) > 0 {
                LabeledContent(revision == nil ? "Updated" : "Date") { Text(Date(timeIntervalSince1970: Double(versionDate ?? item.updatedAt) / 1000), format: .dateTime) }.font(.caption).foregroundStyle(.secondary)
            }
            if !item.tags.isEmpty { Text(item.tags.joined(separator: " · ")).font(.caption).foregroundStyle(Theme.forest) }
        }.padding(.vertical, 4).listRowBackground(Color.clear).listRowSeparator(.hidden)
    }

    private func groups(_ item: VaultItem) -> [String] {
        item.fields.reduce(into: [String]()) { if !$0.contains($1.section) { $0.append($1.section) } }
    }

    private func load() async {
        clearRevealed(); failure = nil
        if item?.id != id { item = nil }
        let epoch = model.epoch
        do {
            #if DEBUG
            if model.uiTesting {
                if ProcessInfo.processInfo.arguments.contains("--ui-slow-vault") { try await Task.sleep(for: .seconds(2)) }
                if ProcessInfo.processInfo.arguments.contains("--ui-vault-load-failure") && reload == 0 { throw MobileError.Failure(code: "CANCELLED") }
            }
            #endif
            let loaded = try await model.readVaultItem(id: id, revision: revision)
            guard model.epoch == epoch, !model.concealed, !Task.isCancelled else { return }
            if revision == nil, let previous = item, previous.id == loaded.id,
               previous.revision != loaded.revision, model.notice == nil {
                model.notice = String(localized: "Updated") + " · " + loaded.title
            }
            item = loaded
        } catch {
            guard model.epoch == epoch, !model.concealed, !Task.isCancelled else { return }
            failure = vaultReadError(error)
        }
    }

    private func clearRevealed() {
        revealTask?.cancel(); revealTask = nil; revealRequest = nil; workingField = nil
        revealed = [:]; pendingField = nil
    }

    private func reveal(_ field: VaultField) async {
        guard let item, !item.reserved, !item.deleted else { return }
        let copy = copying, epoch = model.epoch, request = UUID(), requestedAt = Date.now
        revealRequest = request; pendingField = nil; workingField = field.id; model.activity()
        defer { if revealRequest == request { workingField = nil } }
        do {
            let value: SecretValue
            #if DEBUG
            if model.uiTesting {
                value = SecretValue(generation: 0, value: field.kind == "otp" ? "123456" : field.concealed ? "fixture-secret" : field.value,
                    remainingMs: field.kind == "otp" ? 5000 : nil)
            } else {
                guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
                value = try await engine.revealField(id: item.id, revision: item.revision, field: field.id)
            }
            #else
            guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
            value = try await engine.revealField(id: item.id, revision: item.revision, field: field.id)
            #endif
            guard epoch == model.epoch, value.generation == model.session?.generation, revealRequest == request, !model.concealed, !Task.isCancelled else { return }
            // Conservatively include the read's latency; never extend a code's validity.
            let expiry = value.remainingMs.map { requestedAt.addingTimeInterval(Double($0) / 1000) }
            let lifetime = expiry.map { max(0, $0.timeIntervalSinceNow) }
            if lifetime == 0 { model.notice = String(localized: "Expired"); return }
            if copy {
                model.clipboard.copy(value.value, lifetime: min(30, lifetime ?? 30))
                model.notice = String(localized: value.remainingMs == nil ? "Copied for 30 seconds" : "Copied. The code expires shortly.")
            } else { revealed[field.id] = RevealedVaultValue(text: value.value, expires: expiry) }
        } catch { if epoch == model.epoch && !Task.isCancelled { model.report(error) } }
    }

    private func favorite(_ item: VaultItem) async {
        working = true; defer { working = false }
        do { try await model.setVaultFavorite(item); reload &+= 1 }
        catch { model.report(error) }
    }

    private func remove(_ item: VaultItem) async {
        working = true; defer { working = false }; clearRevealed()
        do { try await model.trashVaultItem(item); dismiss() }
        catch { model.report(error) }
    }

    private func restore(_ item: VaultItem) async {
        working = true; defer { working = false }
        do { try await model.trashVaultItem(item, restore: true); reload &+= 1 }
        catch { model.report(error) }
    }
}

private struct VaultDetailField: View {
    let field: VaultField
    let value: String?
    let remaining: Int?
    let busy: Bool
    let readOnly: Bool
    let showReferences: Bool
    let reveal: () -> Void
    let copy: () -> Void
    let reference: () -> Void
    @Environment(\.dynamicTypeSize) private var size
    @Environment(\.layoutDirection) private var direction
    private var text: String { value ?? field.value }
    private var stacked: Bool { size.isAccessibilitySize || text.count > 34 || text.contains("\n") }
    private var layout: AnyLayout { stacked ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8)) : AnyLayout(HStackLayout(spacing: 8)) }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(vaultFieldTitle(field)).font(.caption).foregroundStyle(.secondary).accessibilityHidden(true)
            layout {
                Text(text.isEmpty ? "—" : text).font(field.concealed ? .body.monospaced() : .body)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .environment(\.layoutDirection, vaultFieldIsLTR(field, value: text) ? .leftToRight : direction)
                    .accessibilityLabel(vaultFieldTitle(field)).accessibilityValue(spokenValue)
                    .accessibilityIdentifier("vault.value.\(field.id)")
                if !readOnly {
                    HStack(spacing: 4) {
                        if busy { ProgressView().frame(width: 44, height: 44) }
                        else {
                            if field.concealed && !field.value.isEmpty {
                                Button(action: reveal) { Image(systemName: value == nil ? "eye" : "eye.slash").frame(width: 44, height: 44) }
                                    .accessibilityLabel(Text(value == nil ? "Reveal" : "Hide") + Text(verbatim: " · " + vaultFieldTitle(field)))
                                    .accessibilityIdentifier("vault.reveal.\(field.id)")
                            }
                            if !field.value.isEmpty {
                                Button(action: copy) { Image(systemName: "doc.on.doc").frame(width: 44, height: 44) }
                                    .accessibilityLabel(Text("Copy") + Text(verbatim: " · " + vaultFieldTitle(field))).accessibilityIdentifier("vault.copy.\(field.id)")
                            }
                            if showReferences {
                                Menu { Button("Field reference and dependencies", systemImage: "link", action: reference) } label: {
                                    Image(systemName: "ellipsis").frame(width: 44, height: 44)
                                }.accessibilityLabel(Text("Field settings") + Text(verbatim: " · " + vaultFieldTitle(field)))
                            }
                        }
                    }.font(.system(size: 20)).buttonStyle(.borderless)
                }
            }
            if let remaining, value != nil {
                LabeledContent("Remaining", value: remaining.formatted()).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
        }.padding(.vertical, 5)
    }

    private var spokenValue: String {
        if field.concealed && value == nil { return String(localized: "Hidden value") }
        if field.kind == "otp" { return text.map(String.init).joined(separator: " ") }
        return text
    }
}
