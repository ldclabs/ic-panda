import Foundation
@preconcurrency import DMsgCore

extension SessionModel {
    func fileSourceDevice(_ device: String) -> String {
        if device == session?.deviceId { return String(localized: "This device") }
        return deviceLabel(device) ?? (device.count > 8 ? String(device.prefix(8)) + "…" : device)
    }

    func reportFileError(_ value: Error) {
        if let message = fileFailureMessage(value) { error = message }
    }

    func fileFailureMessage(_ value: Error) -> String? {
        if value is CancellationError || Task.isCancelled || (value as? CocoaError)?.code == .userCancelled { return nil }
        if let code = (value as? MobileError)?.fileCode, ["CANCELLED", "LOCKED", "DEVICE_NOT_APPROVED"].contains(code) {
            report(value); return nil
        }
        return fileFailure(value)
    }

    func resetFilePresentation() {
        filesSearches.removeAll()
        filesFavoritesOnly = false; filesTag = ""; fileSearchMeta = [:]; folderSearchMeta = [:]
        filesRequest = nil; filesLoading = false; filesLoaded = false; filesFailed = false; filesQuery = ""
        filesLocalOnly = nil; filesPendingImports = 0; filesSharedImports = 0; fileOperations = []
    }

    func setFileSearch(_ text: String, in folder: String) {
        filesSearches[folder] = text.isEmpty ? nil : text
    }

    func revealLocalFileListing(in folder: String) {
        guard filesFolder == folder else { return }
        filesScope = .local
        filesSearch = ""
    }

    var fileImportNotice: String {
        localAccount ? String(localized: "Saved on this device") : String(localized: "Imported to this device. Cloud backup is a separate action.")
    }

    /// A quiet reload tells views about the library only if the listing changed;
    /// after a change of its own, a caller has them read everything again.
    func loadFiles(quiet: Bool = false) async {
        guard unlocked else { return }
        let stamp = epoch, key = filesQueryKey, folder = filesFolder, query = filesSearch, starred = filesFavoritesOnly, tag = filesTag, request = UUID()
        filesRequest = request; filesLoading = true; filesFailed = false
        defer { if filesRequest == request { filesRequest = nil; filesLoading = false } }
        do {
            let value: LibraryListing
            var nextFolderMeta: [String: FileSearchMeta] = [:]
            var nextFileMeta: [String: FileSearchMeta] = [:]
            #if DEBUG
            if uiTesting {
                value = try await fileListingFixture(folder: folder, query: query)
            } else {
                let loaded = try await readRichFileListing(folder: folder, query: query, starred: starred, tag: tag)
                value = loaded.listing; nextFolderMeta = loaded.folderMeta; nextFileMeta = loaded.fileMeta
            }
            #else
            let loaded = try await readRichFileListing(folder: folder, query: query, starred: starred, tag: tag)
            value = loaded.listing; nextFolderMeta = loaded.folderMeta; nextFileMeta = loaded.fileMeta
            #endif
            guard stamp == epoch, key == filesQueryKey, filesRequest == request, !Task.isCancelled else { return }
            if !quiet || library != value { filesRevision &+= 1 }
            library = value; folderSearchMeta = nextFolderMeta; fileSearchMeta = nextFileMeta
            filesLoaded = true; filesQuery = key
            if query.isEmpty && !starred && tag.isEmpty { filesListings[folder] = value }
            await loadFileSummary()
        } catch {
            guard stamp == epoch, key == filesQueryKey, filesRequest == request, !Task.isCancelled else { return }
            filesFailed = true
        }
    }

    func readFileListing(folder: String, query: String) async throws -> LibraryListing {
        try await readRichFileListing(folder: folder, query: query).listing
    }

    func readRichFileListing(folder: String, query: String, starred: Bool = false, tag: String = "") async throws -> (listing: LibraryListing, folderMeta: [String: FileSearchMeta], fileMeta: [String: FileSearchMeta]) {
        guard let engine else { throw CancellationError() }
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty && !starred && tag.isEmpty {
            let listing: LibraryListing = try await decodeInBackground(try await engine.files(folder: folder))
            return (listing, [:], [:])
        }
        let scopeFolder = folder == "root" ? nil : folder
        let response: FileSearchListingResponse = try await decodeInBackground(
            try await engine.searchFilesListing(query: trimmed, folder: scopeFolder, starred: starred, tag: tag.isEmpty ? nil : tag, trash: false)
        )
        let currentFolder = filesListings[folder]?.folder
        let folders = response.folders.map(\.folder)
        let files = response.files.map(\.file)
        let folderMeta = Dictionary(uniqueKeysWithValues: response.folders.map { ($0.id, $0.meta) })
        let fileMeta = Dictionary(uniqueKeysWithValues: response.files.map { ($0.id, $0.meta) })
        return (LibraryListing(folder: currentFolder, folders: folders, files: files), folderMeta, fileMeta)
    }

    func loadFileSummary() async {
        let stamp = epoch
        if engine != nil {
            let value = await LocalOnly.load(self)
            guard stamp == epoch, !Task.isCancelled else { return }
            if let value { filesLocalOnly = value }
        }
        guard let files, let account = session?.account else { return }
        let counts = await Task.detached {
            let pending = (try? files.pending(account: account).count) ?? 0
            let hash = IncomingShares.accountHash(account)
            let shared = (try? IncomingShares().list().count { $0.accountHash == nil || $0.accountHash == hash }) ?? 0
            return (pending, shared)
        }.value
        guard stamp == epoch, !Task.isCancelled else { return }
        filesPendingImports = counts.0; filesSharedImports = counts.1
    }

    /// Shares saved while dMsg was away announce themselves once per launch.
    func announceIncomingShares() {
        #if DEBUG
        if uiTesting { return }
        #endif
        guard unlocked, let account = session?.account, let entries = try? IncomingShares().list() else { return }
        let hash = IncomingShares.accountHash(account)
        let waiting = entries.filter { $0.accountHash == nil || $0.accountHash == hash }.map(\.id)
        guard !waiting.allSatisfy(announcedShares.contains) else { return }
        announcedShares.formUnion(waiting)
        incomingSharesArrived &+= 1
    }

    func fileDetails(_ id: String) async throws -> LibraryDetails {
        #if DEBUG
        if uiTesting { return try fileDetailsFixture(id) }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await engine.fileDetails(id: id))
    }

    func unbackedFileVersions() async throws -> [LibraryDetails] {
        #if DEBUG
        if uiTesting {
            return try fixtureFiles.filter { $0.trashedAt == nil }.compactMap { file in
                let details = try fileDetailsFixture(file.id)
                let missing = details.versions.filter { $0.local && $0.backup == nil }
                return missing.isEmpty ? nil : LibraryDetails(file: details.file, versions: missing)
            }
        }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await engine.unbackedFileVersions())
    }

    func queueFileBackup(_ file: LibraryFile, explicitRevision: Bool = false) async throws -> BackupAdmission {
        let operation = "backup:\(file.id):\(file.current)", stamp = epoch
        guard fileOperations.insert(operation).inserted else { return BackupAdmission(started: [], skipped: [.init(reason: "in_progress")]) }
        defer { if stamp == epoch { fileOperations.remove(operation) } }
        #if DEBUG
        if uiTesting { return try backupFileFixture(file) }
        #endif
        guard let engine else { throw CancellationError() }
        let snapshot = explicitRevision
            ? try await engine.backupFileRevision(id: file.id, version: file.current)
            : try await engine.backupFile(id: file.id, expectedVersion: file.current)
        let result: BackupAdmission = try decode(snapshot)
        guard stamp == epoch, !Task.isCancelled else { throw CancellationError() }
        await fileTransfers.load(self)
        await loadFiles()
        return result
    }

    func restoreCloudFile(_ backup: CloudBackup, folder: String) async throws {
        let operation = "restore:\(backup.id)", stamp = epoch
        guard fileOperations.insert(operation).inserted else { return }
        defer { if stamp == epoch { fileOperations.remove(operation) } }
        #if DEBUG
        if uiTesting { try restoreFileFixture(backup, folder: folder); return }
        #endif
        guard let engine else { throw CancellationError() }
        if let files,
           let available = try files.root.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage,
           available < Int64(clamping: backup.bytes) + 8 * 1024 * 1024 { throw CocoaError(.fileWriteOutOfSpace) }
        _ = try await coreOperation { try await engine.restoreBackup(upload: backup.id, folder: folder, operation: $0) }
        guard stamp == epoch, !Task.isCancelled else { throw CancellationError() }
        await fileTransfers.load(self)
        notice = String(localized: "Transfer queued. Follow its progress in Files.")
    }

    func createFileLink(_ target: FileLinkTarget, expiresInMs: UInt64, maxDownloads: UInt64, password: String?, showProfile: Bool) async throws -> FileLink {
        #if DEBUG
        if uiTesting { return try await createFileLinkFixture(target, expiresInMs: expiresInMs, maxDownloads: maxDownloads, password: password != nil, showProfile: showProfile) }
        #endif
        guard let engine else { throw CancellationError() }
        let result: ObjectSnapshot
        switch target.source {
        case .version(let file, let version):
            result = try await coreOperation {
                try await engine.createFileLink(id: file, version: version, expiresInMs: expiresInMs, maxDownloads: maxDownloads, password: password, showProfile: showProfile, operation: $0)
            }
        case .backup(let upload):
            result = try await coreOperation {
                try await engine.createBackupLink(upload: upload, expiresInMs: expiresInMs, maxDownloads: maxDownloads, password: password, showProfile: showProfile, operation: $0)
            }
        }
        return try decode(result)
    }

    /// Saved account links include their decrypted source when available.
    func fileLinks(refresh: Bool) async throws -> [FileLink] {
        #if DEBUG
        if uiTesting { return try fileLinksFixture(refresh: refresh) }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await coreOperation { try await engine.fileLinks(refresh: refresh, operation: $0) })
    }

    func revokeFileLink(_ id: String) async throws -> FileLink {
        #if DEBUG
        if uiTesting { return try revokeFileLinkFixture(id) }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await coreOperation { try await engine.revokeFileLink(shareId: id, operation: $0) })
    }

    func fileRequests(refresh: Bool) async throws -> [FileRequest] {
        #if DEBUG
        if uiTesting { return fileRequestsFixture() }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await coreOperation { try await engine.fileRequests(refresh: refresh, operation: $0) })
    }

    func fileRequestCapacity() async throws -> FileBackupCapacity {
        #if DEBUG
        if uiTesting { return fileRequestCapacityFixture() }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await coreOperation { try await engine.fileBackupCapacity(operation: $0) })
    }

    func createFileRequest(title: String, instructions: String, label: String, expiresInMs: UInt64, maxTotalBytes: UInt64) async throws -> FileRequest {
        #if DEBUG
        if uiTesting {
            if fixtureFileRequestAttempts == 1 && ProcessInfo.processInfo.arguments.contains("--ui-files-request-slow") {
                try await Task.sleep(for: .seconds(2))
            }
            return try createFileRequestFixture(title: title, instructions: instructions, label: label)
        }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await coreOperation {
            try await engine.createFileRequest(title: title, instructions: instructions, label: label, expiresInMs: expiresInMs, maxTotalBytes: maxTotalBytes, operation: $0)
        })
    }

    func revokeFileRequest(_ id: String) async throws -> FileRequest {
        #if DEBUG
        if uiTesting { return try revokeFileRequestFixture(id) }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await coreOperation { try await engine.revokeFileRequest(requestId: id, operation: $0) })
    }

    func fileRequestFiles(_ id: String) async throws -> [ReceivedFile] {
        #if DEBUG
        if uiTesting { return fixtureReceivedFiles }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await coreOperation { try await engine.fileRequestFiles(requestId: id, operation: $0) })
    }

    /// Downloads one received file and re-encrypts it into the library, in the foreground.
    func saveRequestFile(_ upload: String) async throws {
        #if DEBUG
        if uiTesting { try await saveRequestFileFixture(upload); return }
        #endif
        guard let engine else { throw CancellationError() }
        let root = String(localized: "Received files")
        _ = try await coreOperation { try await engine.saveRequestFile(upload: upload, folder: nil, root: root, operation: $0) }
    }

    func deleteRequestFile(_ upload: String) async throws {
        #if DEBUG
        if uiTesting { fixtureReceivedFiles.removeAll { $0.id == upload }; return }
        #endif
        guard let engine else { throw CancellationError() }
        _ = try await coreOperation { try await engine.deleteRequestFile(upload: upload, operation: $0) }
    }

    func renameLibraryItem(_ id: String, name: String) async throws {
        #if DEBUG
        if uiTesting { try renameFileFixture(id, name: name); return }
        #endif
        guard let engine else { throw CancellationError() }
        try await engine.renameFile(id: id, name: name)
    }

    func moveLibraryItem(_ id: String, folder: String) async throws {
        #if DEBUG
        if uiTesting {
            if ProcessInfo.processInfo.arguments.contains("--ui-files-slow-move") {
                try await Task.sleep(for: .seconds(5))
            }
            try moveFileFixture(id, folder: folder); return
        }
        #endif
        guard let engine else { throw CancellationError() }
        try await engine.moveFile(id: id, folder: folder)
    }

    func trashLibraryItem(_ id: String) async throws {
        #if DEBUG
        if uiTesting { try trashFileFixture(id); return }
        #endif
        guard let engine else { throw CancellationError() }
        try await engine.trashFile(id: id)
    }
}
