import SwiftUI
@preconcurrency import DMsgCore

struct ConflictsView: View {
    @Environment(SessionModel.self) private var model
    @State private var conflicts: [ConflictViewData] = []
    @State private var loading = true
    @State private var failed = false
    var body: some View {
        List {
            ForEach(conflicts) { conflict in NavigationLink { ConflictDetail(conflict: conflict) } label: {
                VStack(alignment: .leading, spacing: 4) { Text(conflict.head.item.title).font(.headline); Text("Two versions need your decision").font(.caption).foregroundStyle(.secondary) }
            } }
            if loading { ProgressView() }
            else if failed { Button("Retry") { Task { await load() } } }
            else if conflicts.isEmpty { ContentUnavailableView("No items", systemImage: "checkmark.circle") }
        }.privateScreen().navigationTitle("Sync conflicts").task(id: model.contentRevision) { await load() }.refreshable { await load() }
    }
    private func load() async {
        loading = true; failed = false; defer { loading = false }; let epoch = model.epoch
        do {
            guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
            let snapshot = try await engine.conflicts()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            conflicts = try model.decode(snapshot)
        } catch { if epoch == model.epoch && !Task.isCancelled { failed = true } }
    }
}

struct ConflictViewData: Decodable, Identifiable {
    struct Entry: Decodable, Identifiable { let id: String; let revision: String; let item: Item; let deleted: Bool }
    struct Item: Decodable { let title: String; let notes: String; let fields: [Field]; let tags: [String]; let favorite: Bool; let files: [String] }
    struct Field: Decodable, Identifiable {
        let id: String; let label: String; let value: String; let section: String; let type: String
        var title: String { label.isEmpty ? vaultFieldName(id) : label }
        var description: String { (section.isEmpty ? "" : section + " / ") + title + " · " + vaultFieldKindTitle(type) + "\n" + value }
    }
    let copy: Entry; let head: Entry; let hidden: [String]
    var id: String { copy.id + ":" + copy.revision }
}

struct ConflictDetail: View {
    let conflict: ConflictViewData
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var choice: Bool?
    @State private var inspecting: ConflictViewData.Entry?
    @State private var merging = false
    @State private var mergeConfirmation = false
    @State private var theirs: Set<String> = []
    @State private var working = false
    var body: some View {
        List {
            NoticeBar(text: String(localized: "Choose which version to keep. Existing history is retained; secrets are never silently overwritten."))
            version(conflict.head, title: "Current version")
            version(conflict.copy, title: "Conflicting version")
            if !conflict.hidden.isEmpty {
                NoticeBar(text: String(localized: "Hidden fields differ: ") + conflict.hidden.map { id in
                    (conflict.head.item.fields + conflict.copy.item.fields).first { $0.id == id }?.title ?? vaultFieldName(id)
                }.joined(separator: ", "))
            }
            Button("Keep current version") { choice = true }
            Button("Use conflicting version") { choice = false }
            if !conflict.head.deleted && !conflict.copy.deleted { Toggle("Merge individual fields", isOn: $merging) }
            if merging {
                Text("Selected parts use the conflicting version. Other parts keep the current version.").font(.footnote).foregroundStyle(.secondary)
                ForEach(parts, id: \.id) { part in
                    Toggle(isOn: Binding(get: { theirs.contains(part.id) }, set: { if $0 { theirs.insert(part.id) } else { theirs.remove(part.id) } })) {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(part.title).font(.headline)
                            LabeledContent("Current version", value: part.current)
                            LabeledContent("Conflicting version", value: part.theirs)
                        }
                    }
                }
                Button("Save merged version") { mergeConfirmation = true }
            }
        }.disabled(working).privateScreen().navigationTitle("Choose a version")
            .onChange(of: conflict.head.revision) { _, _ in choice = nil; mergeConfirmation = false; merging = false; theirs = [] }
            .sheet(item: $inspecting) { entry in NavigationStack { VaultDetailView(id: entry.id, revision: entry.revision) } }
            .confirmationDialog("Save the selected version?", isPresented: Binding(get: { choice != nil }, set: { if !$0 { choice = nil } })) {
                Button("Save selected version") { if let choice { Task { await resolve(choice) } } }
            }
            .confirmationDialog("Save merged version?", isPresented: $mergeConfirmation, titleVisibility: .visible) {
                Button("Save merged version") { Task { await merge() } }
            }
    }

    private func version(_ entry: ConflictViewData.Entry, title: LocalizedStringKey) -> some View {
        Section(title) {
            Text(entry.item.title).font(.headline)
            if entry.deleted { Label("In Trash", systemImage: "trash").foregroundStyle(.secondary) }
            ForEach(entry.item.fields) { field in LabeledContent(field.title, value: field.value) }
            if !entry.item.notes.isEmpty { Text(entry.item.notes) }
            Button("Inspect this version") { inspecting = entry }
        }
    }

    private func resolve(_ current: Bool) async {
        choice = nil; guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }; let epoch = model.epoch
        do { _ = try await engine.resolveConflict(key: conflict.id, expectedRevision: conflict.head.revision, keepCurrent: current); if epoch == model.epoch { await model.updateSession(); dismiss() } }
        catch { if epoch == model.epoch { model.report(error) } }
    }

    private struct Part { let id: String; let title: String; let current: String; let theirs: String }
    private var parts: [Part] {
        let a = conflict.head.item, b = conflict.copy.item
        var result = [Part(id: "title", title: String(localized: "Title"), current: a.title, theirs: b.title),
            Part(id: "notes", title: String(localized: "Notes"), current: a.notes, theirs: b.notes),
            Part(id: "tags", title: String(localized: "Tags"), current: a.tags.joined(separator: ", "), theirs: b.tags.joined(separator: ", ")),
            Part(id: "favorite", title: String(localized: "Favorite"), current: String(localized: a.favorite ? "Yes" : "No"), theirs: String(localized: b.favorite ? "Yes" : "No")),
            Part(id: "files", title: String(localized: "Item attachments"), current: a.files.joined(separator: "\n"), theirs: b.files.joined(separator: "\n"))]
        var changedFields: Set<String> = []
        for id in Set(a.fields.map(\.id) + b.fields.map(\.id)).sorted() {
            let current = a.fields.first { $0.id == id }, other = b.fields.first { $0.id == id }
            if current?.value != other?.value || current?.label != other?.label || current?.section != other?.section || current?.type != other?.type || conflict.hidden.contains(id) { changedFields.insert("field:" + id) }
            result.append(Part(id: "field:" + id, title: current?.title ?? other?.title ?? vaultFieldName(id),
                current: current?.description ?? "—", theirs: other?.description ?? "—"))
        }
        return result.filter { $0.current != $0.theirs || changedFields.contains($0.id) }
    }

    private func merge() async {
        guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }; let epoch = model.epoch
        do {
            _ = try await engine.mergeVaultConflict(key: conflict.id, expectedRevision: conflict.head.revision, theirs: theirs.sorted())
            guard epoch == model.epoch else { return }
            await model.updateSession(); dismiss()
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
