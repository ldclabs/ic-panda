import Foundation

struct PrivateFiles: Sendable {
    let root: URL
    let workspace: URL
    let staging: URL
    let installation: String

    init(base: URL? = nil, environment: String = "production") throws {
        guard ["production","staging"].contains(environment) else { throw CocoaError(.fileWriteInvalidFileName) }
        let manager = FileManager.default
        root = try base ?? manager.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                      appropriateFor: nil, create: true).appending(path: environment == "production" ? "dMsg" : "dMsg-staging")
        workspace = root.appending(path: environment)
        staging = root.appending(path: "imports")
        for directory in [root, workspace, staging] {
            try manager.createDirectory(at: directory, withIntermediateDirectories: true,
                attributes: [.protectionKey: FileProtectionType.complete])
            var directory = directory
            var values = URLResourceValues(); values.isExcludedFromBackup = true
            try directory.setResourceValues(values)
        }
        // Previews are read-only app-owned copies; never preserve them across
        // a process restart. Import staging is managed separately below.
        for namespace in ["accounts", "logins"] {
            let parent = workspace.appending(path: namespace)
            for account in (try? manager.contentsOfDirectory(at: parent, includingPropertiesForKeys: [.isSymbolicLinkKey])) ?? [] {
                guard try account.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink != true else { continue }
                let copies = account.appending(path: "open")
                if manager.fileExists(atPath: copies.path) { try manager.removeItem(at: copies) }
            }
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
        let metadata = directory.appendingPathExtension("json")
        if FileManager.default.fileExists(atPath: metadata.path) { try FileManager.default.removeItem(at: metadata) }
        if try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty {
            try FileManager.default.removeItem(at: directory)
        }
    }

    func importDocument(_ url: URL, account: String, folder: String = "root", purpose: String = "file", sharedId: String? = nil) throws -> URL {
        guard account.count == 20, account.utf8.allSatisfy({ (48...57).contains($0) || (97...118).contains($0) }) else {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        let allowed = url.startAccessingSecurityScopedResource()
        defer { if allowed { url.stopAccessingSecurityScopedResource() } }
        let directory = staging.appending(path: account).appending(path: "dmsg-import-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                               attributes: [.protectionKey: FileProtectionType.complete])
        let destination = directory.appending(path: url.lastPathComponent)
        var record = PendingImport(id: directory.lastPathComponent, account: account, name: url.lastPathComponent,
            folder: folder, purpose: purpose, ready: false, sharedId: sharedId)
        try save(record)
        var copied = false
        defer {
            if !copied {
                try? FileManager.default.removeItem(at: directory)
                try? FileManager.default.removeItem(at: directory.appendingPathExtension("json"))
            }
        }
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
        record.ready = true; try save(record); copied = true
        return destination
    }
    struct PendingImport: Codable, Identifiable, Sendable {
        let id: String; let account: String; let name: String; let folder: String; let purpose: String; var ready: Bool; var sharedId: String? = nil
        var handle: String { account + "/" + id + "/" + name }
    }

    private func save(_ record: PendingImport) throws {
        let file = staging.appending(path: record.account).appending(path: record.id).appendingPathExtension("json")
        try JSONEncoder().encode(record).write(to: file, options: [.atomic, .completeFileProtection])
    }

    func pending(account: String) throws -> [PendingImport] {
        let directory = staging.appending(path: account)
        guard FileManager.default.fileExists(atPath: directory.path) else { return [] }
        return try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
            .filter { $0.lastPathComponent.hasPrefix("dmsg-import-") && $0.pathExtension == "json" }
            .map { try JSONDecoder().decode(PendingImport.self, from: Data(contentsOf: $0)) }
            .filter { $0.account == account && $0.id.hasPrefix("dmsg-import-") && !$0.id.contains("/") && !$0.name.contains("/") }
            .sorted { $0.id < $1.id }
    }

    func discard(_ record: PendingImport) throws {
        guard try pending(account: record.account).contains(where: { $0.id == record.id }) else { throw CocoaError(.fileReadNoSuchFile) }
        let directory = staging.appending(path: record.account).appending(path: record.id)
        if FileManager.default.fileExists(atPath: directory.path) { try FileManager.default.removeItem(at: directory) }
        try FileManager.default.removeItem(at: directory.appendingPathExtension("json"))
    }

}
