import SwiftUI
import PhotosUI
import UniformTypeIdentifiers
@preconcurrency import DMsgCore

struct AttachmentPicker: View {
    let folder: String
    var use: LibraryImportUse = .attachment
    var attached: Set<String> = []
    var cancel: (() -> Void)? = nil
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
    @State private var loadedQuery: String?
    @State private var visible = true
    private var libraryQuery: String { use == .library ? query : "" }
    private var files: [LibraryFile] {
        guard loadedQuery == libraryQuery else { return [] }
        return use == .library || query.isEmpty ? listing.files : listing.files.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }
    private var folders: [FolderItem] {
        guard loadedQuery == libraryQuery else { return [] }
        return use == .library || query.isEmpty ? listing.folders : listing.folders.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }
    var body: some View {
        List {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            TransferStatusView()
            Section {
                Button("Import file", systemImage: "doc.badge.plus") { importing = true }
                PhotosPicker(selection: $photo, matching: .images) { Label("Import photo", systemImage: "photo") }
            }.disabled(working)
            ForEach(folders) { folder in
                NavigationLink { AttachmentPicker(folder: folder.id, use: use, attached: attached, cancel: cancel, choose: choose) } label: {
                    if libraryQuery.isEmpty { Text(folder.name) }
                    else { FileSummaryRow(name: folder.name, mime: "inode/directory", subtitle: fileFolderPath(folder.path)) }
                }
            }
            ForEach(files) { file in
                Button { choose(file) } label: {
                    HStack {
                        FileRow(file: file, showsLocation: !libraryQuery.isEmpty)
                        if attached.contains(file.id) { Image(systemName: "checkmark").accessibilityHidden(true) }
                    }
                }.disabled(working || attached.contains(file.id) || use == .attachment && file.size > 100 * 1024 * 1024)
                    .accessibilityAddTraits(attached.contains(file.id) ? .isSelected : [])
                    .accessibilityIdentifier("attachment.choose.\(file.id)")
            }
            if !loading && !failed && loadedQuery == libraryQuery && files.isEmpty && folders.isEmpty {
                if query.isEmpty { Text("Import a file or photo to attach it.").foregroundStyle(.secondary) }
                else { ContentUnavailableView.search(text: query) }
            }
        }.privateScreen().navigationTitle(listing.folder?.name ?? String(localized: "Choose an attachment")).searchable(text: $query, prompt: "Search files")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") {
                if working { model.transfer.cancel() }
                if let cancel { cancel() } else { dismiss() }
            } } }
            .onAppear { visible = true }
            .onDisappear { visible = false }
            .task(id: libraryQuery) { await load() }
            .fileImporter(isPresented: $importing, allowedContentTypes: [.item]) { result in Task {
                do { await importFile(try result.get()) } catch { if (error as NSError).code != NSUserCancelledError { model.reportFileError(error) } }
            } }
            .onChange(of: photo) { _, item in Task {
                let epoch = model.epoch
                do {
                    guard let item, let imported = try await item.loadTransferable(type: PhotoImport.self) else { return }
                    defer { imported.finish() }
                    if epoch == model.epoch { await importFile(imported.file) }
                } catch { if epoch == model.epoch { model.reportFileError(error) } }
            } }
    }
    private func load() async {
        let epoch = model.epoch, term = libraryQuery
        loading = true; failed = false
        defer { if epoch == model.epoch, term == libraryQuery, !Task.isCancelled { loading = false } }
        do {
            if !term.isEmpty { try await Task.sleep(for: .milliseconds(180)) }
            let value: LibraryListing
            #if DEBUG
            if model.uiTesting { value = try await model.fileListingFixture(folder: folder, query: term) }
            else { value = try await model.readFileListing(folder: folder, query: term) }
            #else
            value = try await model.readFileListing(folder: folder, query: term)
            #endif
            guard epoch == model.epoch, term == libraryQuery, !Task.isCancelled else { return }
            listing = value; loadedQuery = term
        } catch { if epoch == model.epoch, term == libraryQuery, !Task.isCancelled { failed = true } }
    }
    private func importFile(_ url: URL) async {
        guard !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            let file = try await model.importLibraryDocument(url, folder: folder, use: use)
            guard use != .attachment || file.size <= 100 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
            if epoch == model.epoch, visible, !Task.isCancelled { choose(file) }
        }
        catch { if epoch == model.epoch, visible, !Task.isCancelled { model.reportFileError(error) } }
    }
}

extension SessionModel {
    func importLibraryDocument(_ url: URL, folder: String, use: LibraryImportUse = .library) async throws -> LibraryFile {
        #if DEBUG
        if uiTesting {
            if use != .library {
                let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize
                try use.validate(url, size: size ?? Int.max)
            }
            return importFileFixture(url, folder: folder)
        }
        #endif
        guard let paths = files, let engine, let account = session?.account else { throw CancellationError() }
        let epoch = epoch
        let result = try await transfer.run(url.lastPathComponent) { operation in
            transfer.phase = String(localized: "Preparing and copying file…")
            let staged = try await Task.detached { try paths.importDocument(url, account: account, folder: folder, use: use) }.value
            guard epoch == self.epoch, !Task.isCancelled else {
                if !FileManager.default.fileExists(atPath: paths.workspace.appending(path: "accounts").appending(path: account).path) { try? paths.finishImport(staged) }
                throw CancellationError()
            }
            transfer.phase = String(localized: "Encrypting on this device…")
            let handle = account + "/" + staged.deletingLastPathComponent().lastPathComponent + "/" + staged.lastPathComponent
            let result = try await engine.importFile(name: handle, folder: folder, sharedId: nil, operation: operation)
            try paths.finishImport(staged)
            return result
        }
        guard epoch == self.epoch else { throw CancellationError() }
        struct Imported: Decodable { let file: LibraryFile }
        let imported: Imported = try decode(result)
        return imported.file
    }
}
