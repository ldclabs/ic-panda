#if DEBUG
import Foundation
import UIKit
import UniformTypeIdentifiers
import DMsgCore

extension SessionModel {
    func prepareFileFixtures() {
        fixtureFiles = library.files
        if ProcessInfo.processInfo.arguments.contains("--ui-files-rich") {
            let receipt = FileBackupReceipt(uploadId: "fixture-local-backup", verified: true, backedUpAt: 1_793_000_100_000)
            fixtureFileFolders = [FolderItem(id: "fixture-folder", name: "Projects", parent: "root", path: [])]
            fixtureFiles = [
                LibraryFile(id: "fixture-local", name: "Local image.png", mime: "image/png", size: 2048, current: "v2", sha256: "fixture-digest", status: "backed_up", versions: 2, local: true, folder: "root", path: [], updatedAt: 1_793_000_000_000, backup: receipt),
                LibraryFile(id: "fixture-note", name: "研究笔记与产品设计 — A very long research document with a visible extension.md", mime: "text/markdown", size: 8192, current: "v1", sha256: "fixture-note", status: "local", versions: 1, local: true, folder: "root", path: [], updatedAt: 1_793_100_000_000),
                LibraryFile(id: "fixture-video", name: "Product walkthrough.mp4", mime: "video/mp4", size: 150 * 1024 * 1024, current: "v1", sha256: "fixture-video", status: "local", versions: 1, local: true, folder: "root", path: [], updatedAt: 1_793_200_000_000),
                LibraryFile(id: "fixture-nested", name: "Project notes.txt", mime: "text/plain", size: 123, current: "v1", sha256: "fixture-text", status: "local", versions: 1, local: true, folder: "fixture-folder", path: ["Projects"], updatedAt: 1_793_100_000_000),
            ]
            var backups = cloudBackups
            backups += [CloudBackup(uploadId: "fixture-local-backup", name: "Local image.png", size: 2048, bytes: 4096, needsRoot: false, local: true,
                                    fileId: "fixture-local", versionId: "v2", path: [], mime: "image/png", committedAt: 1_793_000_100_000, device: "fixture-device"),
                        CloudBackup(uploadId: "fixture-old-backup", name: "Local image.png", size: 1024, bytes: 2048, needsRoot: false, local: false,
                                    fileId: "fixture-local", versionId: "v1", path: [], mime: "image/png", committedAt: 1_792_000_100_000, device: "older-device")]
            setFileCloudFixture(backups)
            selectedTab = .files
        }
        for file in fixtureFiles {
            fixtureFileVersions[file.id] = [LibraryVersion(id: file.current, size: file.size, sha256: file.sha256, createdAt: file.createdAt ?? 1_793_000_000_000,
                                                        current: true, local: file.canOpen, status: file.status, backup: file.backup)]
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-files-rich") {
            fixtureFileVersions["fixture-local", default: []].insert(LibraryVersion(id: "v1", size: 1024, sha256: "old-version", createdAt: 1_792_000_000_000,
                current: false, local: false, status: "cloud_only", backup: FileBackupReceipt(uploadId: "fixture-old-backup", verified: true, backedUpAt: 1_792_000_100_000)), at: 0)
        }
        updateFileSummaryFixture()
    }

    func refreshFileCloudFixture(refresh: Bool) {
        if refresh { fixtureFileCloudLoads += 1 }
        let failed = ProcessInfo.processInfo.arguments.contains("--ui-files-cloud-failure") && fixtureFileCloudLoads <= 1
        setFileCloudFixture(cloudBackups, failed: failed)
    }

    func fileListingFixture(folder: String, query: String) async throws -> LibraryListing {
        fixtureFileLoads += 1
        if ProcessInfo.processInfo.arguments.contains("--ui-files-load-failure"), fixtureFileLoads == 1 { throw MobileError.Failure(code: "UNAVAILABLE") }
        if ProcessInfo.processInfo.arguments.contains("--ui-files-slow"), !query.isEmpty { try await Task.sleep(for: .milliseconds(600)) }
        let selected = fixtureFileFolders.first { $0.id == folder }
        if folder != "root", selected == nil { throw MobileError.Failure(code: "NOT_FOUND") }
        let query = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let folders = fixtureFileFolders.filter { $0.trashedAt == nil && ($0.parent ?? "root") == folder && (query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)) }
        let files = fixtureFiles.filter { file in
            guard file.trashedAt == nil, !fixtureFileFolders.contains(where: { $0.id == file.folder && $0.trashedAt != nil }) else { return false }
            return (query.isEmpty ? (file.folder ?? "root") == folder : folder == "root" || file.folder == folder)
                && (query.isEmpty || file.name.localizedCaseInsensitiveContains(query))
        }
        updateFileSummaryFixture()
        return LibraryListing(folder: selected, folders: folders, files: files)
    }

    func fileDetailsFixture(_ id: String) throws -> LibraryDetails {
        guard let file = fixtureFiles.first(where: { $0.id == id }) else { throw MobileError.Failure(code: "NOT_FOUND") }
        return LibraryDetails(file: file, versions: fixtureFileVersions[id] ?? [])
    }

    func backupFileFixture(_ file: LibraryFile) throws -> BackupAdmission {
        if file.backup?.verified == true { return BackupAdmission(started: [], skipped: [.init(reason: "backed_up")]) }
        if file.size > 100 * 1024 * 1024 { return BackupAdmission(started: [], skipped: [.init(reason: "too_large")]) }
        if !file.canOpen { return BackupAdmission(started: [], skipped: [.init(reason: "not_local")]) }
        return BackupAdmission(started: [.init(uploadId: "fixture-upload-" + file.id)], skipped: [])
    }

    func restoreFileFixture(_ backup: CloudBackup, folder: String) throws {
        guard let id = backup.fileId, let version = backup.versionId else { throw MobileError.Failure(code: "NOT_FOUND") }
        if let index = fixtureFileVersions[id]?.firstIndex(where: { $0.id == version }) {
            fixtureFileVersions[id]?[index].local = true; fixtureFileVersions[id]?[index].status = "backed_up"
        }
        if let index = fixtureFiles.firstIndex(where: { $0.id == id }) {
            if fixtureFiles[index].current == version { fixtureFiles[index].local = true; fixtureFiles[index].status = "backed_up" }
        } else {
            let receipt = FileBackupReceipt(uploadId: backup.id, verified: true, backedUpAt: backup.date)
            let file = LibraryFile(id: id, name: backup.displayName, mime: backup.mime ?? "", size: backup.size ?? 0, current: version,
                sha256: backup.sha256 ?? "fixture", status: "backed_up", versions: 1, local: true, folder: folder, path: [], backup: receipt)
            fixtureFiles.append(file)
            fixtureFileVersions[id] = [LibraryVersion(id: version, size: file.size, sha256: file.sha256, createdAt: backup.createdAt ?? backup.date, current: true, local: true, status: "backed_up", backup: receipt)]
        }
        var backups = cloudBackups
        if let index = backups.firstIndex(where: { $0.id == backup.id }) { backups[index].local = true }
        setFileCloudFixture(backups); filesRevision &+= 1
    }

    func importFileFixture(_ url: URL, folder: String) -> LibraryFile {
        let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        let file = LibraryFile(id: "fixture-import-" + UUID().uuidString, name: url.lastPathComponent, mime: UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "",
            size: UInt64(size), current: "v1", sha256: "fixture-import", status: "local", versions: 1, local: true, folder: folder, path: [], updatedAt: 1_794_000_000_000)
        fixtureFiles.append(file)
        fixtureFileVersions[file.id] = [LibraryVersion(id: file.current, size: file.size, sha256: file.sha256, createdAt: 1_794_000_000_000, current: true, local: true, status: file.status, backup: nil)]
        updateFileSummaryFixture()
        return file
    }

    func cameraCaptureFixture() throws -> PhotoImport {
        let image = UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).image { $0.fill(CGRect(x: 0, y: 0, width: 8, height: 8)) }
        return try PhotoImport.capturedPhoto(image, at: .now)
    }

    func renameFileFixture(_ id: String, name: String) throws {
        fixtureFileSaves += 1
        if ProcessInfo.processInfo.arguments.contains("--ui-files-save-failure"), fixtureFileSaves == 1 { throw MobileError.Failure(code: "UNAVAILABLE") }
        if let index = fixtureFiles.firstIndex(where: { $0.id == id }) { fixtureFiles[index].name = name }
        else if let index = fixtureFileFolders.firstIndex(where: { $0.id == id }) { fixtureFileFolders[index].name = name }
        else { throw MobileError.Failure(code: "NOT_FOUND") }
    }

    func createFileFolderFixture(parent: String, name: String) throws {
        if fixtureFileFolders.contains(where: { $0.parent == parent && $0.name == name }) { throw MobileError.Failure(code: "IDEMPOTENCY_CONFLICT") }
        fixtureFileFolders.append(FolderItem(id: UUID().uuidString, name: name, parent: parent))
    }

    func moveFileFixture(_ id: String, folder: String) throws {
        if ProcessInfo.processInfo.arguments.contains("--ui-files-move-failure"), id == "fixture-video", !fixtureFileMoveFailed {
            fixtureFileMoveFailed = true
            throw MobileError.Failure(code: "UNAVAILABLE")
        }
        if let index = fixtureFiles.firstIndex(where: { $0.id == id }) { fixtureFiles[index].folder = folder }
        else if let index = fixtureFileFolders.firstIndex(where: { $0.id == id }) { fixtureFileFolders[index].parent = folder }
        else { throw MobileError.Failure(code: "NOT_FOUND") }
    }

    func trashFileFixture(_ id: String) throws {
        if let index = fixtureFiles.firstIndex(where: { $0.id == id }) { fixtureFiles[index].trashedAt = 1_793_000_000_000 }
        else if let index = fixtureFileFolders.firstIndex(where: { $0.id == id }) { fixtureFileFolders[index].trashedAt = 1_793_000_000_000 }
        else { throw MobileError.Failure(code: "NOT_FOUND") }
    }

    func fileTrashFixture() -> LibraryListing {
        LibraryListing(folder: nil, folders: fixtureFileFolders.filter { $0.trashedAt != nil }, files: fixtureFiles.filter { $0.trashedAt != nil }, trashDays: 30)
    }

    func restoreTrashFixture(_ id: String) throws {
        if let index = fixtureFiles.firstIndex(where: { $0.id == id }) { fixtureFiles[index].trashedAt = nil }
        else if let index = fixtureFileFolders.firstIndex(where: { $0.id == id }) { fixtureFileFolders[index].trashedAt = nil }
        else { throw MobileError.Failure(code: "NOT_FOUND") }
    }

    func evictFileFixture(_ id: String, version: String) throws {
        guard let index = fixtureFiles.firstIndex(where: { $0.id == id }), fixtureFiles[index].backup?.verified == true else { throw MobileError.Failure(code: "INVALID_INPUT") }
        if fixtureFiles[index].current == version { fixtureFiles[index].local = false; fixtureFiles[index].status = "cloud_only" }
        if let v = fixtureFileVersions[id]?.firstIndex(where: { $0.id == version }) {
            fixtureFileVersions[id]?[v].local = false; fixtureFileVersions[id]?[v].status = "cloud_only"
        }
        var backups = cloudBackups
        for index in backups.indices where backups[index].fileId == id && backups[index].versionId == version { backups[index].local = false }
        setFileCloudFixture(backups)
    }

    func createFileLinkFixture(_ target: FileLinkTarget, expiresInMs: UInt64, maxDownloads: UInt64, password: Bool) async throws -> FileLink {
        fixtureFileLinkAttempts += 1
        if ProcessInfo.processInfo.arguments.contains("--ui-files-link-slow") { try await Task.sleep(for: .seconds(2)) }
        if ProcessInfo.processInfo.arguments.contains("--ui-files-link-failure"), fixtureFileLinkAttempts == 1 { throw MobileError.Failure(code: "UNAVAILABLE") }
        let share = "fixture-link-\(fixtureFileLinkAttempts)"
        var file: String?
        if case .version(let id, _) = target.source { file = id }
        let link = FileLink(shareId: share, source: .init(type: "file_version"), file: file, name: target.name, size: target.size,
            url: "https://dmsg.net/s/\(share)" + (password ? "" : "#fixture-key"), expiresAt: 1_794_000_000_000 + expiresInMs,
            maxDownloads: maxDownloads, password: password, state: "active", downloads: 0)
        fixtureFileLinks.insert(link, at: 0)
        return link
    }

    func fileLinksFixture(refresh: Bool) throws -> [FileLink] {
        if refresh { fixtureFileLinkRefreshes += 1 }
        if refresh, ProcessInfo.processInfo.arguments.contains("--ui-files-links-failure"), fixtureFileLinkRefreshes == 1 { throw MobileError.Failure(code: "UNAVAILABLE") }
        if fixtureFileLinks.isEmpty, ProcessInfo.processInfo.arguments.contains("--ui-files-links") {
            fixtureFileLinks = [
                FileLink(shareId: "fixture-link-active", source: .init(type: "file_version"), file: "fixture-local", name: "Local image.png", size: 2048,
                         url: "https://dmsg.net/s/fixture-link-active#fixture-key", expiresAt: 1_794_000_000_000, maxDownloads: 5, password: false, state: "active", downloads: 2),
                FileLink(shareId: "fixture-link-guarded", source: .init(type: "file_version"), name: "研究笔记与产品设计 — A very long research document with a visible extension.md", size: 8192,
                         url: "https://dmsg.net/s/fixture-link-guarded", expiresAt: 1_793_000_000_000, maxDownloads: 0, password: true, state: "expired", downloads: 11),
                FileLink(shareId: "fixture-link-other-device", expiresAt: 1_794_000_000_000, maxDownloads: 0, password: false, state: "active", downloads: 0),
            ]
        }
        return fixtureFileLinks
    }

    func revokeFileLinkFixture(_ id: String) throws -> FileLink {
        guard let index = fixtureFileLinks.firstIndex(where: { $0.id == id }) else { throw MobileError.Failure(code: "NOT_FOUND") }
        fixtureFileLinks[index].state = "revoked"
        return fixtureFileLinks[index]
    }

    func deleteFileCloudFixture(_ id: String) {
        setFileCloudFixture(cloudBackups.filter { $0.id != id })
        for index in fixtureFiles.indices where fixtureFiles[index].backup?.uploadId == id {
            fixtureFiles[index].backup = nil; fixtureFiles[index].status = "local"
        }
    }

    private func updateFileSummaryFixture() {
        let files = fixtureFiles.filter { $0.trashedAt == nil && $0.canOpen }
        let unbacked = files.filter { $0.backup == nil }
        filesLocalOnly = LocalOnly(versions: files.count, bytes: Int64(files.reduce(0) { $0 + $1.size }),
                                  unbackedVersions: unbacked.count, unbackedBytes: Int64(unbacked.reduce(0) { $0 + $1.size }), pendingSync: 0, lastSyncOkAt: nil)
    }
}
#endif
