import SwiftUI

struct ConflictsView: View {
    @Environment(SessionModel.self) private var model
    @State private var conflicts: [ConflictViewData] = []
    var body: some View {
        List {
            ForEach(conflicts) { conflict in NavigationLink { ConflictDetail(conflict: conflict) } label: {
                VStack(alignment: .leading, spacing: 4) { Text(conflict.head.item.title).font(.headline); Text("Two versions need your decision").font(.caption).foregroundStyle(.secondary) }
            } }
        }.privateScreen().navigationTitle("Sync conflicts").task {
            do { if let snapshot = try await model.engine?.conflicts() { conflicts = try model.decode(snapshot) } }
            catch { model.report(error) }
        }
    }
}

struct ConflictViewData: Decodable, Identifiable {
    struct Entry: Decodable, Identifiable { let id: String; let revision: String; let item: Item; let deleted: Bool }
    struct Item: Decodable { let title: String; let notes: String; let fields: [Field] }
    struct Field: Decodable, Identifiable { let id: String; let label: String; let value: String }
    let copy: Entry; let head: Entry; let hidden: [String]
    var id: String { copy.id + ":" + copy.revision }
}

struct ConflictDetail: View {
    let conflict: ConflictViewData
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var choice: Bool?
    @State private var inspecting: ConflictViewData.Entry?
    var body: some View {
        List {
            NoticeBar(text: String(localized: "Choose which version to keep. Existing history is retained; secrets are never silently overwritten."))
            version(conflict.head, title: "Current version")
            version(conflict.copy, title: "Conflicting version")
            if !conflict.hidden.isEmpty { NoticeBar(text: String(localized: "Hidden fields differ: ") + conflict.hidden.joined(separator: ", ")) }
            Button("Keep current version") { choice = true }
            Button("Use conflicting version") { choice = false }
        }.privateScreen().navigationTitle("Choose a version")
            .sheet(item: $inspecting) { entry in NavigationStack { VaultDetailView(id: entry.id, revision: entry.revision) } }
            .confirmationDialog("Save the selected version?", isPresented: Binding(get: { choice != nil }, set: { if !$0 { choice = nil } })) {
                Button("Save selected version") { if let choice { Task { await resolve(choice) } } }
            }
    }

    private func version(_ entry: ConflictViewData.Entry, title: LocalizedStringKey) -> some View {
        Section(title) {
            Text(entry.item.title).font(.headline)
            if entry.deleted { Label("In Trash", systemImage: "trash").foregroundStyle(.secondary) }
            ForEach(entry.item.fields) { field in LabeledContent(field.label.isEmpty ? field.id : field.label, value: field.value) }
            if !entry.item.notes.isEmpty { Text(entry.item.notes) }
            Button("Inspect this version") { inspecting = entry }
        }
    }

    private func resolve(_ current: Bool) async {
        choice = nil
        do { _ = try await model.engine?.resolveConflict(key: conflict.id, keepCurrent: current); await model.updateSession(); dismiss() }
        catch { model.report(error) }
    }
}
