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
        filesRequest = nil; filesLoading = false; filesLoaded = false; filesFailed = false; filesQuery = ""
        filesLocalOnly = nil; filesPendingImports = 0; filesSharedImports = 0; fileOperations = []
    }

    func loadFiles() async {
        guard unlocked else { return }
        let stamp = epoch, key = filesQueryKey, folder = filesFolder, query = filesSearch, request = UUID()
        filesRequest = request; filesLoading = true; filesFailed = false
        defer { if filesRequest == request { filesRequest = nil; filesLoading = false } }
        do {
            let value: LibraryListing
            #if DEBUG
            if uiTesting {
                value = try await fileListingFixture(folder: folder, query: query)
            } else {
                value = try await readFileListing(folder: folder, query: query)
            }
            #else
            value = try await readFileListing(folder: folder, query: query)
            #endif
            guard stamp == epoch, key == filesQueryKey, filesRequest == request, !Task.isCancelled else { return }
            library = value; filesLoaded = true; filesQuery = key; filesRevision &+= 1
            await loadFileSummary()
        } catch {
            guard stamp == epoch, key == filesQueryKey, filesRequest == request, !Task.isCancelled else { return }
            filesFailed = true
        }
    }

    private func readFileListing(folder: String, query: String) async throws -> LibraryListing {
        guard let engine else { throw CancellationError() }
        _ = try await engine.maintainFiles()
        let listing: LibraryListing = try decode(try await engine.files(folder: folder))
        let query = query.trimmingCharacters(in: .whitespacesAndNewlines)
        if query.isEmpty { return listing }
        let matches: [LibraryFile] = try decode(try await engine.searchFiles(query: query, folder: folder == "root" ? nil : folder))
        return LibraryListing(folder: listing.folder,
                              folders: listing.folders.filter { $0.name.localizedCaseInsensitiveContains(query) }, files: matches)
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
            let shared = (try? IncomingShares().list().filter { $0.accountHash == nil || $0.accountHash == IncomingShares.accountHash(account) }.count) ?? 0
            return (pending, shared)
        }.value
        guard stamp == epoch, !Task.isCancelled else { return }
        filesPendingImports = counts.0; filesSharedImports = counts.1
    }

    func fileDetails(_ id: String) async throws -> LibraryDetails {
        #if DEBUG
        if uiTesting { return try fileDetailsFixture(id) }
        #endif
        guard let engine else { throw CancellationError() }
        return try decode(try await engine.fileDetails(id: id))
    }

    func queueFileBackup(_ file: LibraryFile) async throws -> BackupAdmission {
        let operation = "backup:\(file.id):\(file.current)", stamp = epoch
        guard fileOperations.insert(operation).inserted else { return BackupAdmission(started: [], skipped: [.init(reason: "in_progress")]) }
        defer { if stamp == epoch { fileOperations.remove(operation) } }
        #if DEBUG
        if uiTesting { return try backupFileFixture(file) }
        #endif
        guard let engine else { throw CancellationError() }
        let result: BackupAdmission = try decode(try await engine.backupFile(id: file.id, expectedVersion: file.current))
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
