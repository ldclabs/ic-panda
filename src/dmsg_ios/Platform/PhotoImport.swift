import CoreTransferable
import Foundation
import UniformTypeIdentifiers

/// Photos transfers a file while its provider URL is valid. The app-owned copy
/// is protected before any bytes are written, and removed after staging.
struct PhotoImport: Transferable, Sendable {
    let file: URL
    let directory: URL
    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .image) { received in
            let directory = FileManager.default.temporaryDirectory.appending(path: "dmsg-photo-" + UUID().uuidString)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false,
                                                   attributes: [.protectionKey: FileProtectionType.complete])
            var copied = false
            defer { if !copied { try? FileManager.default.removeItem(at: directory) } }
            let file = directory.appending(path: received.file.lastPathComponent)
            let size = try received.file.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max
            guard size <= 100 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
            try FileManager.default.copyItem(at: received.file, to: file)
            copied = true
            return PhotoImport(file: file, directory: directory)
        }
    }
    func finish() {
        // Only this transfer's exact app-created copy, never the Photos source.
        try? FileManager.default.removeItem(at: file)
        try? FileManager.default.removeItem(at: directory)
    }
}
