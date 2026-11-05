import CoreTransferable
import Foundation
import UIKit
import UniformTypeIdentifiers

/// Photos transfers a file while its provider URL is valid; the camera hands
/// over a photo in memory or a movie in temporary storage. The app-owned copy
/// is protected before any bytes are written, and removed after staging.
struct PhotoImport: Transferable, Sendable {
    let file: URL
    let directory: URL
    private static let directoryPrefix = "dmsg-photo-"

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .image) { received in try copy(received.file) }
        FileRepresentation(importedContentType: .movie) { received in try copy(received.file) }
    }

    private static func copy(_ source: URL) throws -> PhotoImport {
        try store(name: source.lastPathComponent) { file, directory in
            let size = try source.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max
            guard size <= 1000 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
            if let available = try directory.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage,
               available < Int64(size) + 8 * 1024 * 1024 { throw CocoaError(.fileWriteOutOfSpace) }
            try FileManager.default.copyItem(at: source, to: file)
        }
    }

    /// JPEG keeps the library's copy viewable on every client. It carries the
    /// orientation but no location or other capture metadata.
    static func capturedPhoto(_ image: UIImage, at date: Date) throws -> PhotoImport {
        guard let data = image.jpegData(compressionQuality: 0.9) else { throw CocoaError(.fileWriteUnknown) }
        return try store(name: capturedName("IMG", "jpg", at: date)) { file, _ in
            try data.write(to: file, options: [.atomic, .completeFileProtection])
        }
    }

    /// Moved rather than copied, so the camera's unprotected recording is gone
    /// as soon as it is handed over.
    static func capturedMovie(_ source: URL, at date: Date) throws -> PhotoImport {
        try store(name: capturedName("VID", source.pathExtension.lowercased(), at: date)) { file, _ in
            try FileManager.default.moveItem(at: source, to: file)
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: file.path)
        }
    }

    private static func capturedName(_ prefix: String, _ fileExtension: String, at date: Date) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd_HHmmss"
        return prefix + "_" + formatter.string(from: date) + "." + fileExtension
    }

    private static func store(name: String, write: (_ file: URL, _ directory: URL) throws -> Void) throws -> PhotoImport {
        let directory = FileManager.default.temporaryDirectory.appending(path: directoryPrefix + UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false,
                                               attributes: [.protectionKey: FileProtectionType.complete])
        var stored = false
        defer { if !stored { try? FileManager.default.removeItem(at: directory) } }
        let file = directory.appending(path: name)
        try write(file, directory)
        stored = true
        return PhotoImport(file: file, directory: directory)
    }

    func finish() {
        // Only this transfer's exact app-created copy, never the Photos source.
        try? FileManager.default.removeItem(at: file)
        try? FileManager.default.removeItem(at: directory)
    }

    /// Copies are only in use within one app run. What an interrupted run left
    /// behind is plaintext, and whatever reached staging can still be retried.
    static func removeAbandoned() {
        let temporary = FileManager.default.temporaryDirectory
        for name in (try? FileManager.default.contentsOfDirectory(atPath: temporary.path)) ?? [] where name.hasPrefix(directoryPrefix) {
            try? FileManager.default.removeItem(at: temporary.appending(path: name))
        }
    }
}
