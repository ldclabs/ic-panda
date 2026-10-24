// Synthetic-data gate: never reads Vault, Files, or daemon credentials.
import Foundation
import FSKit
import OSLog

@main struct SecretsProbeExtension: UnaryFileSystemExtension {
    var fileSystem: FSUnaryFileSystem & FSUnaryFileSystemOperations { ProbeFileSystem() }
}

final class ProbeFileSystem: FSUnaryFileSystem, FSUnaryFileSystemOperations {
    func probeResource(resource: FSResource, replyHandler reply: @escaping (FSProbeResult?, Error?) -> Void) {
        reply(.usable(name: "dMsg synthetic FSKit probe", containerID: FSContainerIdentifier(uuid: UUID())), nil)
    }
    func loadResource(resource: FSResource, options: FSTaskOptions, replyHandler reply: @escaping (FSVolume?, Error?) -> Void) {
        containerStatus = .ready
        reply(ProbeVolume(), nil)
    }
    func unloadResource(resource: FSResource, options: FSTaskOptions, replyHandler reply: @escaping (Error?) -> Void) { reply(nil) }
}

final class ProbeItem: FSItem {
    let number: UInt64
    init(_ number: UInt64) { self.number = number; super.init() }
}

// A fresh file gets exactly one delayed synthetic grant. Once it expires,
// every read fails until remount: an OS cache hit cannot hide behind reapproval.
final class ProbeVolume: FSVolume, FSVolume.Operations, FSVolume.ReadWriteOperations, FSVolume.DataCacheHandler {
    let logger = Logger(subsystem: "net.dmsg.SecretsProbe", category: "cache-gate")
    let root = ProbeItem(2)
    let files = ["fixture.env": ProbeItem(3), "second.bin": ProbeItem(4)]
    let queue = DispatchQueue(label: "net.dmsg.fskit.probe")
    var deadlines: [UInt64: UInt64] = [:]
    var pending = 0
    var active = true
    let owner = getuid()
    let content = Data(("DMSG_PROBE='synthetic-only'\n" + String(repeating: "#fixture\n", count: 150000)).utf8)
    init() { super.init(volumeID: FSVolume.Identifier(uuid: UUID()), volumeName: FSFileName(string: "dMsg probe")) }
    var maximumLinkCount: Int { 1 }
    var maximumNameLength: Int { 255 }
    var restrictsOwnershipChanges: Bool { true }
    var truncatesLongNames: Bool { false }
    var isDataCacheInhibited: Bool { false }
    var supportedVolumeCapabilities: FSVolume.SupportedCapabilities { FSVolume.SupportedCapabilities() }
    var volumeStatistics: FSStatFSResult {
        let r = FSStatFSResult(fileSystemTypeName: "dmsgsecretsprobe")
        r.blockSize = 4096; r.ioSize = 4096; r.totalBlocks = 10; r.usedBlocks = 10; r.totalFiles = 3
        return r
    }
    func attrs(_ item: ProbeItem) -> FSItem.Attributes {
        let a = FSItem.Attributes()
        a.type = item.number == 2 ? .directory : .file
        a.mode = item.number == 2 ? 0o500 : 0o400
        a.uid = owner; a.gid = getgid(); a.linkCount = 1
        a.fileID = FSItem.Identifier(rawValue: item.number)!
        a.parentID = FSItem.Identifier(rawValue: 2)!
        a.size = item.number == 2 ? 0 : UInt64(content.count)
        a.allocSize = a.size
        return a
    }
    func activate(options: FSTaskOptions, replyHandler reply: @escaping (FSItem?, Error?) -> Void) { reply(root, nil) }
    func deactivate(options: FSDeactivateOptions, replyHandler reply: @escaping (Error?) -> Void) {
        queue.async { self.active = false; self.deadlines.removeAll(); reply(nil) }
    }
    func mount(options: FSTaskOptions, replyHandler reply: @escaping (Error?) -> Void) { reply(nil) }
    func unmount(replyHandler reply: @escaping () -> Void) { queue.async { self.active = false; self.deadlines.removeAll(); reply() } }
    func synchronize(flags: FSSyncFlags, replyHandler reply: @escaping (Error?) -> Void) { reply(nil) }
    func getAttributes(_ desiredAttributes: FSItem.GetAttributesRequest, of item: FSItem, replyHandler reply: @escaping (FSItem.Attributes?, Error?) -> Void) {
        guard let i = item as? ProbeItem else { return reply(nil, POSIXError(.EINVAL)) }; reply(attrs(i), nil)
    }
    func setAttributes(_ newAttributes: FSItem.SetAttributesRequest, on item: FSItem, replyHandler reply: @escaping (FSItem.Attributes?, Error?) -> Void) { reply(nil, POSIXError(.EROFS)) }
    func lookupItem(named name: FSFileName, inDirectory directory: FSItem, replyHandler reply: @escaping (FSItem?, FSFileName?, Error?) -> Void) {
        guard directory === root, let nameString = name.string, let item = files[nameString] else { return reply(nil, nil, POSIXError(.ENOENT)) }; reply(item, name, nil)
    }
    func reclaimItem(_ item: FSItem, replyHandler reply: @escaping (Error?) -> Void) { reply(nil) }
    func readSymbolicLink(_ item: FSItem, replyHandler reply: @escaping (FSFileName?, Error?) -> Void) { reply(nil, POSIXError(.ENOTSUP)) }
    func createItem(named name: FSFileName, type: FSItem.ItemType, inDirectory directory: FSItem, attributes newAttributes: FSItem.SetAttributesRequest, replyHandler reply: @escaping (FSItem?, FSFileName?, Error?) -> Void) { reply(nil, nil, POSIXError(.EROFS)) }
    func createSymbolicLink(named name: FSFileName, inDirectory directory: FSItem, attributes newAttributes: FSItem.SetAttributesRequest, linkContents contents: FSFileName, replyHandler reply: @escaping (FSItem?, FSFileName?, Error?) -> Void) { reply(nil, nil, POSIXError(.EROFS)) }
    func createLink(to item: FSItem, named name: FSFileName, inDirectory directory: FSItem, replyHandler reply: @escaping (FSFileName?, Error?) -> Void) { reply(nil, POSIXError(.EROFS)) }
    func removeItem(_ item: FSItem, named name: FSFileName, fromDirectory directory: FSItem, replyHandler reply: @escaping (Error?) -> Void) { reply(POSIXError(.EROFS)) }
    func renameItem(_ item: FSItem, inDirectory sourceDirectory: FSItem, named sourceName: FSFileName, to destinationName: FSFileName, inDirectory destinationDirectory: FSItem, overItem: FSItem?, replyHandler reply: @escaping (FSFileName?, Error?) -> Void) { reply(nil, POSIXError(.EROFS)) }
    func enumerateDirectory(_ directory: FSItem, startingAt cookie: FSDirectoryCookie, verifier: FSDirectoryVerifier, attributes: FSItem.GetAttributesRequest?, packer: FSDirectoryEntryPacker, replyHandler reply: @escaping (FSDirectoryVerifier, Error?) -> Void) {
        guard directory === root else { return reply(FSDirectoryVerifier(0), POSIXError(.ENOTDIR)) }
        let names = files.keys.sorted()
        guard cookie.rawValue <= names.count else { return reply(FSDirectoryVerifier(0), POSIXError(.EINVAL)) }
        for index in Int(cookie.rawValue)..<names.count {
            let name = names[index], item = files[names[index]]!
            if !packer.packEntry(name: FSFileName(string: name), itemType: .file, itemID: FSItem.Identifier(rawValue: item.number)!, nextCookie: FSDirectoryCookie(UInt64(index + 1)), attributes: attrs(item)) { break }
        }
        reply(FSDirectoryVerifier(1), nil)
    }
    func open(_ item: FSItem, modes: FSVolume.OpenModes, cacheMode: FSVolume.DataCacheMode, context: FSContext, replyHandler reply: @escaping (FSOpenItemResult?, Error?) -> Void) {
        guard context.realUserID == Int(owner), context.effectiveUserID == Int(owner), !modes.contains(.write) else { return reply(nil, POSIXError(.EACCES)) }
        logger.info("open cache mode: \(cacheMode.rawValue, privacy: .public)")
        reply(FSOpenItemResult(grantedCoherency: .noCache), nil)
    }
    func close(_ item: FSItem, context: FSContext, replyHandler reply: @escaping () -> Void) { reply() }
    func upgrade(_ item: FSItem, cacheMode: FSVolume.DataCacheMode, context: FSContext, replyHandler reply: @escaping (FSUpgradeItemResult?, Error?) -> Void) {
        guard context.realUserID == Int(owner), context.effectiveUserID == Int(owner) else { return reply(nil, POSIXError(.EACCES)) }
        logger.info("refusing cache upgrade: \(cacheMode.rawValue, privacy: .public)")
        reply(nil, POSIXError(.ENOTSUP))
    }
    func read(from item: FSItem, at offset: off_t, length: Int, into buffer: FSMutableFileDataBuffer, replyHandler reply: @escaping (Int, Error?) -> Void) {
        guard let item = item as? ProbeItem, item.number != 2, offset >= 0, length >= 0, length <= 1048576 else { return reply(0, POSIXError(.EINVAL)) }
        queue.async {
            guard self.active, self.pending < 64 else { return reply(0, POSIXError(.EACCES)) }
            let now = DispatchTime.now().uptimeNanoseconds
            if let deadline = self.deadlines[item.number] {
                guard now < deadline else { return reply(0, POSIXError(.EACCES)) }
                self.copy(offset, length, buffer, reply)
            } else {
                self.pending += 1
                self.queue.asyncAfter(deadline: .now() + 5) {
                    self.pending -= 1
                    guard self.active else { return reply(0, POSIXError(.EACCES)) }
                    // Concurrent waits share the first grant and never renew it.
                    if self.deadlines[item.number] == nil {
                        self.deadlines[item.number] = DispatchTime.now().uptimeNanoseconds + 3_000_000_000
                        DispatchQueue.global().asyncAfter(deadline: .now() + 3) {
                            // Outside the state queue: the kernel may call back into the module.
                            if let error = self.setCacheState(for: item, cacheMode: .none, coherencyType: .noCache, action: .revoke) {
                                self.logger.error("cache revoke failed: \(String(describing: error), privacy: .public)")
                                self.queue.async { self.active = false }
                            }
                        }
                    }
                    guard DispatchTime.now().uptimeNanoseconds < self.deadlines[item.number]! else { return reply(0, POSIXError(.EACCES)) }
                    self.copy(offset, length, buffer, reply)
                }
            }
        }
    }
    func copy(_ offset: off_t, _ length: Int, _ buffer: FSMutableFileDataBuffer, _ reply: (Int, Error?) -> Void) {
        let start = min(Int(offset), content.count), count = min(length, content.count - start)
        buffer.withUnsafeMutableBytes { destination in content.withUnsafeBytes { source in
            if count > 0 { destination.baseAddress!.copyMemory(from: source.baseAddress!.advanced(by: start), byteCount: count) }
        } }
        reply(count, nil)
    }
    func write(contents: Data, to item: FSItem, at offset: off_t, replyHandler reply: @escaping (Int, Error?) -> Void) { reply(0, POSIXError(.EROFS)) }
}
