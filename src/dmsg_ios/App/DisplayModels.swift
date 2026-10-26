import Foundation

struct LibraryListing: Decodable {
    let folder: FolderItem?
    let folders: [FolderItem]
    let files: [LibraryFile]
    static let empty = LibraryListing(folder: nil, folders: [], files: [])
}

struct FolderItem: Decodable, Identifiable { let id: String; let name: String; let parent: String? }
struct LibraryFile: Decodable, Identifiable {
    let id: String; let name: String; let mime: String; let size: UInt64
    let current: String; let sha256: String; let status: String
    let versions: Int
}

struct ApprovalView: Decodable, Identifiable {
    let id: String; let digest: String; let type: String; let state: String
    let source: String; let risk: String; let expiresAt: UInt64
    let mobileSupported: Bool?
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
