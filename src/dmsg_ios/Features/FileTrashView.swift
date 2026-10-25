import SwiftUI

struct FileTrashView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var listing = LibraryListing.empty
    var body: some View {
        List {
            NoticeBar(text: String(localized: "Local trash is retained for 30 days. Cloud backups are managed separately."))
            ForEach(listing.folders) { folder in restoreRow(folder.id, name: folder.name) }
            ForEach(listing.files) { file in restoreRow(file.id, name: file.name) }
            if listing.files.isEmpty && listing.folders.isEmpty { ContentUnavailableView("Trash is empty", systemImage: "trash") }
        }.privateScreen().navigationTitle("Trash").task { await load() }
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
    }
    private func restoreRow(_ id: String, name: String) -> some View {
        HStack {
            Text(name).frame(maxWidth: .infinity, alignment: .leading)
            Button("Restore") { Task {
                do { try await model.engine?.restoreFile(id: id); await load(); await model.refresh() }
                catch { model.report(error) }
            } }.buttonStyle(.bordered)
        }
    }
    private func load() async {
        do { if let result = try await model.engine?.fileTrash(), !Task.isCancelled { listing = try model.decode(result) } }
        catch { model.report(error) }
    }
}
