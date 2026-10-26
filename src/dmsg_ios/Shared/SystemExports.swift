import CryptoKit
import Foundation
import LocalAuthentication
import Security

enum ExportPurpose: String, Codable, Sendable {
    case credentials, files
    var group: String { self == .credentials ? "group.net.dmsg.ios.autofill" : "group.net.dmsg.ios.files" }
    var keyGroupSuffix: String { self == .credentials ? "net.dmsg.ios.autofill" : "net.dmsg.ios.files" }
}
struct ExportHeader: Codable, Sendable {
    let id: String
    let purpose: ExportPurpose
    let accountHash: String
    let expires: UInt64
}
struct CredentialExport: Codable, Identifiable, Sendable { let id: String; let title: String; let username: String; let origin: String }
struct FileExport: Codable, Identifiable, Sendable { let id: String; let name: String; let size: UInt64; let sha256: String; let parts: Int; let modified: UInt64 }
struct CredentialInput: Sendable { let title: String; let username: String; let password: String; let origin: String }
struct FileExportInput: Sendable { let url: URL; let name: String; let sha256: String; let size: UInt64 }
struct PreparedExport: Sendable { let header: ExportHeader; let key: SymmetricKey; let directory: URL }
struct OpenExport: Sendable { let header: ExportHeader; let key: SymmetricKey }

/// Only explicitly exported copies live in these groups. This store never knows
/// a Client workspace, device signing key or content root.
struct SystemExports: Sendable {
    let purpose: ExportPurpose
    let root: URL
    private let keyGroup: String
    private var service: String { "net.dmsg.export." + purpose.rawValue }
    init(_ purpose: ExportPurpose, root: URL? = nil, keyGroup: String? = nil) throws {
        self.purpose = purpose
        guard let directory = root ?? FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: purpose.group) else { throw CocoaError(.fileReadNoPermission) }
        self.root = directory.appending(path: "exports")
        let prefix = Bundle.main.object(forInfoDictionaryKey: "DMsgKeychainPrefix") as? String ?? ""
        self.keyGroup = keyGroup ?? (prefix + purpose.keyGroupSuffix)
        guard keyGroup != nil || (!prefix.isEmpty && !prefix.contains("$(")) else { throw CocoaError(.fileReadNoPermission) }
        try FileManager.default.createDirectory(at: self.root, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.complete])
        var protected = self.root; var values = URLResourceValues(); values.isExcludedFromBackup = true; try protected.setResourceValues(values)
    }
    private func query(_ name: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
         kSecAttrAccount as String: name, kSecAttrAccessGroup as String: keyGroup, kSecAttrSynchronizable as String: false]
    }
    private func encode<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]; return try encoder.encode(value)
    }
    private func aad(_ header: ExportHeader, _ name: String) throws -> Data {
        var bytes = try encode(header); bytes.append(Data((":" + name).utf8)); return bytes
    }
    private func directory(_ header: ExportHeader) throws -> URL {
        guard header.purpose == purpose, UUID(uuidString: header.id) != nil else { throw CocoaError(.fileReadCorruptFile) }
        let directory = root.appending(path: header.id)
        guard directory.resolvingSymlinksInPath().standardizedFileURL.path == root.resolvingSymlinksInPath().appending(path: header.id).standardizedFileURL.path else { throw CocoaError(.fileReadNoPermission) }
        return directory
    }
    func header(checkExpiry: Bool = true) throws -> ExportHeader {
        let file = root.appending(path: "current.json")
        guard (try file.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) <= 4096 else { throw CocoaError(.fileReadTooLarge) }
        let header = try JSONDecoder().decode(ExportHeader.self, from: Data(contentsOf: file))
        _ = try directory(header)
        guard !checkExpiry || Double(header.expires) > Date.now.timeIntervalSince1970 * 1000 else { throw CocoaError(.userCancelled) }
        return header
    }
    func prepareCredentials(_ entries: [CredentialInput], account: String, expires: UInt64) throws -> PreparedExport {
        guard purpose == .credentials, !entries.isEmpty, entries.count <= 50 else { throw CocoaError(.coderInvalidValue) }
        let prepared = try prepare(account: account, expires: expires)
        var committed = false; defer { if !committed { discard(prepared) } }
        var metadata: [CredentialExport] = []
        for entry in entries {
            guard entry.password.utf8.count <= 4096, entry.username.utf8.count <= 1024 else { throw CocoaError(.fileReadTooLarge) }
            let id = UUID().uuidString
            metadata.append(CredentialExport(id: id, title: entry.title, username: entry.username, origin: entry.origin))
            try seal(Data(entry.password.utf8), name: id, prepared: prepared)
        }
        try seal(encode(metadata), name: "manifest", prepared: prepared); committed = true; return prepared
    }
    func prepareFiles(_ entries: [FileExportInput], account: String, expires: UInt64) throws -> PreparedExport {
        guard purpose == .files, !entries.isEmpty, entries.count <= 10,
              entries.allSatisfy({ $0.size <= 100 * 1024 * 1024 }),
              entries.reduce(UInt64(0), { $0 + $1.size }) <= 200 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
        let prepared = try prepare(account: account, expires: expires)
        var committed = false; defer { if !committed { discard(prepared) } }
        var metadata: [FileExport] = []
        for entry in entries {
            guard entry.size <= 100 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
            let id = UUID().uuidString, input = try FileHandle(forReadingFrom: entry.url)
            defer { try? input.close() }
            var hash = SHA256(), size: UInt64 = 0, parts = 0
            while let bytes = try input.read(upToCount: 1024 * 1024), !bytes.isEmpty {
                size += UInt64(bytes.count); guard size <= entry.size else { throw CocoaError(.fileReadCorruptFile) }
                hash.update(data: bytes); try seal(bytes, name: id + "." + String(parts), prepared: prepared); parts += 1
            }
            guard size == entry.size, hash.finalize().map({ String(format: "%02x", $0) }).joined() == entry.sha256 else { throw CocoaError(.fileReadCorruptFile) }
            metadata.append(FileExport(id: id, name: entry.name, size: size, sha256: entry.sha256, parts: parts, modified: UInt64(Date.now.timeIntervalSince1970 * 1000)))
        }
        try seal(encode(metadata), name: "manifest", prepared: prepared); committed = true; return prepared
    }
    private func prepare(account: String, expires: UInt64) throws -> PreparedExport {
        let now = UInt64(Date.now.timeIntervalSince1970 * 1000)
        guard expires > now, expires - now <= 86_400_000 else { throw CocoaError(.coderInvalidValue) }
        let header = ExportHeader(id: UUID().uuidString, purpose: purpose, accountHash: IncomingShares.accountHash(account), expires: expires)
        let directory = try directory(header)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.complete])
        return PreparedExport(header: header, key: SymmetricKey(size: .bits256), directory: directory)
    }
    private func seal(_ bytes: Data, name: String, prepared: PreparedExport) throws {
        let box = try AES.GCM.seal(bytes, using: prepared.key, authenticating: aad(prepared.header, name))
        guard let combined = box.combined else { throw CocoaError(.coderInvalidValue) }
        try combined.write(to: prepared.directory.appending(path: name), options: [.atomic, .completeFileProtection])
    }
    /// Publish only after the main app rechecks the originating account/session.
    func publish(_ prepared: PreparedExport, biometryOnly: Bool) throws {
        let previous = try? header(checkExpiry: false)
        guard let acl = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly,
            biometryOnly ? .biometryCurrentSet : .userPresence, nil) else { throw CocoaError(.fileWriteNoPermission) }
        var attributes = query(prepared.header.id)
        attributes[kSecValueData as String] = prepared.key.withUnsafeBytes { Data($0) }
        attributes[kSecAttrAccessControl as String] = acl
        guard SecItemAdd(attributes as CFDictionary, nil) == errSecSuccess else { throw CocoaError(.fileWriteNoPermission) }
        do { try encode(prepared.header).write(to: root.appending(path: "current.json"), options: [.atomic, .completeFileProtection]) }
        catch { SecItemDelete(query(prepared.header.id) as CFDictionary); throw error }
        lockFiles()
        if let previous, previous.id != prepared.header.id {
            SecItemDelete(query(previous.id) as CFDictionary)
            if let directory = try? directory(previous) { try? FileManager.default.removeItem(at: directory) }
        }
    }
    func authenticate() throws -> OpenExport {
        let header = try header()
        let context = LAContext(); context.localizedReason = String(localized: "Access the copies you explicitly exported from dMsg")
        defer { context.invalidate() }
        var query = query(header.id); query[kSecReturnData as String] = true; query[kSecMatchLimit as String] = kSecMatchLimitOne; query[kSecUseAuthenticationContext as String] = context
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess, let data = result as? Data, data.count == 32 else { throw CocoaError(.userCancelled) }
        let opened = OpenExport(header: header, key: SymmetricKey(data: data))
        _ = try plain("manifest", opened: opened)
        return opened
    }
    func credentialMetadata(_ opened: OpenExport) throws -> [CredentialExport] { try JSONDecoder().decode([CredentialExport].self, from: plain("manifest", opened: opened)) }
    func credential(_ id: String, opened: OpenExport) throws -> String {
        guard purpose == .credentials, UUID(uuidString: id) != nil, try credentialMetadata(opened).contains(where: { $0.id == id }) else { throw CocoaError(.fileReadNoSuchFile) }
        guard let value = String(data: try plain(id, opened: opened), encoding: .utf8) else { throw CocoaError(.fileReadCorruptFile) }; return value
    }
    func fileMetadata(_ opened: OpenExport) throws -> [FileExport] { try JSONDecoder().decode([FileExport].self, from: plain("manifest", opened: opened)) }
    func materialize(_ file: FileExport, opened: OpenExport, output: URL, cancelled: () -> Bool) throws {
        guard purpose == .files, UUID(uuidString: file.id) != nil, file.parts >= 0, file.parts <= 100, file.size <= 100 * 1024 * 1024 else { throw CocoaError(.fileReadCorruptFile) }
        guard FileManager.default.createFile(atPath: output.path, contents: nil, attributes: [.protectionKey: FileProtectionType.complete]) else { throw CocoaError(.fileWriteUnknown) }
        let stream = try FileHandle(forWritingTo: output); defer { try? stream.close() }
        var complete = false; defer { if !complete { try? FileManager.default.removeItem(at: output) } }
        var hash = SHA256(), size: UInt64 = 0
        for part in 0..<file.parts {
            if cancelled() { throw CocoaError(.userCancelled) }
            let bytes = try plain(file.id + "." + String(part), opened: opened)
            hash.update(data: bytes); size += UInt64(bytes.count); guard size <= file.size else { throw CocoaError(.fileReadCorruptFile) }
            try stream.write(contentsOf: bytes)
        }
        guard size == file.size, hash.finalize().map({ String(format: "%02x", $0) }).joined() == file.sha256 else { throw CocoaError(.fileReadCorruptFile) }
        try stream.synchronize(); complete = true
    }
    private func plain(_ name: String, opened: OpenExport) throws -> Data {
        guard try header().id == opened.header.id else { throw CocoaError(.userCancelled) }
        let directory = try directory(opened.header), path = directory.appending(path: name)
        guard path.resolvingSymlinksInPath().deletingLastPathComponent() == directory.resolvingSymlinksInPath(),
              (try path.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) <= 1024 * 1024 + 64 else { throw CocoaError(.fileReadCorruptFile) }
        return try AES.GCM.open(AES.GCM.SealedBox(combined: Data(contentsOf: path)), using: opened.key, authenticating: aad(opened.header, name))
    }
    struct FileSession: Codable { let snapshot: String; let key: Data; let expires: UInt64 }
    /// File Provider's short session is established only by its interactive UI.
    func unlockFiles(_ opened: OpenExport) throws {
        guard purpose == .files else { throw CocoaError(.fileReadNoPermission) }
        let session = FileSession(snapshot: opened.header.id, key: opened.key.withUnsafeBytes { Data($0) }, expires: min(opened.header.expires, UInt64(Date.now.timeIntervalSince1970 * 1000) + 300_000))
        lockFiles()
        var entry = query("file-session"); entry[kSecValueData as String] = try encode(session); entry[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        guard SecItemAdd(entry as CFDictionary, nil) == errSecSuccess else { throw CocoaError(.fileWriteNoPermission) }
    }
    func fileSession() throws -> OpenExport {
        guard purpose == .files else { throw CocoaError(.fileReadNoPermission) }
        let header = try header()
        var query = query("file-session"); query[kSecReturnData as String] = true; query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess, let data = result as? Data else { throw CocoaError(.userCancelled) }
        let session = try JSONDecoder().decode(FileSession.self, from: data)
        guard session.snapshot == header.id, session.key.count == 32, Double(session.expires) > Date.now.timeIntervalSince1970 * 1000 else { throw CocoaError(.userCancelled) }
        return OpenExport(header: header, key: SymmetricKey(data: session.key))
    }
    func lockFiles() { if purpose == .files { SecItemDelete(query("file-session") as CFDictionary) } }
    func discard(_ prepared: PreparedExport) { try? FileManager.default.removeItem(at: prepared.directory) }
    func invalidate(account: String? = nil) throws {
        if let account, let current = try? header(), current.accountHash != IncomingShares.accountHash(account) { return }
        var query: [String: Any] = [kSecClass as String:kSecClassGenericPassword,kSecAttrService as String:service,kSecAttrAccessGroup as String:keyGroup]
        query[kSecAttrSynchronizable as String] = false
        SecItemDelete(query as CFDictionary)
        if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) }
    }
}
