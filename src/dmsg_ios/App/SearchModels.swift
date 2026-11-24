import Foundation
@preconcurrency import DMsgCore

/// One pre-sliced text span returned by `dmsg_client::search`.
struct HighlightSpan: Decodable, Hashable, Sendable {
    let text: String
    let matched: Bool
}

/// Active search scope inside any workspace: stay in the current tab or search across all workspaces.
enum WorkspaceSearchScope: Hashable {
    case current
    case all
}

struct VaultSearchMeta: Equatable, Sendable {
    let titleSpans: [HighlightSpan]
    let subtitleSpans: [HighlightSpan]
    let matchField: String?
    let matchValue: String?
}

struct FileSearchMeta: Equatable, Sendable {
    let nameSpans: [HighlightSpan]
    let pathSpans: [HighlightSpan]
    let matchField: String?
    let matchValue: String?
}

struct FileTagCount: Decodable, Identifiable, Equatable, Sendable {
    let tag: String
    let files: UInt64
    var id: String { tag }
}

/// Flat `FolderSearchHit` from `dmsg_client::search::queries::FileSearchListing`.
struct FolderSearchHit: Decodable, Sendable {
    let id: String
    let name: String
    let parent: String?
    var path: [String]? = nil
    var trashedAt: UInt64? = nil
    var nameSpans: [HighlightSpan]? = nil
    var pathSpans: [HighlightSpan]? = nil
    var score: UInt32? = nil

    var folder: FolderItem {
        FolderItem(id: id, name: name, parent: parent, path: path, trashedAt: trashedAt)
    }

    var meta: FileSearchMeta {
        FileSearchMeta(nameSpans: nameSpans ?? [], pathSpans: pathSpans ?? [], matchField: nil, matchValue: nil)
    }
}

/// Flat `FileSearchHit` from `dmsg_client::search::queries::FileSearchListing`.
struct FileSearchHit: Decodable, Sendable {
    let id: String
    let name: String
    let mime: String
    let size: UInt64
    let current: String
    let sha256: String
    let status: String
    let versions: Int
    var local: Bool? = nil
    var folder: String? = nil
    var path: [String]? = nil
    var createdAt: UInt64? = nil
    var updatedAt: UInt64? = nil
    var trashedAt: UInt64? = nil
    var backup: FileBackupReceipt? = nil
    var nameSpans: [HighlightSpan]? = nil
    var pathSpans: [HighlightSpan]? = nil
    var matchField: String? = nil
    var matchValue: String? = nil
    var score: UInt32? = nil

    var file: LibraryFile {
        LibraryFile(id: id, name: name, mime: mime, size: size, current: current, sha256: sha256,
                    status: status, versions: versions, local: local, folder: folder, path: path,
                    createdAt: createdAt, updatedAt: updatedAt, trashedAt: trashedAt, backup: backup)
    }

    var meta: FileSearchMeta {
        FileSearchMeta(nameSpans: nameSpans ?? [], pathSpans: pathSpans ?? [], matchField: matchField, matchValue: matchValue)
    }
}

struct FileSearchListingResponse: Decodable, Sendable {
    let folders: [FolderSearchHit]
    let files: [FileSearchHit]
}

/// Flat `ChannelSearchHit` from `channel_list_search`.
struct ChannelDirectoryHit: Decodable, Identifiable, Sendable {
    let id: String
    let name: String
    var unread: UInt64? = nil
    var muted: Bool? = nil
    var nameSpans: [HighlightSpan]? = nil
    var matchField: String? = nil
    var matchValue: String? = nil
    var score: UInt32? = nil
}

struct MessageHitFile: Decodable, Sendable {
    let name: String
}

struct MessageHitBody: Decodable, Sendable {
    let seq: UInt64
    var messageId: String? = nil
    let account: String
    var text: String? = nil
    var createdAt: UInt64? = nil
    var storedAt: UInt64? = nil
    var file: MessageHitFile? = nil
    var fileDeleted: Bool? = nil

    var timestampMs: UInt64 { storedAt ?? createdAt ?? 0 }
}

struct RichMessageSearchHit: Decodable, Identifiable, Sendable {
    let channel: String
    let message: MessageHitBody
    var senderName: String? = nil
    var snippetSpans: [HighlightSpan]? = nil
    var fileSpans: [HighlightSpan]? = nil
    var score: UInt32? = nil

    var id: String { "\(channel):\(message.seq)" }
    var sequence: UInt64 { message.seq }
    var text: String { message.text ?? "" }
    var attachmentName: String? { (message.fileDeleted == true) ? nil : message.file?.name }
    var preview: (text: String, spans: [HighlightSpan]?) {
        if let attachmentName, text.isEmpty || (fileSpans?.contains(where: \.matched) == true && snippetSpans?.contains(where: \.matched) != true) {
            return (attachmentName, fileSpans)
        }
        return (text, snippetSpans)
    }
}

struct RichChannelMessagePage: Decodable, Sendable {
    let hits: [RichMessageSearchHit]
    let hasMore: Bool
    let nextBefore: UInt64?
}

struct RichChannelSearchGroup: Decodable, Identifiable, Sendable {
    let channelId: String
    let channelName: String
    var hits: [RichMessageSearchHit]
    var nextBefore: UInt64?
    var topScore: UInt32? = nil

    var id: String { channelId }
}

struct LetterSearchHit: Decodable, Identifiable, Sendable {
    let id: String
    let boxKind: String
    let account: String
    var displayName: String? = nil
    var nameSpans: [HighlightSpan]? = nil
    var snippetSpans: [HighlightSpan]? = nil
    let createdAt: UInt64
    let read: Bool
    let archived: Bool
    let state: String
    var score: UInt32? = nil
}

struct GlobalChannelHit: Decodable, Identifiable, Sendable {
    let id: String
    let name: String
    var nameSpans: [HighlightSpan]? = nil
    var unread: UInt64? = nil
    var direct: Bool? = nil
    var matchField: String? = nil
    var matchValue: String? = nil
    var score: UInt32? = nil
}

struct GlobalFileHit: Decodable, Identifiable, Sendable {
    let id: String
    let kind: String
    let folder: String
    let name: String
    var nameSpans: [HighlightSpan]? = nil
    var path: [String]? = nil
    var pathSpans: [HighlightSpan]? = nil
    var mime: String? = nil
    var size: UInt64? = nil
    var status: String? = nil
    var matchField: String? = nil
    var matchValue: String? = nil
    var score: UInt32? = nil

    var isFolder: Bool { kind == "folder" }
}

struct GlobalSearchResponse: Decodable, Sendable {
    let query: String
    let vault: [VaultSummary]
    let channels: [GlobalChannelHit]
    let messages: [RichMessageSearchHit]
    let letters: [LetterSearchHit]
    let files: [GlobalFileHit]

    var folders: [GlobalFileHit] { files.filter(\.isFolder) }
    var regularFiles: [GlobalFileHit] { files.filter { !$0.isFolder } }

    var isEmpty: Bool {
        vault.isEmpty && channels.isEmpty && messages.isEmpty && letters.isEmpty && files.isEmpty
    }
}

extension SessionModel {
    func searchGlobal(query: String, scopes: [String] = [], limit: UInt32 = 10) async throws -> GlobalSearchResponse {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return GlobalSearchResponse(query: "", vault: [], channels: [], messages: [], letters: [], files: [])
        }
        #if DEBUG
        if uiTesting {
            let vaultMatches = fixtureVaultItems
                .filter { !$0.deleted && ($0.title.localizedCaseInsensitiveContains(trimmed) || $0.subtitle.localizedCaseInsensitiveContains(trimmed)) }
                .prefix(Int(limit))
                .map { item in
                    VaultSummary(id: item.id, revision: item.revision, kind: item.kind, title: item.title, subtitle: item.subtitle,
                                 favorite: item.favorite, tags: item.tags, synced: item.synced, saveState: item.saveState,
                                 conflict: item.conflict, reserved: item.reserved, deleted: item.deleted,
                                 createdAt: item.createdAt, updatedAt: item.updatedAt)
                }
            let fileMatches = fixtureFiles
                .filter { $0.trashedAt == nil && $0.name.localizedCaseInsensitiveContains(trimmed) }
                .prefix(Int(limit))
                .map { GlobalFileHit(id: $0.id, kind: "file", folder: $0.folder ?? "root", name: $0.name, path: $0.path, mime: $0.mime, size: $0.size, status: $0.status) }
            let folderMatches = fixtureFileFolders
                .filter { $0.trashedAt == nil && $0.name.localizedCaseInsensitiveContains(trimmed) }
                .prefix(Int(limit))
                .map { GlobalFileHit(id: $0.id, kind: "folder", folder: $0.parent ?? "root", name: $0.name, path: $0.path, status: "folder") }
            return GlobalSearchResponse(query: trimmed, vault: Array(vaultMatches), channels: [], messages: [], letters: [], files: folderMatches + fileMatches)
        }
        #endif
        guard let engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        let snapshot = try await engine.globalSearch(query: trimmed, scopes: scopes, limit: limit)
        return try await decodeInBackground(snapshot)
    }

    func loadFileTags() async {
        #if DEBUG
        if uiTesting { return }
        #endif
        guard unlocked, let engine else { return }
        let stamp = epoch
        do {
            let tags: [FileTagCount] = try await decodeInBackground(try await engine.fileTags())
            if stamp == epoch, !Task.isCancelled { fileTags = tags }
        } catch { /* Keep previous tag list on transient error. */ }
    }

    func openGlobalVaultHit(_ id: String) {
        selectedTab = .vault
        route.vaultItem = id
    }

    func openGlobalChannelHit(_ channelID: String, sequence: UInt64? = nil) {
        guard !localAccount else { return }
        selectedTab = .messages
        route.letterDraft = nil
        route.messagesMode = 0
        if let sequence { route.message = "hit:\(channelID):\(sequence)" }
        else { route.message = channelID }
    }

    func openGlobalLetterHit(_ hit: LetterSearchHit) {
        guard !localAccount else { return }
        selectedTab = .messages
        route.messagesMode = 1
        route.letterDraft = hit.boxKind == "draft" ? hit.id : nil
        if hit.boxKind == "sent" || hit.boxKind == "pending" {
            route.message = "sent:" + hit.id
        } else if hit.boxKind == "inbox" || hit.boxKind == "archive" {
            route.message = "received:" + hit.id
        } else {
            route.message = nil
        }
    }

    func openGlobalFileHit(_ hit: GlobalFileHit, compact: Bool) {
        selectedTab = .files
        filesScope = .local
        if hit.isFolder {
            route.file = nil
            route.filesPath = [.folder(hit.id)]
            revealLocalFileListing(in: hit.id)
        } else if compact {
            route.filesPath = [.file(hit.id)]
        } else {
            route.file = hit.id
        }
    }
}
