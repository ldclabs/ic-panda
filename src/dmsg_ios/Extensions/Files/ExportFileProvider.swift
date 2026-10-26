import FileProvider
import UniformTypeIdentifiers

private func providerError(_ code: NSFileProviderError.Code) -> NSError { NSError(domain: NSFileProviderErrorDomain, code: code.rawValue) }

final class ExportFileItem: NSObject, NSFileProviderItem {
    let snapshot: String
    let file: FileExport?
    init(snapshot: String, file: FileExport?) { self.snapshot = snapshot; self.file = file }
    var itemIdentifier: NSFileProviderItemIdentifier { file.map { .init($0.id) } ?? .rootContainer }
    var parentItemIdentifier: NSFileProviderItemIdentifier { .rootContainer }
    var filename: String { file?.name ?? "dMsg" }
    var contentType: UTType { file.map { UTType(filenameExtension: ($0.name as NSString).pathExtension) ?? .data } ?? .folder }
    var capabilities: NSFileProviderItemCapabilities { file == nil ? [.allowsContentEnumerating, .allowsReading] : [.allowsReading] }
    var documentSize: NSNumber? { file.map { NSNumber(value: $0.size) } }
    var contentModificationDate: Date? { file.map { Date(timeIntervalSince1970: Double($0.modified) / 1000) } }
    var itemVersion: NSFileProviderItemVersion { NSFileProviderItemVersion(contentVersion: Data((file?.sha256 ?? snapshot).utf8), metadataVersion: Data(snapshot.utf8)) }
}

final class ExportEnumerator: NSObject, NSFileProviderEnumerator {
    func invalidate() {}
    func enumerateItems(for observer: any NSFileProviderEnumerationObserver, startingAt page: NSFileProviderPage) {
        do {
            let store = try SystemExports(.files), opened = try store.fileSession()
            let files = try store.fileMetadata(opened)
            observer.didEnumerate(files.map { ExportFileItem(snapshot: opened.header.id, file: $0) })
            observer.finishEnumerating(upTo: nil)
        } catch { observer.finishEnumeratingWithError(providerError(.notAuthenticated)) }
    }
    func enumerateChanges(for observer: any NSFileProviderChangeObserver, from anchor: NSFileProviderSyncAnchor) {
        do {
            let store = try SystemExports(.files), opened = try store.fileSession()
            let next = NSFileProviderSyncAnchor(Data(opened.header.id.utf8))
            guard anchor == next else { observer.finishEnumeratingWithError(providerError(.syncAnchorExpired)); return }
            observer.finishEnumeratingChanges(upTo: next, moreComing: false)
        } catch { observer.finishEnumeratingWithError(providerError(.notAuthenticated)) }
    }
    func currentSyncAnchor(completionHandler: @escaping (NSFileProviderSyncAnchor?) -> Void) {
        completionHandler((try? SystemExports(.files).header()).map { NSFileProviderSyncAnchor(Data($0.id.utf8)) })
    }
}

/// File Provider invokes its completion handlers across queues. This box owns
/// one callback until the bounded download worker calls it exactly once.
private final class FetchCompletion: @unchecked Sendable {
    let call: (URL?, (any NSFileProviderItem)?, Error?) -> Void
    init(_ call: @escaping (URL?, (any NSFileProviderItem)?, Error?) -> Void) { self.call = call }
}

final class ExportFileProvider: NSObject, NSFileProviderReplicatedExtension {
    private let domain: NSFileProviderDomain
    private let shutdown = Progress(totalUnitCount: 1)
    private let downloads: OperationQueue = { let queue = OperationQueue(); queue.maxConcurrentOperationCount = 2; return queue }()
    required init(domain: NSFileProviderDomain) {
        self.domain = domain
        super.init()
        // Recover only our own abandoned materializations after process death.
        if let directory = try? NSFileProviderManager(for: domain)?.temporaryDirectoryURL(),
           let files = try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) {
            for file in files where file.lastPathComponent.hasPrefix("dmsg-export-") { try? FileManager.default.removeItem(at: file) }
        }
    }
    func invalidate() { shutdown.cancel() }
    func item(for identifier: NSFileProviderItemIdentifier, request: NSFileProviderRequest, completionHandler: @escaping ((any NSFileProviderItem)?, Error?) -> Void) -> Progress {
        let progress = Progress(totalUnitCount: 1)
        do {
            let store = try SystemExports(.files), opened = try store.fileSession()
            if identifier == .rootContainer { completionHandler(ExportFileItem(snapshot: opened.header.id, file: nil), nil) }
            else if let file = try store.fileMetadata(opened).first(where: { $0.id == identifier.rawValue }) { completionHandler(ExportFileItem(snapshot: opened.header.id, file: file), nil) }
            else { completionHandler(nil, providerError(.noSuchItem)) }
        } catch { completionHandler(nil, providerError(.notAuthenticated)) }
        progress.completedUnitCount = 1; return progress
    }
    func fetchContents(for itemIdentifier: NSFileProviderItemIdentifier, version requestedVersion: NSFileProviderItemVersion?, request: NSFileProviderRequest, completionHandler: @escaping (URL?, (any NSFileProviderItem)?, Error?) -> Void) -> Progress {
        let progress = Progress(totalUnitCount: 1), callback = FetchCompletion(completionHandler)
        do {
            guard let directory = try NSFileProviderManager(for: domain)?.temporaryDirectoryURL() else { throw providerError(.cannotSynchronize) }
            let identifier = itemIdentifier.rawValue, version = requestedVersion?.contentVersion, shutdown = shutdown
            downloads.addOperation {
                do {
                    let store = try SystemExports(.files), opened: OpenExport
                    do { opened = try store.fileSession() } catch { throw providerError(.notAuthenticated) }
                    guard let file = try store.fileMetadata(opened).first(where: { $0.id == identifier }), version == nil || version == Data(file.sha256.utf8) else { throw providerError(.noSuchItem) }
                    let output = directory.appending(path: "dmsg-export-" + UUID().uuidString)
                    try store.materialize(file, opened: opened, output: output, cancelled: { progress.isCancelled || shutdown.isCancelled || (try? store.fileSession().header.id) != opened.header.id })
                    guard !progress.isCancelled, !shutdown.isCancelled, (try? store.fileSession().header.id) == opened.header.id else { try? FileManager.default.removeItem(at: output); throw CocoaError(.userCancelled) }
                    callback.call(output, ExportFileItem(snapshot: opened.header.id, file: file), nil)
                } catch { callback.call(nil, nil, error) }
                progress.completedUnitCount = 1
            }
        } catch { callback.call(nil, nil, error); progress.completedUnitCount = 1 }
        return progress
    }
    func enumerator(for containerItemIdentifier: NSFileProviderItemIdentifier, request: NSFileProviderRequest) throws -> any NSFileProviderEnumerator {
        guard containerItemIdentifier == .rootContainer || containerItemIdentifier == .workingSet else { throw providerError(.noSuchItem) }
        return ExportEnumerator()
    }
    func createItem(basedOn itemTemplate: any NSFileProviderItem, fields: NSFileProviderItemFields, contents url: URL?, options: NSFileProviderCreateItemOptions, request: NSFileProviderRequest, completionHandler: @escaping ((any NSFileProviderItem)?, NSFileProviderItemFields, Bool, Error?) -> Void) -> Progress {
        completionHandler(nil, fields, false, CocoaError(.fileWriteNoPermission)); return Progress(totalUnitCount: 0)
    }
    func modifyItem(_ item: any NSFileProviderItem, baseVersion version: NSFileProviderItemVersion, changedFields: NSFileProviderItemFields, contents newContents: URL?, options: NSFileProviderModifyItemOptions, request: NSFileProviderRequest, completionHandler: @escaping ((any NSFileProviderItem)?, NSFileProviderItemFields, Bool, Error?) -> Void) -> Progress {
        completionHandler(nil, changedFields, false, CocoaError(.fileWriteNoPermission)); return Progress(totalUnitCount: 0)
    }
    func deleteItem(identifier: NSFileProviderItemIdentifier, baseVersion version: NSFileProviderItemVersion, options: NSFileProviderDeleteItemOptions, request: NSFileProviderRequest, completionHandler: @escaping (Error?) -> Void) -> Progress {
        completionHandler(CocoaError(.fileWriteNoPermission)); return Progress(totalUnitCount: 0)
    }
}
