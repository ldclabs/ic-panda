import Foundation

struct PrivateFiles: Sendable {
    let root: URL
    let workspace: URL
    let staging: URL
    let exports: URL
    let installation: String

    init(base: URL? = nil) throws {
        let manager = FileManager.default
        root = try base ?? manager.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                      appropriateFor: nil, create: true).appending(path: "dMsg")
        workspace = root.appending(path: "production")
        staging = root.appending(path: "imports")
        exports = root.appending(path: "exports")
        for directory in [root, workspace, staging, exports] {
            try manager.createDirectory(at: directory, withIntermediateDirectories: true,
                attributes: [.protectionKey: FileProtectionType.complete])
            var directory = directory
            var values = URLResourceValues(); values.isExcludedFromBackup = true
            try directory.setResourceValues(values)
        }
        let marker = root.appending(path: "installation")
        if let value = try? String(contentsOf: marker, encoding: .utf8), UUID(uuidString: value) != nil {
            installation = value
        } else {
            installation = UUID().uuidString
            try Data(installation.utf8).write(to: marker, options: [.atomic, .completeFileProtection])
        }
    }

    /// Only the caller that completed an import may remove its exact temporary
    /// copy. Interrupted imports remain available to retry after restart.
    func finishImport(_ file: URL) throws {
        let directory = file.deletingLastPathComponent()
        guard directory.deletingLastPathComponent().deletingLastPathComponent().standardizedFileURL == staging.standardizedFileURL,
              directory.lastPathComponent.hasPrefix("dmsg-import-") else { throw CocoaError(.fileWriteInvalidFileName) }
        try FileManager.default.removeItem(at: file)
        if try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty {
            try FileManager.default.removeItem(at: directory)
        }
    }

    func importDocument(_ url: URL, account: String) throws -> URL {
        guard account.count == 20, account.utf8.allSatisfy({ (48...57).contains($0) || (97...118).contains($0) }) else {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        let allowed = url.startAccessingSecurityScopedResource()
        defer { if allowed { url.stopAccessingSecurityScopedResource() } }
        let directory = staging.appending(path: account).appending(path: "dmsg-import-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                               attributes: [.protectionKey: FileProtectionType.complete])
        let destination = directory.appending(path: url.lastPathComponent)
        var coordinationError: NSError?
        var copyError: Error?
        NSFileCoordinator().coordinate(readingItemAt: url, options: .withoutChanges, error: &coordinationError) { source in
            do {
                let values = try source.resourceValues(forKeys: [.fileSizeKey])
                guard let size = values.fileSize, size <= 100 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
                let available = try staging.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage
                if let available, available < Int64(size) * 3 + 8 * 1024 * 1024 { throw CocoaError(.fileWriteOutOfSpace) }
                try FileManager.default.copyItem(at: source, to: destination)
                try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: destination.path)
            } catch { copyError = error }
        }
        if let error = coordinationError ?? copyError as NSError? { throw error }
        return destination
    }
}
