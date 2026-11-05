import Foundation

struct AccountProfile: Decodable {
    let name: String?
    let description: String?
    let avatarUrl: String?
    let updatedAt: Int64
    var links: [PublicProfileLink]? = nil

    var displayName: String? {
        guard let name = name?.trimmingCharacters(in: .whitespacesAndNewlines), !name.isEmpty else { return nil }
        return name
    }
}

struct LibraryListing: Decodable {
    let folder: FolderItem?
    let folders: [FolderItem]
    let files: [LibraryFile]
    var trashDays: UInt64? = nil
    static let empty = LibraryListing(folder: nil, folders: [], files: [])
}

struct CloudBackup: Decodable, Identifiable {
    let uploadId: String
    let name: String?
    let size: UInt64?
    let bytes: UInt64
    let needsRoot: Bool
    var local: Bool
    var fileId: String? = nil
    var versionId: String? = nil
    var path: [String]? = nil
    var mime: String? = nil
    var sha256: String? = nil
    var createdAt: UInt64? = nil
    var committedAt: UInt64? = nil
    var device: String? = nil
    var id: String { uploadId }

    var displayName: String { name ?? String(localized: "Encrypted backup") }
    var date: UInt64 { committedAt ?? createdAt ?? 0 }
}

/// A share link as the core reports it. Name, size, URL and source are absent
/// when this device cannot open the link's encrypted management record.
struct FileLink: Decodable, Identifiable {
    struct Source: Decodable { let type: String }
    let shareId: String
    var source: Source? = nil
    var file: String? = nil
    var name: String? = nil
    var size: UInt64? = nil
    var url: String? = nil
    let expiresAt: UInt64
    let maxDownloads: UInt64
    let password: Bool
    var state: String
    var downloads: UInt64
    var id: String { shareId }
    /// Secret snapshots are managed, and revealed, under Shared snapshots.
    var isSecretSnapshot: Bool { source?.type == "vault_snapshot" }
    var active: Bool { state == "active" }
    var stateTitle: String {
        switch state {
        case "active": String(localized: "Active")
        case "revoked": String(localized: "Revoked")
        case "expired": String(localized: "Expired")
        default: String(localized: "Download limit reached")
        }
    }
    var downloadsText: String {
        maxDownloads > 0 ? "\(downloads.formatted()) / \(maxDownloads.formatted())" : downloads.formatted()
    }
}

/// What a new link shares: a library version, or a backup that may not be in
/// this device's library. `remote` says its chunks must come down first.
struct FileLinkTarget: Identifiable {
    enum Source { case version(file: String, version: String), backup(upload: String) }
    /// The core's limit for a link.
    static let maxBytes: UInt64 = 100 * 1024 * 1024
    let source: Source
    let name: String
    let mime: String
    let size: UInt64
    let remote: Bool
    var id: String {
        switch source {
        case .version(let file, let version): "version:\(file):\(version)"
        case .backup(let upload): "backup:\(upload)"
        }
    }
    var tooLarge: Bool { size > Self.maxBytes }

    init(file: LibraryFile, remote: Bool) {
        source = .version(file: file.id, version: file.current)
        name = file.name; mime = file.mime; size = file.size; self.remote = remote
    }

    init(backup: CloudBackup) {
        source = .backup(upload: backup.id)
        name = backup.displayName; mime = backup.mime ?? ""; size = backup.size ?? 0; remote = !backup.local
    }
}

struct FolderItem: Decodable, Identifiable {
    let id: String; var name: String; var parent: String?
    var path: [String]? = nil
    var trashedAt: UInt64? = nil
}
struct LibraryFile: Decodable, Identifiable {
    let id: String; var name: String; let mime: String; let size: UInt64
    let current: String; let sha256: String; var status: String
    let versions: Int
    var local: Bool? = nil
    var folder: String? = nil
    var path: [String]? = nil
    var createdAt: UInt64? = nil
    var updatedAt: UInt64? = nil
    var trashedAt: UInt64? = nil
    var backup: FileBackupReceipt? = nil
    var canOpen: Bool { local ?? (status != "cloud_only") }
    var statusLabel: String {
        switch status {
        case "cloud_only": String(localized: "Only in cloud")
        case "backed_up": String(localized: "Backed up")
        case "backing_up": String(localized: "Backing up…")
        case "failed": String(localized: "Backup failed")
        case "too_large": String(localized: "On this device")
        default: String(localized: "On this device")
        }
    }
}

struct FileBackupReceipt: Decodable {
    let uploadId: String
    let verified: Bool
    let backedUpAt: UInt64
}

struct LibraryVersion: Decodable, Identifiable {
    let id: String; let size: UInt64; let sha256: String; let createdAt: UInt64
    let current: Bool; var local: Bool
    var device: String? = nil
    var status: String? = nil
    var backup: FileBackupReceipt? = nil

    func file(in parent: LibraryFile) -> LibraryFile {
        LibraryFile(id: parent.id, name: parent.name, mime: parent.mime, size: size, current: id,
                    sha256: sha256, status: status ?? (local ? "local" : "cloud_only"), versions: parent.versions,
                    local: local, folder: parent.folder, path: parent.path, createdAt: createdAt,
                    updatedAt: createdAt, trashedAt: parent.trashedAt, backup: backup)
    }
}

struct LibraryDetails: Decodable {
    let file: LibraryFile
    let versions: [LibraryVersion]
}

struct ApprovalView: Decodable, Identifiable {
    let id: String; let digest: String; let type: String; let state: String
    let source: String; let risk: String; let expiresAt: UInt64
    let mobileSupported: Bool?
    let updatedAt: UInt64?
}

/// Lossless JSON display for the full canonical approval payload. Integers
/// remain exact Decimal values; amounts are never rounded through Double.
enum DisplayValue: Decodable {
    case object([String: DisplayValue]), array([DisplayValue]), string(String), number(Decimal), boolean(Bool), null
    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() { self = .null }
        else if let value = try? container.decode(Bool.self) { self = .boolean(value) }
        else if let value = try? container.decode(String.self) { self = .string(value) }
        else if let value = try? container.decode([String: DisplayValue].self) { self = .object(value) }
        else if let value = try? container.decode([DisplayValue].self) { self = .array(value) }
        else { self = .number(try container.decode(Decimal.self)) }
    }
    var text: String {
        switch self {
        case .string(let value): value
        case .number(let value): NSDecimalNumber(decimal: value).stringValue
        case .boolean(let value): value ? String(localized: "Yes") : String(localized: "No")
        case .null: "—"
        case .array, .object: ""
        }
    }
}
