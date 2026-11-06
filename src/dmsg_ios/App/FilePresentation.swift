import Foundation
import DMsgCore

enum FileScope: String, CaseIterable { case local, cloud }

enum FileSort: String, CaseIterable {
    case name, date, size

    var title: String {
        switch self {
        case .name: String(localized: "Name")
        case .date: String(localized: "Newest first")
        case .size: String(localized: "Largest first")
        }
    }

    func files(_ values: [LibraryFile]) -> [LibraryFile] {
        values.sorted {
            switch self {
            case .name: break
            case .date: if ($0.updatedAt ?? 0) != ($1.updatedAt ?? 0) { return ($0.updatedAt ?? 0) > ($1.updatedAt ?? 0) }
            case .size: if $0.size != $1.size { return $0.size > $1.size }
            }
            let order = $0.name.localizedStandardCompare($1.name)
            return order == .orderedSame ? $0.id < $1.id : order == .orderedAscending
        }
    }

    func backups(_ values: [CloudBackup], query: String) -> [CloudBackup] {
        let query = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return values.filter { query.isEmpty || $0.displayName.localizedCaseInsensitiveContains(query)
            || ($0.path ?? []).joined(separator: "/").localizedCaseInsensitiveContains(query) }.sorted {
            switch self {
            case .name: break
            case .date: if $0.date != $1.date { return $0.date > $1.date }
            case .size: if $0.size != $1.size { return ($0.size ?? 0) > ($1.size ?? 0) }
            }
            let order = $0.displayName.localizedStandardCompare($1.displayName)
            if order != .orderedSame { return order == .orderedAscending }
            return $0.date != $1.date ? $0.date > $1.date : $0.id < $1.id
        }
    }
}

struct BackupAdmission: Decodable {
    struct Started: Decodable { let uploadId: String }
    struct Skipped: Decodable { let reason: String }
    let started: [Started]
    let skipped: [Skipped]
    var accepted: Bool { !started.isEmpty || ["backed_up", "in_progress"].contains(skipped.first?.reason ?? "") }

    var message: String {
        if !started.isEmpty { return String(localized: "Transfer queued. Follow its progress in Files.") }
        switch skipped.first?.reason {
        case "backed_up": return String(localized: "This version is already backed up.")
        case "in_progress": return String(localized: "This version already has a transfer in progress.")
        case "too_large": return String(localized: "This file exceeds your current cloud backup file limit.")
        case "not_local": return String(localized: "Restore this version to this device before backing it up.")
        default: return String(localized: "No transfer was started.")
        }
    }
}

struct FileBackupCapacity: Decodable {
    let backupMaxFileBytes: UInt64?
    let storageLimit: UInt64?
    let committed: UInt64
    let reserved: UInt64
    /// Active file request links the plan allows; absent while the entitlement is unknown.
    var fileRequestLimit: UInt64? = nil

    var remaining: UInt64? {
        storageLimit.map { $0 > committed && $0 - committed > reserved ? $0 - committed - reserved : 0 }
    }
}

func fileSize(_ bytes: UInt64) -> String {
    ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
}

/// A plan's limit in the unit it is set in: 100 MiB reads as that, not as 104.9 MB.
func fileLimit(_ bytes: UInt64) -> String {
    let mib: UInt64 = 1024 * 1024, gib = 1024 * mib
    return bytes >= gib && bytes % gib == 0 ? "\(bytes / gib) GiB" : "\(bytes / mib) MiB"
}

func fileDate(_ milliseconds: UInt64) -> Date { Date(timeIntervalSince1970: Double(milliseconds) / 1000) }

func fileSymbol(_ mime: String) -> String {
    if mime.hasPrefix("image/") { return "photo" }
    if mime.hasPrefix("video/") { return "film" }
    if mime.hasPrefix("audio/") { return "waveform" }
    if mime == "application/pdf" { return "doc.richtext" }
    if mime.hasPrefix("text/") { return "doc.text" }
    if mime.contains("zip") || mime.contains("compressed") { return "doc.zipper" }
    return "doc"
}

/// Types worth opening straight into a preview. Anything else lands on its details.
func filePreviewable(_ mime: String) -> Bool {
    ["image/", "video/", "audio/", "text/"].contains { mime.hasPrefix($0) } || mime == "application/pdf"
        || mime.contains("officedocument") || mime.contains("msword") || mime.contains("ms-excel") || mime.contains("ms-powerpoint")
}

func fileFailure(_ error: Error) -> String {
    if let error = error as? CocoaError {
        if error.code == .fileWriteOutOfSpace { return String(localized: "Not enough storage on this device.") }
        if error.code == .fileReadTooLarge { return String(localized: "This file exceeds the size limit for this operation.") }
    }
    return fileFailureCode((error as? MobileError)?.fileCode)
}

extension MobileError {
    var fileCode: String? { if case .Failure(let code) = self { return code }; return nil }
}

func fileFailureCode(_ code: String?) -> String {
    switch code {
    case "QUOTA_EXCEEDED": String(localized: "Cloud storage or the file size limit is exceeded. Review your plan and backup size.")
    case "MEMBERSHIP_STALE": String(localized: "Your current entitlement could not be refreshed. Retry when the verification service is available.")
    case "INTEGRITY_FAILED": String(localized: "The file could not be verified. Retry the download; an incomplete copy cannot be opened.")
    case "RECOVERY_INCOMPLETE": String(localized: "The required content key is not available yet.")
    case "IDEMPOTENCY_CONFLICT": String(localized: "A file or folder with this name already exists.")
    case "INVALID_INPUT": String(localized: "Check the name and destination folder, then try again.")
    case "VERSION_CONFLICT": String(localized: "The record changed. Refresh and review your edits before saving again.")
    case "NOT_FOUND": String(localized: "The file or folder is no longer available. Refresh the list.")
    case "NETWORK_UNAVAILABLE", "UNAVAILABLE": String(localized: "Connection unavailable. Your saved content stays on this device.")
    case "SYNC_BUSY": String(localized: "Sync is waiting for the server.")
    case "PENDING": String(localized: "This version already has a transfer in progress.")
    default: String(localized: "The operation could not be completed. Please try again.")
    }
}
