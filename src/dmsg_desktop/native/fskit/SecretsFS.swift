// Apple FSKit only. No FUSE compatibility layer, owner credential, or plaintext backing file.
import Foundation
import FSKit

@main struct SecretsExtension: UnaryFileSystemExtension {
    var fileSystem: FSUnaryFileSystem & FSUnaryFileSystemOperations { SecretsFileSystem() }
}

final class SecretsFileSystem: FSUnaryFileSystem, FSUnaryFileSystemOperations {
    var resource: FSPathURLResource?
    var volume: SecretVolume?
    var loading: ContentRequest?
    let lifecycle = NSLock()
    var loadID: UUID?

    func probeResource(resource: FSResource, replyHandler reply: @escaping (FSProbeResult?, Error?) -> Void) {
        guard resource is FSPathURLResource else { return reply(nil, POSIXError(.ENOTSUP)) }
        reply(.usable(name: "dMsg secret files", containerID: FSContainerIdentifier(uuid: UUID())), nil)
    }

    func loadResource(resource: FSResource, options: FSTaskOptions, replyHandler reply: @escaping (FSVolume?, Error?) -> Void) {
        guard let path = resource as? FSPathURLResource, !options.taskOptions.contains("-f"), path.url.startAccessingSecurityScopedResource() else { return reply(nil, POSIXError(.EACCES)) }
        let token = UUID()
        lifecycle.lock()
        if self.resource != nil { lifecycle.unlock(); path.url.stopAccessingSecurityScopedResource(); return reply(nil, POSIXError(.EBUSY)) }
        self.resource = path; loadID = token
        lifecycle.unlock()
        let socket = path.url.appendingPathComponent("content.sock").path
        let job = ContentRequest(path: socket, request: ["method": "metadata"]) { [weak self] result in
            guard let self else { return reply(nil, POSIXError(.EIO)) }
            do {
                let volume = try SecretVolume(socket: socket, metadata: result.get())
                self.lifecycle.lock()
                let live = self.loadID == token
                if live { self.volume = volume; self.loadID = nil; self.loading = nil }
                self.lifecycle.unlock()
                guard live else { volume.stop(); return reply(nil, POSIXError(.ECANCELED)) }
                self.containerStatus = .ready; reply(volume, nil); volume.watch()
            } catch {
                self.lifecycle.lock()
                let live = self.loadID == token
                if live { self.resource = nil; self.loading = nil; self.loadID = nil }
                self.lifecycle.unlock()
                if live { path.url.stopAccessingSecurityScopedResource() }
                reply(nil, POSIXError(.EACCES))
            }
        }
        lifecycle.lock()
        let live = loadID == token
        if live { loading = job }
        lifecycle.unlock()
        if !live { job.cancel() }
    }

    func unloadResource(resource: FSResource, options: FSTaskOptions, replyHandler reply: @escaping (Error?) -> Void) {
        lifecycle.lock()
        let pending = loading, current = volume, path = self.resource
        loading = nil; volume = nil; self.resource = nil; loadID = nil
        lifecycle.unlock()
        pending?.cancel(); current?.stop(); path?.url.stopAccessingSecurityScopedResource()
        reply(nil)
    }

}

final class SecretNode: FSItem {
    let number: UInt64
    let path: String
    let parentNumber: UInt64
    let entry: String?
    let generation: UInt64
    let size: UInt64
    var invalid = false
    var closed = true
    init(number: UInt64, path: String, parent: UInt64 = 2, entry: String?, generation: UInt64, size: UInt64) {
        self.number = number; self.path = path; self.parentNumber = parent; self.entry = entry; self.generation = generation; self.size = size
        super.init()
    }
}

final class SecretVolume: FSVolume, FSVolume.Operations, FSVolume.ReadWriteOperations, FSVolume.DataCacheHandler, FSVolume.AccessCheckHandler {
    let socket: String
    let owner: uid_t
    let windowMilliseconds: UInt64
    let root = SecretNode(number: 2, path: "", entry: nil, generation: 0, size: 0)
    let state = DispatchQueue(label: "net.dmsg.secrets.volume")
    let cache = DispatchQueue(label: "net.dmsg.secrets.cache")
    var nodes: [String: SecretNode] = [:]
    var jobs: [UUID: (SecretNode?, ContentRequest)] = [:]
    var active = true
    var watching = false
    var windows: [String: (String, UInt64)] = [:]
    var nextDirectory: UInt64 = 3

    init(socket: String, metadata: [String: Any]) throws {
        guard let uid = unsigned(metadata["owner_uid"], maximum: UInt64(UInt32.max)), uid == getuid(),
              let mode = metadata["authorization"] as? [String: Any], mode["type"] as? String == "macos_file_window",
              let window = unsigned(mode["window_ms"], maximum: 60000), window > 0 else { throw POSIXError(.EACCES) }
        self.socket = socket; owner = uid_t(uid); windowMilliseconds = window
        super.init(volumeID: FSVolume.Identifier(uuid: UUID()), volumeName: FSFileName(string: "dMsg secret files"))
        nodes[""] = root
        try install(metadata)
    }

    var maximumLinkCount: Int { 1 }
    var maximumNameLength: Int { 255 }
    var restrictsOwnershipChanges: Bool { true }
    var truncatesLongNames: Bool { false }
    var isDataCacheInhibited: Bool { false }
    var isAccessCheckInhibited: Bool { false }
    var supportedVolumeCapabilities: FSVolume.SupportedCapabilities { FSVolume.SupportedCapabilities() }
    var volumeStatistics: FSStatFSResult {
        let r = FSStatFSResult(fileSystemTypeName: "dmsgsecrets")
        r.blockSize = 4096; r.ioSize = 1048576; r.totalBlocks = 1; r.usedBlocks = 1
        r.totalFiles = state.sync { UInt64(nodes.count) }; return r
    }

    func attributes(_ node: SecretNode) -> FSItem.Attributes {
        let a = FSItem.Attributes(); a.type = node.entry == nil ? .directory : .file
        a.mode = node.entry == nil ? 0o500 : 0o400; a.uid = owner; a.gid = getgid(); a.linkCount = 1
        a.fileID = FSItem.Identifier(rawValue: node.number)!; a.parentID = FSItem.Identifier(rawValue: node.parentNumber)!
        a.size = node.size; a.allocSize = node.size; return a
    }

    // State queue only, except the initial construction before publication.
    func install(_ metadata: [String: Any]) throws {
        guard let entries = metadata["entries"] as? [[String: Any]], entries.count <= 64 else { throw POSIXError(.EIO) }
        var seen = Set<String>()
        for (index, entry) in entries.enumerated() {
            guard let id = entry["id"] as? String, let path = entry["path"] as? String,
                  let generation = unsigned(entry["generation"]), generation > 0,
                  let size = unsigned(entry["size"], maximum: 104857600), !path.isEmpty, path.utf8.count <= 1024,
                  !path.contains("\\"), !path.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
                  seen.insert(path).inserted else { throw POSIXError(.EIO) }
            let parts = path.split(separator: "/", omittingEmptySubsequences: false)
            guard parts.count <= 8, parts.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." && $0.utf8.count <= 255 }) else { throw POSIXError(.EIO) }
            var directory = ""
            var parentNumber: UInt64 = 2
            for part in parts.dropLast() {
                directory += directory.isEmpty ? String(part) : "/" + part
                if nodes[directory] == nil { nodes[directory] = SecretNode(number: nextDirectory, path: directory, parent: parentNumber, entry: nil, generation: 0, size: 0); nextDirectory += 1 }
                parentNumber = nodes[directory]!.number
            }
            if let old = nodes[path], old.generation != generation {
                if !old.invalid { old.invalid = true; revoke(old, acknowledge: generation) }
            }
            if nodes[path]?.generation != generation {
                let node = SecretNode(number: (UInt64(1) << 62) | (generation << 7) | UInt64(index), path: path, parent: parentNumber, entry: id, generation: generation, size: size)
                nodes[path] = node
            }
        }
    }

    func request(_ payload: [String: Any], node: SecretNode? = nil, completion: @escaping (Result<[String: Any], Error>) -> Void) {
        let id = UUID()
        let job = ContentRequest(path: socket, request: payload) { [weak self] result in
            guard let self else { return }
            self.state.async { self.jobs.removeValue(forKey: id); completion(result) }
        }
        jobs[id] = (node, job)
    }

    func watch() {
        state.async {
            guard self.active, !self.watching else { return }
            self.watching = true; self.pollMetadata()
        }
    }

    private func pollMetadata() {
        guard active else { return }
        request(["method": "metadata"]) { result in
            do { try self.install(result.get()) } catch { self.stopOnQueue(); return }
            self.state.asyncAfter(deadline: .now() + .milliseconds(200)) { self.pollMetadata() }
        }
    }

    func acknowledge(_ node: SecretNode, generation: UInt64, failed: Bool) {
        guard let entry = node.entry else { return }
        request(["method": "acknowledge", "entry": entry, "generation": generation, "failed": failed]) { _ in }
    }

    func revoke(_ node: SecretNode, acknowledge generation: UInt64) {
        // Never hold the state queue while FSKit can call back into this volume.
        cache.async {
            let closed = self.state.sync { node.closed }
            let error = closed ? nil : self.setCacheState(for: node, cacheMode: .none, coherencyType: .noCache, action: .revoke)
            self.state.async {
                let failed = error != nil && !node.closed
                self.acknowledge(node, generation: generation, failed: failed)
                if failed { self.stopOnQueue() }
            }
        }
    }

    func stopOnQueue() {
        guard active else { return }; active = false
        for (_, job) in jobs.values { job.cancel() }; jobs.removeAll()
        for node in nodes.values where node.entry != nil { node.invalid = true; revoke(node, acknowledge: node.generation) }
        windows.removeAll()
    }
    func stop() { state.async { self.stopOnQueue() } }

    func activate(options: FSTaskOptions, replyHandler reply: @escaping (FSItem?, Error?) -> Void) { reply(root, nil) }
    func deactivate(options: FSDeactivateOptions, replyHandler reply: @escaping (Error?) -> Void) { stop(); reply(nil) }
    func mount(options: FSTaskOptions, replyHandler reply: @escaping (Error?) -> Void) { reply(nil) }
    func unmount(replyHandler reply: @escaping () -> Void) { stop(); reply() }
    func synchronize(flags: FSSyncFlags, replyHandler reply: @escaping (Error?) -> Void) { reply(nil) }
    func getAttributes(_ desiredAttributes: FSItem.GetAttributesRequest, of item: FSItem, replyHandler reply: @escaping (FSItem.Attributes?, Error?) -> Void) {
        guard let node = item as? SecretNode else { return reply(nil, POSIXError(.EINVAL)) }; reply(attributes(node), nil)
    }
    func setAttributes(_ newAttributes: FSItem.SetAttributesRequest, on item: FSItem, replyHandler reply: @escaping (FSItem.Attributes?, Error?) -> Void) { reply(nil, POSIXError(.EROFS)) }
    func lookupItem(named name: FSFileName, inDirectory directory: FSItem, replyHandler reply: @escaping (FSItem?, FSFileName?, Error?) -> Void) {
        guard let parent = directory as? SecretNode, parent.entry == nil, let nameString = name.string, !nameString.contains("/"), nameString != ".", nameString != ".." else { return reply(nil, nil, POSIXError(.ENOENT)) }
        state.async { let path = parent.path.isEmpty ? nameString : parent.path + "/" + nameString; guard let node = self.nodes[path], !node.invalid else { return reply(nil, nil, POSIXError(.ENOENT)) }; reply(node, name, nil) }
    }
    func reclaimItem(_ item: FSItem, replyHandler reply: @escaping (Error?) -> Void) { reply(nil) }
    func readSymbolicLink(_ item: FSItem, replyHandler reply: @escaping (FSFileName?, Error?) -> Void) { reply(nil, POSIXError(.ENOTSUP)) }
    func createItem(named name: FSFileName, type: FSItem.ItemType, inDirectory directory: FSItem, attributes newAttributes: FSItem.SetAttributesRequest, replyHandler reply: @escaping (FSItem?, FSFileName?, Error?) -> Void) { reply(nil, nil, POSIXError(.EROFS)) }
    func createSymbolicLink(named name: FSFileName, inDirectory directory: FSItem, attributes newAttributes: FSItem.SetAttributesRequest, linkContents contents: FSFileName, replyHandler reply: @escaping (FSItem?, FSFileName?, Error?) -> Void) { reply(nil, nil, POSIXError(.EROFS)) }
    func createLink(to item: FSItem, named name: FSFileName, inDirectory directory: FSItem, replyHandler reply: @escaping (FSFileName?, Error?) -> Void) { reply(nil, POSIXError(.EROFS)) }
    func removeItem(_ item: FSItem, named name: FSFileName, fromDirectory directory: FSItem, replyHandler reply: @escaping (Error?) -> Void) { reply(POSIXError(.EROFS)) }
    func renameItem(_ item: FSItem, inDirectory sourceDirectory: FSItem, named sourceName: FSFileName, to destinationName: FSFileName, inDirectory destinationDirectory: FSItem, overItem: FSItem?, replyHandler reply: @escaping (FSFileName?, Error?) -> Void) { reply(nil, POSIXError(.EROFS)) }
    func enumerateDirectory(_ directory: FSItem, startingAt cookie: FSDirectoryCookie, verifier: FSDirectoryVerifier, attributes: FSItem.GetAttributesRequest?, packer: FSDirectoryEntryPacker, replyHandler reply: @escaping (FSDirectoryVerifier, Error?) -> Void) {
        guard let parent = directory as? SecretNode, parent.entry == nil else { return reply(FSDirectoryVerifier(0), POSIXError(.ENOTDIR)) }
        state.async {
            let prefix = parent.path.isEmpty ? "" : parent.path + "/"
            let children = self.nodes.values.filter { !$0.path.isEmpty && !$0.invalid && $0.path.hasPrefix(prefix) && !$0.path.dropFirst(prefix.count).contains("/") }.sorted { $0.path < $1.path }
            guard cookie.rawValue <= children.count else { return reply(FSDirectoryVerifier(0), POSIXError(.EINVAL)) }
            for index in Int(cookie.rawValue)..<children.count {
                let node = children[index]; let name = String(node.path.dropFirst(prefix.count))
                if !packer.packEntry(name: FSFileName(string: name), itemType: node.entry == nil ? .directory : .file, itemID: FSItem.Identifier(rawValue: node.number)!, nextCookie: FSDirectoryCookie(UInt64(index + 1)), attributes: self.attributes(node)) { break }
            }
            reply(FSDirectoryVerifier(1), nil)
        }
    }
    func checkAccess(to item: FSItem, requestedAccess access: FSVolume.AccessMask, context: FSContext, replyHandler reply: @escaping (FSCheckAccessResult?, Error?) -> Void) {
        let directory = (item as? SecretNode)?.entry == nil
        // Read/list, read metadata and directory search only. All mutation and executable-file access is denied.
        let allowed: UInt = (1 << 1) | (1 << 7) | (1 << 9) | (1 << 11) | (directory ? (1 << 3) : 0)
        reply(FSCheckAccessResult(accessAllowed: context.realUserID == Int(owner) && context.effectiveUserID == Int(owner) && access.rawValue & ~allowed == 0), nil)
    }
    private func expireOnQueue(_ node: SecretNode) {
        guard let entry = node.entry, let window = windows[entry] else { return }
        guard let now = try? continuousMilliseconds(), now < window.1 else {
            windows.removeValue(forKey: entry); node.invalid = true; revoke(node, acknowledge: node.generation); return
        }
    }

    func open(_ item: FSItem, modes: FSVolume.OpenModes, cacheMode: FSVolume.DataCacheMode, context: FSContext, replyHandler reply: @escaping (FSOpenItemResult?, Error?) -> Void) {
        guard let node = item as? SecretNode, context.realUserID == Int(owner), context.effectiveUserID == Int(owner), !modes.contains(.write) else { return reply(nil, POSIXError(.EACCES)) }
        state.async { self.expireOnQueue(node); guard self.active, !node.invalid else { return reply(nil, POSIXError(.EACCES)) }; node.closed = false; reply(FSOpenItemResult(grantedCoherency: .noCache), nil) }
    }
    func close(_ item: FSItem, context: FSContext, replyHandler reply: @escaping () -> Void) {
        state.async {
            if let node = item as? SecretNode { node.closed = true; for (pending, job) in self.jobs.values where pending === node { job.cancel() } }
            reply()
        }
    }
    func upgrade(_ item: FSItem, cacheMode: FSVolume.DataCacheMode, context: FSContext, replyHandler reply: @escaping (FSUpgradeItemResult?, Error?) -> Void) { reply(nil, POSIXError(.ENOTSUP)) }
    func read(from item: FSItem, at offset: off_t, length: Int, into buffer: FSMutableFileDataBuffer, replyHandler reply: @escaping (Int, Error?) -> Void) {
        guard let node = item as? SecretNode, let entry = node.entry, offset >= 0, (0...1048576).contains(length) else { return reply(0, POSIXError(.EINVAL)) }
        state.async {
            self.expireOnQueue(node)
            guard self.active, !node.invalid, !node.closed, self.jobs.values.filter({ $0.0 != nil }).count < 64 else { return reply(0, POSIXError(.EACCES)) }
            self.request(["method": "read", "entry": entry, "generation": node.generation, "offset": UInt64(offset), "length": length], node: node) { result in
                do {
                    let value = try result.get()
                    guard self.active, !node.invalid, !node.closed, let encoded = value["base64url"] as? String,
                          let grant = value["grant"] as? [String: Any], let request = grant["request_id"] as? String,
                          let remaining = unsigned(grant["remaining_ms"], maximum: 60000), remaining > 0,
                          let candidate = unsigned(grant["monotonic_deadline_ms"]) else { throw POSIXError(.EACCES) }
                    var base64 = encoded.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/"); base64 += String(repeating: "=", count: (4 - base64.count % 4) % 4)
                    guard var bytes = Data(base64Encoded: base64), bytes.count <= length, UInt64(bytes.count) <= node.size.saturatingSubtract(UInt64(offset)) else { throw POSIXError(.EIO) }
                    defer { bytes.resetBytes(in: 0..<bytes.count) }
                    let now = try continuousMilliseconds()
                    guard now < candidate, candidate - now <= self.windowMilliseconds else { throw POSIXError(.EACCES) }
                    if let old = self.windows[entry], old.0 == request { self.windows[entry] = (request, min(old.1, candidate)) }
                    else {
                        self.windows[entry] = (request, candidate)
                        self.state.asyncAfter(deadline: .now() + .milliseconds(Int(candidate - now))) {
                            guard self.windows[entry]?.0 == request else { return }
                            self.windows.removeValue(forKey: entry); node.invalid = true
                            self.revoke(node, acknowledge: node.generation); self.watch()
                        }
                    }
                    guard let deadline = self.windows[entry]?.1, try continuousMilliseconds() < deadline else { throw POSIXError(.EACCES) }
                    let copied = buffer.withUnsafeMutableBytes { destination -> Bool in
                        guard bytes.count <= destination.count else { return false }
                        bytes.withUnsafeBytes { source in if !bytes.isEmpty { destination.baseAddress!.copyMemory(from: source.baseAddress!, byteCount: bytes.count) } }
                        return true
                    }
                    guard copied, try continuousMilliseconds() < deadline else {
                        buffer.withUnsafeMutableBytes { $0.initializeMemory(as: UInt8.self, repeating: 0) }
                        throw POSIXError(.EACCES)
                    }
                    reply(bytes.count, nil)
                } catch { reply(0, POSIXError(.EACCES)) }
            }
        }
    }
    func write(contents: Data, to item: FSItem, at offset: off_t, replyHandler reply: @escaping (Int, Error?) -> Void) { reply(0, POSIXError(.EROFS)) }
}

private extension UInt64 { func saturatingSubtract(_ other: UInt64) -> UInt64 { self >= other ? self - other : 0 } }
