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
    /// One-time codes stay current on their own while the item is open.
    @State private var codes: [String: RevealedVaultValue] = [:]
    /// One confirmation covers this item's sensitive fields for 30 seconds.
    @State private var trustedUntil: Date?
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
                            if liveCode(field, item: item) {
                                TimelineView(.periodic(from: .now, by: 1)) { timeline in fieldRow(field, item: item, at: timeline.date) }
                            } else { fieldRow(field, item: item, at: .now) }
                        }
                    } header: { if !group.isEmpty { Text(group) } }
                }
                if !item.notes.isEmpty { Section("Notes") { Text(item.notes).textSelection(.enabled) } }
                if item.fields.isEmpty && item.notes.isEmpty { Text("No fields or notes").foregroundStyle(.secondary) }
            } else if let failure {
                NoticeBar(text: failure)
                Button("Retry") { reload &+= 1 }.accessibilityIdentifier("vault.detail.retry")
            } else { ProgressView().accessibilityIdentifier("vault.detail.loading") }
        }.privateScreen().navigationTitle(revision == nil ? "Secret" : "Historical version").navigationBarTitleDisplayMode(.inline)
            // The header already names the item.
            .toolbar(removing: revision == nil ? .title : nil)
            .toolbar {
                ToolbarItemGroup(placement: .primaryAction) {
                    if let item, item.canEdit, revision == nil {
                        Button { Task { await favorite(item) } } label: { Image(systemName: item.favorite ? "star.fill" : "star") }
                            .accessibilityLabel(item.favorite ? "Remove favorite" : "Favorite").accessibilityIdentifier("vault.detail.favorite").disabled(working)
                        Button("Edit item", systemImage: "pencil") { clearRevealed(); editing = VaultEditorTarget(item: item) }
                            .accessibilityIdentifier("vault.edit.open")
                    }
                    if let item, !item.reserved { actions(item) }
                }
            }
            .task(id: readKey) { if !model.concealed && editing == nil { await load() } }
            .task(id: "codes:\(item?.id ?? ""):\(item?.revision ?? ""):\(model.epoch):\(model.concealed)") { await watchCodes() }
            .onDisappear { clearRevealed(); codes = [:]; trustedUntil = nil }
            .onChange(of: id) { _, _ in editing = nil; referenceTarget = nil; codes = [:]; trustedUntil = nil }
            .onChange(of: model.concealed) { _, value in if value { clearRevealed(); codes = [:]; trustedUntil = nil } }
            .sheet(item: $editing, onDismiss: { reload &+= 1 }) { target in NavigationStack { VaultEditor(target: target) } }
            .navigationDestination(item: $referenceTarget) { target in SecretFieldReferenceView(itemID: target.itemID, fieldID: target.fieldID) }
            .confirmationDialog("Move this secret to Trash?", isPresented: $deleting, titleVisibility: .visible) {
                Button("Move to Trash", role: .destructive) { if let item { Task { await remove(item) } } }
            } message: { Text(item?.title ?? "") }
    }

    private func actions(_ item: VaultItem) -> some View {
        Menu("Actions", systemImage: "ellipsis.circle") {
            NavigationLink("Version history") { VaultHistoryView(item: item) }
            if revision == nil && !item.deleted {
                NavigationLink { SecretItemAttachmentsView(itemID: item.id) } label: {
                    Label { Text("Item attachments") + Text(verbatim: " · \(item.files.count)") } icon: { Image(systemName: "paperclip") }
                }
                NavigationLink { SecretSnapshotComposer(item: item) } label: { Label("Share secret snapshot", systemImage: "square.and.arrow.up") }
                    .accessibilityIdentifier("vault.share")
                if item.kind == "login" { NavigationLink("Password AutoFill") { SystemExportsView(section: .autoFill, initialCredential: item) } }
                if !item.fields.isEmpty {
                    Menu("Field reference and dependencies", systemImage: "link") {
                        ForEach(item.fields, id: \.id) { field in
                            Button(vaultFieldTitle(field)) { referenceTarget = ReferenceTarget(itemID: item.id, fieldID: field.id) }
                        }
                    }
                }
                Button("Move to Trash", systemImage: "trash", role: .destructive) { deleting = true }
                    .accessibilityIdentifier("vault.detail.trash")
            }
        }.accessibilityIdentifier("vault.detail.more")
    }

    private func liveCode(_ field: VaultField, item: VaultItem) -> Bool {
        field.kind == "otp" && !field.value.isEmpty && revision == nil && !item.reserved && !item.deleted
    }

    private func fieldRow(_ field: VaultField, item: VaultItem, at date: Date) -> some View {
        let live = liveCode(field, item: item)
        let shown = live ? codes[field.id] : revealed[field.id]
        return VaultDetailField(field: field, value: shown?.value(at: date), remaining: shown?.remaining(at: date),
            busy: workingField == field.id, readOnly: item.reserved || item.deleted, live: live,
            asks: field.concealed && !live && (trustedUntil.map { $0 <= .now } ?? true),
            reveal: {
                if revealed[field.id]?.value(at: .now) != nil { revealed.removeValue(forKey: field.id) }
                else { revealTask?.cancel(); revealTask = Task { await read(field, copy: false) } }
            }, copy: { revealTask?.cancel(); revealTask = Task { await read(field, copy: true) } })
    }

    private func watchCodes() async {
        guard let item, !model.concealed else { return }
        let fields = item.fields.filter { liveCode($0, item: item) }
        let epoch = model.epoch
        while !fields.isEmpty, !Task.isCancelled {
            var wait = 30.0
            for field in fields {
                let requestedAt = Date.now
                guard let value = try? await model.vaultFieldValue(item, field: field) else { continue }
                guard epoch == model.epoch, value.generation == model.session?.generation, !model.concealed, !Task.isCancelled else { return }
                // Conservatively include the read's latency; never extend a code's validity.
                let expiry = value.remainingMs.map { requestedAt.addingTimeInterval(Double($0) / 1000) }
                codes[field.id] = RevealedVaultValue(text: value.value, expires: expiry)
                if let expiry { wait = min(wait, max(0.5, expiry.timeIntervalSinceNow)) }
            }
            try? await Task.sleep(for: .seconds(wait))
        }
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
                            systemImage: item.deleted ? "trash" : item.conflict ? "arrow.triangle.branch" : item.synced ? "checkmark.circle" : "arrow.triangle.2.circlepath")
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
        revealed = [:]
    }

    private func read(_ field: VaultField, copy: Bool) async {
        guard let item, !item.reserved, !item.deleted else { return }
        model.activity()
        // Fields that are not sensitive are already on screen.
        if copy && !field.concealed {
            model.clipboard.copy(field.value)
            model.notice = String(localized: "Copied for 30 seconds")
            return
        }
        let epoch = model.epoch, request = UUID(), requestedAt = Date.now
        revealRequest = request; workingField = field.id
        defer { if revealRequest == request { workingField = nil } }
        do {
            let value = try await model.vaultFieldValue(item, field: field)
            guard epoch == model.epoch, value.generation == model.session?.generation, revealRequest == request, !model.concealed, !Task.isCancelled else { return }
            if field.concealed && !liveCode(field, item: item) { trustedUntil = .now.addingTimeInterval(30) }
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

/// A sensitive field asks first, next to the button that was pressed; a field that
/// is already on screen, or a one-time code, just copies.
private struct VaultDetailField: View {
    private enum Pending { case reveal, copy }
    let field: VaultField
    let value: String?
    let remaining: Int?
    let busy: Bool
    let readOnly: Bool
    let live: Bool
    let asks: Bool
    let reveal: () -> Void
    let copy: () -> Void
    @State private var pending: Pending?
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
                if let remaining, value != nil { countdown(remaining) }
                if !readOnly {
                    HStack(spacing: 4) {
                        if busy { ProgressView().frame(width: 44, height: 44) }
                        else {
                            if field.concealed && !live && !field.value.isEmpty {
                                Button { if value == nil && asks { pending = .reveal } else { reveal() } } label: {
                                    Image(systemName: value == nil ? "eye" : "eye.slash").frame(width: 44, height: 44)
                                }.accessibilityLabel(Text(value == nil ? "Reveal" : "Hide") + Text(verbatim: " · " + vaultFieldTitle(field)))
                                    .accessibilityIdentifier("vault.reveal.\(field.id)")
                                    .confirmationDialog("Reveal this field?", isPresented: asking(.reveal), titleVisibility: .visible) {
                                        Button("Reveal", action: reveal)
                                    } message: { Text("This field will be visible until you hide it or leave this screen.") }
                            }
                            if !field.value.isEmpty {
                                Button { if asks { pending = .copy } else { copy() } } label: {
                                    Image(systemName: "doc.on.doc").frame(width: 44, height: 44)
                                }.accessibilityLabel(Text("Copy") + Text(verbatim: " · " + vaultFieldTitle(field))).accessibilityIdentifier("vault.copy.\(field.id)")
                                    .confirmationDialog("Copy this field?", isPresented: asking(.copy), titleVisibility: .visible) {
                                        Button("Copy for 30 seconds", action: copy)
                                    } message: { Text("Copies stay on this device and expire within 30 seconds.") }
                            }
                        }
                    }.font(.system(size: 20)).buttonStyle(.borderless)
                }
            }
        }.padding(.vertical, 5)
    }

    private func asking(_ action: Pending) -> Binding<Bool> {
        Binding(get: { pending == action }, set: { if !$0 { pending = nil } })
    }

    private func countdown(_ remaining: Int) -> some View {
        ZStack {
            Circle().stroke(Theme.forest.opacity(0.15), lineWidth: 3)
            Circle().trim(from: 0, to: CGFloat(min(remaining, 30)) / 30)
                .stroke(Theme.forest, style: StrokeStyle(lineWidth: 3, lineCap: .round)).rotationEffect(.degrees(-90))
            Text(remaining.formatted()).font(.caption2.monospacedDigit())
        }.frame(width: 30, height: 30).accessibilityElement(children: .ignore)
            .accessibilityLabel("Remaining").accessibilityValue(remaining.formatted())
    }

    private var spokenValue: String {
        if field.concealed && value == nil { return String(localized: "Hidden value") }
        if field.kind == "otp" { return text.map(String.init).joined(separator: " ") }
        return text
    }
}
