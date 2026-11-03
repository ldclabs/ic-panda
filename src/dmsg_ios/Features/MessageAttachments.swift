import SwiftUI
import PhotosUI
import UniformTypeIdentifiers
@preconcurrency import DMsgCore

struct AttachmentPicker: View {
    let folder: String
    let choose: (LibraryFile) -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var listing = LibraryListing.empty
    @State private var loading = true
    @State private var failed = false
    @State private var importing = false
    @State private var photo: PhotosPickerItem?
    @State private var working = false
    @State private var query = ""
    @State private var visible = true
    private var files: [LibraryFile] { listing.files.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) } }
    var body: some View {
        List {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            TransferStatusView()
            Section {
                Button("Import file", systemImage: "doc.badge.plus") { importing = true }
                PhotosPicker(selection: $photo, matching: .images) { Label("Import photo", systemImage: "photo") }
            }.disabled(working)
            ForEach(listing.folders.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) }) { folder in
                NavigationLink(folder.name) { AttachmentPicker(folder: folder.id, choose: choose) }
            }
            ForEach(files) { file in
                Button { choose(file) } label: { FileRow(file: file) }.disabled(working || file.size > 100 * 1024 * 1024)
                    .accessibilityIdentifier("attachment.choose.\(file.id)")
            }
            if !loading && !failed && files.isEmpty && listing.folders.isEmpty { Text("Import a file or photo to attach it.").foregroundStyle(.secondary) }
        }.privateScreen().navigationTitle("Choose an attachment").searchable(text: $query, prompt: "Search files")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { if working { model.transfer.cancel() }; dismiss() } } }
            .onAppear { visible = true }
            .onDisappear { visible = false }
            .task { await load() }
            .fileImporter(isPresented: $importing, allowedContentTypes: [.item]) { result in Task {
                do { await importFile(try result.get()) } catch { if (error as NSError).code != NSUserCancelledError { model.report(error) } }
            } }
            .onChange(of: photo) { _, item in Task {
                let epoch = model.epoch
                do {
                    guard let item, let imported = try await item.loadTransferable(type: PhotoImport.self) else { return }
                    defer { imported.finish() }
                    if epoch == model.epoch { await importFile(imported.file) }
                } catch { if epoch == model.epoch { model.report(error) } }
            } }
    }
    private func load() async {
        let epoch = model.epoch; loading = true; failed = false; defer { if epoch == model.epoch { loading = false } }
        #if DEBUG
        if model.uiTesting { listing = model.library; return }
        #endif
        do {
            guard let engine = model.engine else { return }
            let value: LibraryListing = try model.decode(try await engine.files(folder: folder))
            guard epoch == model.epoch, !Task.isCancelled else { return }; listing = value
        } catch { if epoch == model.epoch, !Task.isCancelled { failed = true } }
    }
    private func importFile(_ url: URL) async {
        guard !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do { let file = try await model.importLibraryDocument(url, folder: folder); if epoch == model.epoch, visible, !Task.isCancelled { choose(file) } }
        catch { if epoch == model.epoch, visible, !Task.isCancelled { model.report(error) } }
    }
}

extension SessionModel {
    func importLibraryDocument(_ url: URL, folder: String) async throws -> LibraryFile {
        guard let paths = files, let engine, let account = session?.account else { throw CancellationError() }
        let epoch = epoch
        let staged = try await Task.detached { try paths.importDocument(url, account: account, folder: folder) }.value
        guard epoch == self.epoch else {
            if !FileManager.default.fileExists(atPath: paths.workspace.appending(path: "accounts").appending(path: account).path) { try? paths.finishImport(staged) }
            throw CancellationError()
        }
        let handle = account + "/" + staged.deletingLastPathComponent().lastPathComponent + "/" + staged.lastPathComponent
        let result = try await transfer.run(staged.lastPathComponent) { try await engine.importFile(name: handle, folder: folder, sharedId: nil, operation: $0) }
        try paths.finishImport(staged)
        guard epoch == self.epoch else { throw CancellationError() }
        struct Imported: Decodable { let file: LibraryFile }
        let imported: Imported = try decode(result)
        return imported.file
    }
}
