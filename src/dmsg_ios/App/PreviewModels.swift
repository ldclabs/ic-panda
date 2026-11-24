import Foundation
import ImageIO
import Observation
import UIKit
@preconcurrency import DMsgCore

/// Preview category mirroring `dmsg_client::preview::PreviewClass`.
enum PreviewClass: String, Decodable, Sendable, Equatable {
    case image
    case markdown
    case text
    case unsupported
}

/// Filter applied to the active fullscreen preview queue.
enum PreviewFilter: String, CaseIterable, Identifiable, Sendable {
    case all
    case previewable
    case images
    case documents

    var id: String { rawValue }

    var title: String {
        switch self {
        case .all: String(localized: "All")
        case .previewable: String(localized: "Previewable")
        case .images: String(localized: "Images")
        case .documents: String(localized: "Documents")
        }
    }
}

/// Reason a file rendered its metadata fallback card instead of content.
enum PreviewFallbackReason: String, Decodable, Sendable, Equatable {
    case unsupportedType = "unsupported_type"
    case tooLarge = "too_large"
    case cloudOnly = "cloud_only"
    case decodeFailed = "decode_failed"
    case unavailable = "unavailable"

    var message: String {
        switch self {
        case .unsupportedType: String(localized: "Preview is not supported for this file type.")
        case .tooLarge: String(localized: "This file is too large to preview in app.")
        case .cloudOnly: String(localized: "This file is stored in the cloud only. Download it first to preview.")
        case .decodeFailed: String(localized: "Could not decode this file for preview.")
        case .unavailable: String(localized: "This file is no longer available.")
        }
    }
}

/// Lightweight channel attachment entry returned by `MobileEngine.channelAttachments`.
struct ChannelAttachmentRecord: Decodable, Sendable, Equatable {
    let seq: UInt64
    let digest: String
    let account: String
    let createdAt: UInt64
    let storedAt: UInt64
    let name: String
    let mime: String
    let size: UInt64
    let previewClass: PreviewClass
    let previewable: Bool
}

// MARK: - Safe Markdown AST (produced by Rust `dmsg_client::preview::parse_markdown`)

enum MarkdownTableAlignment: String, Decodable, Sendable, Equatable {
    case none
    case left
    case center
    case right
}

struct MarkdownDocument: Decodable, Sendable, Equatable {
    let blocks: [MarkdownBlock]
    let truncated: Bool

    private enum CodingKeys: String, CodingKey {
        case blocks
        case truncated
    }

    init(blocks: [MarkdownBlock], truncated: Bool = false) {
        self.blocks = blocks
        self.truncated = truncated
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        blocks = try container.decode([MarkdownBlock].self, forKey: .blocks)
        truncated = try container.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
    }
}

struct MarkdownListItem: Decodable, Sendable, Equatable {
    let checked: Bool?
    let blocks: [MarkdownBlock]
}

indirect enum MarkdownBlock: Decodable, Sendable, Equatable {
    case heading(level: Int, inlines: [MarkdownInline])
    case paragraph(inlines: [MarkdownInline])
    case codeBlock(language: String?, code: String)
    case blockQuote(blocks: [MarkdownBlock])
    case list(ordered: Bool, start: UInt64, items: [MarkdownListItem])
    case table(alignments: [MarkdownTableAlignment], headers: [[MarkdownInline]], rows: [[[MarkdownInline]]])
    case thematicBreak

    private enum CodingKeys: String, CodingKey {
        case type, level, inlines, language, code, blocks, ordered, start, items, alignments, headers, rows
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "heading":
            self = .heading(
                level: try c.decode(Int.self, forKey: .level),
                inlines: try c.decode([MarkdownInline].self, forKey: .inlines)
            )
        case "paragraph":
            self = .paragraph(inlines: try c.decode([MarkdownInline].self, forKey: .inlines))
        case "code_block":
            self = .codeBlock(
                language: try c.decodeIfPresent(String.self, forKey: .language),
                code: try c.decode(String.self, forKey: .code)
            )
        case "block_quote":
            self = .blockQuote(blocks: try c.decode([MarkdownBlock].self, forKey: .blocks))
        case "list":
            self = .list(
                ordered: try c.decode(Bool.self, forKey: .ordered),
                start: try c.decode(UInt64.self, forKey: .start),
                items: try c.decode([MarkdownListItem].self, forKey: .items)
            )
        case "table":
            self = .table(
                alignments: try c.decode([MarkdownTableAlignment].self, forKey: .alignments),
                headers: try c.decode([[MarkdownInline]].self, forKey: .headers),
                rows: try c.decode([[[MarkdownInline]]].self, forKey: .rows)
            )
        case "thematic_break":
            self = .thematicBreak
        default:
            self = .thematicBreak
        }
    }
}

indirect enum MarkdownInline: Decodable, Sendable, Equatable {
    case text(String)
    case strong([MarkdownInline])
    case emphasis([MarkdownInline])
    case strikethrough([MarkdownInline])
    case code(String)
    case link(url: String, children: [MarkdownInline])
    case blockedImage(alt: String)
    case softBreak
    case hardBreak

    private enum CodingKeys: String, CodingKey {
        case type, text, children, code, url, alt
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "text":
            self = .text(try c.decode(String.self, forKey: .text))
        case "strong":
            self = .strong(try c.decode([MarkdownInline].self, forKey: .children))
        case "emphasis":
            self = .emphasis(try c.decode([MarkdownInline].self, forKey: .children))
        case "strikethrough":
            self = .strikethrough(try c.decode([MarkdownInline].self, forKey: .children))
        case "code":
            self = .code(try c.decode(String.self, forKey: .code))
        case "link":
            self = .link(
                url: try c.decode(String.self, forKey: .url),
                children: try c.decode([MarkdownInline].self, forKey: .children)
            )
        case "blocked_image":
            self = .blockedImage(alt: try c.decode(String.self, forKey: .alt))
        case "hard_break":
            self = .hardBreak
        default:
            self = .softBreak
        }
    }
}

// MARK: - Preview Queue Item & Filter Logic

struct FullscreenPreviewItem: Identifiable, Equatable, Sendable {
    enum Source: Equatable, Sendable {
        case library(id: String, version: String?)
        case channel(channel: String, sequence: UInt64)
    }

    let id: String
    let source: Source
    let name: String
    let mime: String
    let size: UInt64
    let updatedAt: UInt64?
    let sha256: String?
    let canOpen: Bool
    let libraryFile: LibraryFile?
    let versionLabel: String?
    let explicitClass: PreviewClass?
    let explicitPreviewable: Bool?

    static func fromLibrary(_ file: LibraryFile, version: String? = nil, versionLabel: String? = nil) -> Self {
        let key = version.map { "\(file.id)@\($0)" } ?? file.id
        return FullscreenPreviewItem(
            id: key,
            source: .library(id: file.id, version: version ?? file.current),
            name: file.name,
            mime: file.mime,
            size: file.size,
            updatedAt: file.updatedAt ?? file.createdAt,
            sha256: file.sha256,
            canOpen: file.canOpen,
            libraryFile: file,
            versionLabel: versionLabel,
            explicitClass: nil,
            explicitPreviewable: nil
        )
    }

    static func fromMessage(channel: String, message: MessageItem) -> Self? {
        guard !message.retracted, !message.hidden, !message.unreadable, !message.attachmentDeleted,
              let name = message.attachmentName else { return nil }
        let mime = message.attachmentMime ?? ""
        if mime == "application/vnd.dmsg.secret-package" { return nil }
        return FullscreenPreviewItem(
            id: "channel:\(channel):\(message.sequence)",
            source: .channel(channel: channel, sequence: message.sequence),
            name: name,
            mime: mime,
            size: message.attachmentSize ?? 0,
            updatedAt: message.createdAtMs,
            sha256: nil,
            canOpen: true,
            libraryFile: nil,
            versionLabel: nil,
            explicitClass: nil,
            explicitPreviewable: nil
        )
    }

    static func fromAttachment(channel: String, record: ChannelAttachmentRecord) -> Self {
        FullscreenPreviewItem(
            id: "channel:\(channel):\(record.seq)",
            source: .channel(channel: channel, sequence: record.seq),
            name: record.name,
            mime: record.mime,
            size: record.size,
            updatedAt: record.storedAt > 0 ? record.storedAt : record.createdAt,
            sha256: nil,
            canOpen: true,
            libraryFile: nil,
            versionLabel: nil,
            explicitClass: record.previewClass,
            explicitPreviewable: record.previewable
        )
    }
}

struct FullscreenPreviewLaunch: Identifiable {
    let id = UUID()
    let items: [FullscreenPreviewItem]
    let initialID: String
    let showFilter: Bool
    let channelID: String?

    init(items: [FullscreenPreviewItem], initialID: String, showFilter: Bool = true, channelID: String? = nil) {
        self.items = items
        self.initialID = initialID
        self.showFilter = showFilter
        self.channelID = channelID
    }
}

enum PreviewFilterLogic {
    static let maxImageBytes: UInt64 = 20 * 1024 * 1024
    static let maxMarkdownBytes: UInt64 = 2 * 1024 * 1024
    static let maxTextBytes: UInt64 = 1024 * 1024

    private static let imageMimes: Set<String> = [
        "image/png", "image/jpeg", "image/jpg", "image/gif", "image/webp",
        "image/bmp", "image/avif", "image/heic", "image/heif",
    ]
    private static let imageExts: Set<String> = [
        "png", "jpg", "jpeg", "gif", "webp", "bmp", "avif", "heic", "heif",
    ]
    private static let markdownExts: Set<String> = ["md", "markdown", "mdown", "mkd"]
    private static let textMimes: Set<String> = [
        "application/json", "application/ld+json", "application/xml", "application/toml",
        "application/yaml", "application/x-yaml", "application/javascript",
        "application/x-javascript", "application/typescript", "application/x-sh",
        "application/sql", "application/csv",
    ]
    private static let textExts: Set<String> = [
        "txt", "log", "json", "jsonc", "yaml", "yml", "toml", "xml", "csv", "tsv",
        "ini", "cfg", "conf", "env", "sh", "bash", "zsh", "py", "rs", "js", "mjs",
        "cjs", "ts", "mts", "cts", "jsx", "tsx", "html", "htm", "css", "scss", "sql",
        "swift", "kt", "kts", "java", "go", "c", "h", "cpp", "cc", "hpp", "rb", "php",
        "svelte", "vue", "dockerfile", "makefile",
    ]

    static func classify(mime: String, name: String) -> PreviewClass {
        let base = mime.split(separator: ";", maxSplits: 1).first.map { $0.trimmingCharacters(in: .whitespaces).lowercased() } ?? ""
        let ext = name.lastIndex(of: ".").map { name[name.index(after: $0)...].lowercased() } ?? ""
        if base == "application/vnd.dmsg.secret-package" || base == "image/svg+xml" || ext == "svg" || ext == "svgz" {
            return .unsupported
        }
        let generic = base.isEmpty || base == "application/octet-stream"
        if base == "text/markdown" || base == "text/x-markdown" || ((generic || base == "text/plain") && markdownExts.contains(ext)) {
            return .markdown
        }
        if imageMimes.contains(base) || (generic && imageExts.contains(ext)) {
            return .image
        }
        if base.hasPrefix("text/") || textMimes.contains(base) || (generic && textExts.contains(ext)) {
            return .text
        }
        return .unsupported
    }

    static func previewClass(for item: FullscreenPreviewItem) -> PreviewClass {
        item.explicitClass ?? classify(mime: item.mime, name: item.name)
    }

    static func sizeLimit(for kind: PreviewClass) -> UInt64 {
        switch kind {
        case .image: maxImageBytes
        case .markdown: maxMarkdownBytes
        case .text: maxTextBytes
        case .unsupported: 0
        }
    }

    static func isPreviewable(_ item: FullscreenPreviewItem) -> Bool {
        guard item.canOpen else { return false }
        if let explicit = item.explicitPreviewable { return explicit }
        let kind = previewClass(for: item)
        let limit = sizeLimit(for: kind)
        return limit > 0 && item.size <= limit
    }

    static func matches(_ item: FullscreenPreviewItem, filter: PreviewFilter) -> Bool {
        switch filter {
        case .all: true
        case .previewable: isPreviewable(item)
        case .images: previewClass(for: item) == .image
        case .documents:
            switch previewClass(for: item) {
            case .markdown, .text: true
            case .image, .unsupported: false
            }
        }
    }

    static func counts(for items: [FullscreenPreviewItem]) -> [PreviewFilter: Int] {
        var previewable = 0
        var images = 0
        var documents = 0
        for item in items {
            if isPreviewable(item) { previewable += 1 }
            switch previewClass(for: item) {
            case .image: images += 1
            case .markdown, .text: documents += 1
            case .unsupported: break
            }
        }
        return [.all: items.count, .previewable: previewable, .images: images, .documents: documents]
    }

    static func pickNearest(
        items: [FullscreenPreviewItem],
        currentID: String?,
        filter: PreviewFilter
    ) -> (filtered: [FullscreenPreviewItem], activeID: String?) {
        let filtered = items.filter { matches($0, filter: filter) }
        guard !filtered.isEmpty else { return ([], nil) }
        if let currentID, filtered.contains(where: { $0.id == currentID }) {
            return (filtered, currentID)
        }
        guard let currentID, let origin = items.firstIndex(where: { $0.id == currentID }) else {
            return (filtered, filtered.first?.id)
        }
        let allowed = Set(filtered.map(\.id))
        for delta in 1..<items.count {
            let forward = origin + delta
            if forward < items.count, allowed.contains(items[forward].id) {
                return (filtered, items[forward].id)
            }
            let backward = origin - delta
            if backward >= 0, allowed.contains(items[backward].id) {
                return (filtered, items[backward].id)
            }
        }
        return (filtered, filtered.first?.id)
    }
}

// MARK: - Bounded Image Decoding Helper

enum PreviewImageDecoder {
    private static let maxPixelSize = 4096

    nonisolated static func decode(_ data: Data) async -> UIImage? {
        await Task.detached(priority: .userInitiated) {
            let options: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceShouldCacheImmediately: true,
                kCGImageSourceThumbnailMaxPixelSize: maxPixelSize,
            ]
            guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
            guard let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
            return UIImage(cgImage: thumbnail)
        }.value
    }
}
