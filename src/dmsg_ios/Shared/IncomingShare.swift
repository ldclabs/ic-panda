import CryptoKit
import Foundation

struct IncomingShare: Codable, Identifiable, Sendable {
    let id: String
    let name: String
    let size: Int64
    let sha256: String
    var accountHash: String?
}

struct IncomingShares: Sendable {
    static let group = "group.net.dmsg.ios.incoming"
    let root: URL
    init(root: URL? = nil) throws {
        guard let base = root ?? FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: Self.group) else { throw CocoaError(.fileReadNoPermission) }
        self.root = base.appending(path: "incoming")
        try FileManager.default.createDirectory(at: self.root, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.complete])
        var url = self.root; var flags = URLResourceValues(); flags.isExcludedFromBackup = true; try url.setResourceValues(flags)
    }
    static func accountHash(_ account: String) -> String { SHA256.hash(data: Data(account.utf8)).map { String(format: "%02x", $0) }.joined() }
    static func digest(_ file: URL) throws -> String {
        let input = try FileHandle(forReadingFrom: file); defer { try? input.close() }
        var hash = SHA256()
        while let bytes = try input.read(upToCount: 1024 * 1024), !bytes.isEmpty { hash.update(data: bytes) }
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }
    func file(_ record: IncomingShare) throws -> URL {
        guard UUID(uuidString: record.id) != nil, record.name == URL(fileURLWithPath: record.name).lastPathComponent,
              !record.name.contains("/"), !record.name.contains("\\"), record.name != ".", record.name != ".." else { throw CocoaError(.fileReadInvalidFileName) }
        let directory = root.appending(path: record.id)
        let file = directory.appending(path: record.name)
        guard directory.resolvingSymlinksInPath().standardizedFileURL.path == root.resolvingSymlinksInPath().appending(path: record.id).standardizedFileURL.path,
              file.resolvingSymlinksInPath().deletingLastPathComponent().standardizedFileURL.path == directory.resolvingSymlinksInPath().standardizedFileURL.path else { throw CocoaError(.fileReadNoPermission) }
        return file
    }
    func list() throws -> [IncomingShare] {
        try FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "json" && UUID(uuidString: $0.deletingPathExtension().lastPathComponent) != nil }
            .map { try JSONDecoder().decode(IncomingShare.self, from: Data(contentsOf: $0)) }
    }
    func write(_ record: IncomingShare) throws {
        _ = try file(record)
        try JSONEncoder().encode(record).write(to: root.appending(path: record.id + ".json"), options: [.atomic, .completeFileProtection])
    }
    func stage(_ source: URL, name: String? = nil) throws -> IncomingShare {
        let existing = try list()
        guard existing.count < 10 else { throw CocoaError(.fileWriteOutOfSpace) }
        let access = source.startAccessingSecurityScopedResource(); defer { if access { source.stopAccessingSecurityScopedResource() } }
        let values = try source.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])
        guard values.isRegularFile == true, let size = values.fileSize, size <= 100 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
        guard existing.reduce(Int64(size), { $0 + $1.size }) <= 200 * 1024 * 1024 else { throw CocoaError(.fileWriteOutOfSpace) }
        let id = UUID().uuidString
        let filename = name ?? source.lastPathComponent
        let provisional = IncomingShare(id: id, name: filename, size: Int64(size), sha256: "")
        let destination = try file(provisional), directory = destination.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.complete])
        try write(provisional)
        var done = false; defer {
            if !done {
                try? FileManager.default.removeItem(at: directory)
                try? FileManager.default.removeItem(at: root.appending(path: provisional.id + ".json"))
            }
        }
        try FileManager.default.copyItem(at: source, to: destination)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: destination.path)
        guard (try destination.resourceValues(forKeys: [.fileSizeKey]).fileSize) == size else { throw CocoaError(.fileReadCorruptFile) }
        let final = IncomingShare(id: id, name: filename, size: Int64(size), sha256: try Self.digest(destination))
        try write(final); done = true; return final
    }
    func stageText(_ text: String) throws -> IncomingShare {
        guard text.utf8.count <= 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
        let temporary = root.appending(path: UUID().uuidString + ".txt")
        defer { try? FileManager.default.removeItem(at: temporary) }
        try Data(text.utf8).write(to: temporary, options: [.completeFileProtection])
        return try stage(temporary, name: "Shared text.txt")
    }
    func remove(_ record: IncomingShare) throws {
        let directory = try file(record).deletingLastPathComponent()
        if FileManager.default.fileExists(atPath: directory.path) { try FileManager.default.removeItem(at: directory) }
        try FileManager.default.removeItem(at: root.appending(path: record.id + ".json"))
    }
}
