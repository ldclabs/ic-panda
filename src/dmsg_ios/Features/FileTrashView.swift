import SwiftUI

struct FileTrashView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var listing = LibraryListing.empty
    @State private var loading = true
    @State private var failure: String?
    @State private var restoring: Set<String> = []
    @State private var errors: [String: String] = [:]
    @State private var visible = true
    @State private var query = ""
    @State private var folderMeta: [String: FileSearchMeta] = [:]
    @State private var fileMeta: [String: FileSearchMeta] = [:]
    var body: some View {
        List {
            if let days = listing.trashDays {
                Section {
                    LabeledContent("Days before removal", value: days.formatted())
                    if !model.localAccount { Text("Cloud backups are kept. This only changes the copy on this device.").font(.footnote).foregroundStyle(.secondary) }
                }
            }
            if loading { ProgressView("Loading…") }
            if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
            ForEach(listing.folders) { folder in restoreRow(folder.id, name: folder.name, mime: "inode/directory", time: folder.trashedAt, spans: folderMeta[folder.id]?.nameSpans) }
            ForEach(listing.files) { file in restoreRow(file.id, name: file.name, mime: file.mime, time: file.trashedAt, spans: fileMeta[file.id]?.nameSpans) }
            if !loading && failure == nil && listing.files.isEmpty && listing.folders.isEmpty {
                if query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    ContentUnavailableView("Trash is empty", systemImage: "trash")
                } else {
                    ContentUnavailableView.search(text: query)
                }
            }
        }.privateScreen().navigationTitle("Trash")
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search files")
            .task(id: query) {
                if !query.isEmpty { try? await Task.sleep(for: .milliseconds(160)) }
                if !Task.isCancelled { await load() }
            }.refreshable { await load() }
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .onAppear { visible = true }.onDisappear { visible = false }
    }

    private func restoreRow(_ id: String, name: String, mime: String, time: UInt64?, spans: [HighlightSpan]?) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            FileSummaryRow(name: name, mime: mime, subtitle: "", nameSpans: spans)
            if let time {
                LabeledContent("Deleted") { Text(fileDate(time), format: .dateTime) }.font(.footnote).foregroundStyle(.secondary)
                if let days = listing.trashDays {
                    LabeledContent("Removal date") { Text(fileDate(time).addingTimeInterval(Double(days) * 86_400), style: .date) }.font(.footnote).foregroundStyle(.secondary)
                }
            }
            if let error = errors[id] { Text(error).font(.footnote).foregroundStyle(.red) }
            Button("Restore") { Task { await restore(id) } }.buttonStyle(.bordered).disabled(restoring.contains(id))
                .accessibilityIdentifier("trash.restore.\(id)")
            if restoring.contains(id) { ProgressView() }
        }.padding(.vertical, 4)
    }

    private func restore(_ id: String) async {
        guard restoring.insert(id).inserted else { return }; let stamp = model.epoch
        errors[id] = nil; defer { restoring.remove(id) }
        do {
            #if DEBUG
            if model.uiTesting { try model.restoreTrashFixture(id) }
            else { try await restoreRemote(id) }
            #else
            try await restoreRemote(id)
            #endif
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }
            model.notice = String(localized: "Restored")
            await load(); await model.loadFiles()
        } catch { if stamp == model.epoch, visible, !Task.isCancelled { errors[id] = model.fileFailureMessage(error) } }
    }

    private func restoreRemote(_ id: String) async throws {
        guard let engine = model.engine else { throw CancellationError() }
        try await engine.restoreFile(id: id)
    }

    private func load() async {
        let stamp = model.epoch, trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        loading = true; failure = nil; defer { loading = false }
        do {
            let value: LibraryListing
            var nextFolderMeta: [String: FileSearchMeta] = [:]
            var nextFileMeta: [String: FileSearchMeta] = [:]
            #if DEBUG
            if model.uiTesting {
                let raw = model.fileTrashFixture()
                if trimmed.isEmpty { value = raw }
                else {
                    value = LibraryListing(folder: raw.folder,
                                           folders: raw.folders.filter { $0.name.localizedCaseInsensitiveContains(trimmed) },
                                           files: raw.files.filter { $0.name.localizedCaseInsensitiveContains(trimmed) },
                                           trashDays: raw.trashDays)
                }
            } else {
                let loaded = try await read(query: trimmed)
                value = loaded.listing; nextFolderMeta = loaded.folderMeta; nextFileMeta = loaded.fileMeta
            }
            #else
            let loaded = try await read(query: trimmed)
            value = loaded.listing; nextFolderMeta = loaded.folderMeta; nextFileMeta = loaded.fileMeta
            #endif
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }
            listing = value; folderMeta = nextFolderMeta; fileMeta = nextFileMeta
        } catch { if stamp == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func read(query: String) async throws -> (listing: LibraryListing, folderMeta: [String: FileSearchMeta], fileMeta: [String: FileSearchMeta]) {
        guard let engine = model.engine else { throw CancellationError() }
        if query.isEmpty {
            let raw: LibraryListing = try model.decode(try await engine.fileTrash())
            return (raw, [:], [:])
        }
        let response: FileSearchListingResponse = try await model.decodeInBackground(
            try await engine.searchFilesListing(query: query, folder: nil, starred: false, tag: nil, trash: true)
        )
        let fMeta = Dictionary(uniqueKeysWithValues: response.folders.map { ($0.id, $0.meta) })
        let dMeta = Dictionary(uniqueKeysWithValues: response.files.map { ($0.id, $0.meta) })
        return (LibraryListing(folder: nil, folders: response.folders.map(\.folder), files: response.files.map(\.file), trashDays: listing.trashDays), fMeta, dMeta)
    }
}
