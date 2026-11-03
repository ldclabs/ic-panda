import CoreTransferable
import Foundation
import UniformTypeIdentifiers

/// Photos transfers a file while its provider URL is valid. The app-owned copy
/// is protected before any bytes are written, and removed after staging.
struct PhotoImport: Transferable, Sendable {
    let file: URL
    let directory: URL
    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .image) { received in try copy(received.file) }
        FileRepresentation(importedContentType: .movie) { received in try copy(received.file) }
    }

    private static func copy(_ source: URL) throws -> PhotoImport {
        let directory = FileManager.default.temporaryDirectory.appending(path: "dmsg-photo-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false,
                                               attributes: [.protectionKey: FileProtectionType.complete])
        var copied = false
        defer { if !copied { try? FileManager.default.removeItem(at: directory) } }
        let file = directory.appending(path: source.lastPathComponent)
        let size = try source.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max
        guard size <= 1000 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
        if let available = try directory.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage,
           available < Int64(size) + 8 * 1024 * 1024 { throw CocoaError(.fileWriteOutOfSpace) }
        try FileManager.default.copyItem(at: source, to: file)
        copied = true
        return PhotoImport(file: file, directory: directory)
    }

    func finish() {
        // Only this transfer's exact app-created copy, never the Photos source.
        try? FileManager.default.removeItem(at: file)
        try? FileManager.default.removeItem(at: directory)
    }
}
