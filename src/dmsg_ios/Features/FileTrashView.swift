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
    var body: some View {
        List {
            if let days = listing.trashDays {
                Section {
                    LabeledContent("Days before removal", value: days.formatted())
                    Text("Cloud backups are kept. This only changes the copy on this device.").font(.footnote).foregroundStyle(.secondary)
                }
            }
            if loading { ProgressView("Loading…") }
            if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
            ForEach(listing.folders) { folder in restoreRow(folder.id, name: folder.name, mime: "inode/directory", time: folder.trashedAt) }
            ForEach(listing.files) { file in restoreRow(file.id, name: file.name, mime: file.mime, time: file.trashedAt) }
            if !loading && failure == nil && listing.files.isEmpty && listing.folders.isEmpty { ContentUnavailableView("Trash is empty", systemImage: "trash") }
        }.privateScreen().navigationTitle("Trash").task { await load() }.refreshable { await load() }
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .onAppear { visible = true }.onDisappear { visible = false }
    }

    private func restoreRow(_ id: String, name: String, mime: String, time: UInt64?) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            FileSummaryRow(name: name, mime: mime, subtitle: "")
            if let time {
                LabeledContent("Deleted") { Text(fileDate(time), format: .dateTime) }.font(.caption).foregroundStyle(.secondary)
                if let days = listing.trashDays {
                    LabeledContent("Removal date") { Text(fileDate(time).addingTimeInterval(Double(days) * 86_400), style: .date) }.font(.caption).foregroundStyle(.secondary)
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
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }; await load(); await model.loadFiles()
        } catch { if stamp == model.epoch, visible, !Task.isCancelled { errors[id] = model.fileFailureMessage(error) } }
    }

    private func restoreRemote(_ id: String) async throws {
        guard let engine = model.engine else { throw CancellationError() }
        try await engine.restoreFile(id: id)
    }

    private func load() async {
        let stamp = model.epoch; loading = true; failure = nil; defer { loading = false }
        do {
            let value: LibraryListing
            #if DEBUG
            if model.uiTesting { value = model.fileTrashFixture() }
            else { value = try await read() }
            #else
            value = try await read()
            #endif
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }; listing = value
        } catch { if stamp == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func read() async throws -> LibraryListing {
        guard let engine = model.engine else { throw CancellationError() }
        return try model.decode(try await engine.fileTrash())
    }
}
