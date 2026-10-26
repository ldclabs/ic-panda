import CryptoKit
import Foundation

struct CipherTransfer: Codable, Sendable, Identifiable {
    struct Part: Codable, Sendable { let slot: String; let size: UInt64; let digest: String; let url: URL }
    let id: String; let account: String; let device: String; let method: String
    let uploadId: String; let expiresAt: UInt64; let parts: [Part]
}
struct CipherTransferState: Codable, Sendable {
    let transfer: CipherTransfer
    var completed: Set<String> = []
    var failure: String?
    var paused = false
    var attempt = 0
    var ready: Bool { completed.count == transfer.parts.count }
}

/// This directory holds only opaque transfer metadata and ciphertext. It is
/// separate from the Complete-protected workspace and excluded from backups.
final class CipherTransferStore: @unchecked Sendable {
    let root: URL
    private let lock = NSLock()
    init(root: URL? = nil) throws {
        self.root = try root ?? FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true).appending(path: "dMsg-cipher-transfers")
        try FileManager.default.createDirectory(at: self.root, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
        var url = self.root, flags = URLResourceValues(); flags.isExcludedFromBackup = true; try url.setResourceValues(flags)
    }
    private var index: URL { root.appending(path: "transfer.json") }
    private func read() throws -> CipherTransferState? {
        guard FileManager.default.fileExists(atPath: index.path) else { return nil }
        guard (try index.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) <= 128 * 1024 else { throw CocoaError(.fileReadTooLarge) }
        let value = try JSONDecoder().decode(CipherTransferState.self, from: Data(contentsOf: index)); try validate(value.transfer); return value
    }
    func snapshot() throws -> CipherTransferState? { try lock.withLock { try read() } }
    private func save(_ state: CipherTransferState) throws {
        try JSONEncoder().encode(state).write(to: index, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }
    func validate(_ transfer: CipherTransfer) throws {
        let hex: (String) -> Bool = { $0.range(of: "^[0-9a-f]{64}$", options: .regularExpression) != nil }
        guard hex(transfer.id), hex(transfer.uploadId), ["GET","PUT"].contains(transfer.method), !transfer.parts.isEmpty, transfer.parts.count <= 101,
              Set(transfer.parts.map(\.slot)).count == transfer.parts.count,
              transfer.parts.allSatisfy({ part in
                  let validSlot = part.slot == "manifest" || part.slot.range(of: "^(0|[1-9][0-9]{0,2})$", options: .regularExpression) != nil
                  return validSlot && part.size <= 1024 * 1024 + 64 && hex(part.digest) && part.url.scheme == "https" && part.url.user == nil && part.url.password == nil && part.url.fragment == nil
              }) else { throw CocoaError(.fileReadCorruptFile) }
    }
    func path(_ transfer: CipherTransfer, _ slot: String) throws -> URL {
        try validate(transfer); guard transfer.parts.contains(where: { $0.slot == slot }) else { throw CocoaError(.fileReadNoSuchFile) }
        let directory = root.appending(path: transfer.id), file = directory.appending(path: slot)
        guard directory.resolvingSymlinksInPath().standardizedFileURL.path == root.resolvingSymlinksInPath().appending(path: transfer.id).standardizedFileURL.path,
              file.resolvingSymlinksInPath().deletingLastPathComponent().standardizedFileURL.path == directory.resolvingSymlinksInPath().standardizedFileURL.path else { throw CocoaError(.fileReadNoPermission) }
        return file
    }
    func stage(_ transfer: CipherTransfer, slot: String, bytes: Data) throws {
        guard let part = transfer.parts.first(where: { $0.slot == slot }), bytes.count == Int(part.size), Self.digest(bytes) == part.digest else { throw CocoaError(.fileReadCorruptFile) }
        let path = try path(transfer, slot)
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
        try bytes.write(to: path, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }
    func start(_ transfer: CipherTransfer) throws {
        try lock.withLock {
            guard try read() == nil else { throw CocoaError(.fileWriteFileExists) }
            try validate(transfer)
            guard Double(transfer.expiresAt) > Date.now.timeIntervalSince1970 * 1000 else { throw CocoaError(.userCancelled) }
            try save(CipherTransferState(transfer: transfer))
        }
    }
    func update(_ id: String, _ work: (inout CipherTransferState) throws -> Void) throws {
        try lock.withLock { guard var value = try read(), value.transfer.id == id else { return }; try work(&value); try save(value) }
    }
    func receive(_ id: String, attempt: Int, slot: String, temporary: URL) throws {
        try lock.withLock {
            guard var value = try read(), value.transfer.id == id, value.attempt == attempt, !value.paused,
                  let part = value.transfer.parts.first(where: { $0.slot == slot }),
                  (try temporary.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) == Int(part.size) else { throw CocoaError(.userCancelled) }
            let bytes = try Data(contentsOf: temporary); try stage(value.transfer, slot: slot, bytes: bytes)
            value.completed.insert(slot); try save(value)
        }
    }
    func discard(_ transfer: CipherTransfer) throws {
        try lock.withLock {
            if let current = try read(), current.transfer.id == transfer.id { try FileManager.default.removeItem(at: index) }
            // Derived from a validated ID, never a URL supplied by an extension.
            if let part = transfer.parts.first {
                let directory = try path(transfer, part.slot).deletingLastPathComponent()
                if FileManager.default.fileExists(atPath: directory.path) { try FileManager.default.removeItem(at: directory) }
            }
        }
    }
    static func digest(_ bytes: Data) -> String { SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined() }
}

extension Notification.Name { static let dmsgCipherTransfer = Notification.Name("dmsg.cipher-transfer") }

/// URLSession's delegate uses a serial queue; mutable scheduling is separately
/// protected. At most three system tasks exist for one user-selected transfer.
final class BackgroundTransfers: NSObject, URLSessionDownloadDelegate, URLSessionTaskDelegate, @unchecked Sendable {
    static let shared = BackgroundTransfers()
    static let identifier = "net.dmsg.ios.ciphertext.v1"
    private let mutex = NSLock()
    private var connection: URLSession?
    private var scheduling = false
    private var rescheduling = false
    let store: CipherTransferStore?
    override init() { store = try? CipherTransferStore(); super.init() }
    private var session: URLSession {
        mutex.withLock {
            if let connection { return connection }
            let config = URLSessionConfiguration.background(withIdentifier: Self.identifier)
            config.sessionSendsLaunchEvents = true; config.isDiscretionary = false; config.waitsForConnectivity = true
            config.httpMaximumConnectionsPerHost = 3; config.timeoutIntervalForResource = 86_400
            config.httpShouldSetCookies = false; config.httpCookieStorage = nil; config.urlCredentialStorage = nil
            let queue = OperationQueue(); queue.maxConcurrentOperationCount = 1
            let created = URLSession(configuration: config, delegate: self, delegateQueue: queue); connection = created; return created
        }
    }
    func reconnect() { _ = session; schedule() }
    func start(_ transfer: CipherTransfer) throws {
        guard let store else { throw CocoaError(.fileWriteNoPermission) }
        try store.start(transfer); schedule(); changed()
    }
    func pause() async {
        if let state = try? store?.snapshot() { try? store?.update(state.transfer.id) { $0.paused = true } }
        let tasks = await session.allTasks; for task in tasks { task.cancel() }; changed()
    }
    func retry() throws {
        guard let store, let state = try store.snapshot(), Double(state.transfer.expiresAt) > Date.now.timeIntervalSince1970 * 1000 else { throw CocoaError(.userCancelled) }
        try store.update(state.transfer.id) { $0.failure = nil; $0.paused = false; $0.attempt += 1 }; schedule(); changed()
    }
    func discard(_ transfer: CipherTransfer) async throws { await pause(); try store?.discard(transfer); changed() }
    private func changed() { NotificationCenter.default.post(name: .dmsgCipherTransfer, object: nil) }
    private func taskPart(_ task: URLSessionTask) -> (String, Int, String)? {
        let values = (task.taskDescription ?? "").split(separator: ":"); guard values.count == 3, let attempt = Int(values[1]) else { return nil }; return (String(values[0]), attempt, String(values[2]))
    }
    private func schedule() {
        // A completion that arrives mid-pass may be missing from its task list;
        // run once more so the last finished task always schedules the rest.
        guard mutex.withLock({ if scheduling { rescheduling = true; return false }; scheduling = true; return true }) else { return }
        session.getAllTasks { [self] tasks in
            defer {
                let again = mutex.withLock { () -> Bool in let again = rescheduling; scheduling = false; rescheduling = false; return again }
                if again { schedule() }
            }
            guard let store, let state = try? store.snapshot(), !state.paused, state.failure == nil, !state.ready else { return }
            guard Double(state.transfer.expiresAt) > Date.now.timeIntervalSince1970 * 1000 else {
                try? store.update(state.transfer.id) { $0.failure = "expired" }; changed(); return
            }
            let active = tasks.filter { $0.state != .completed && $0.state != .canceling }
            let scheduled = Set(active.compactMap { taskPart($0) }.filter { $0.0 == state.transfer.id && $0.1 == state.attempt }.map { $0.2 })
            let next = state.transfer.parts.filter { !state.completed.contains($0.slot) && !scheduled.contains($0.slot) }.prefix(max(0, 3-active.count))
            for part in next {
                var request = URLRequest(url: part.url); request.httpMethod = state.transfer.method
                request.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
                let task: URLSessionTask
                if state.transfer.method == "PUT" {
                    guard let path = try? store.path(state.transfer, part.slot), FileManager.default.fileExists(atPath: path.path) else { try? store.update(state.transfer.id) { $0.failure = "missing_ciphertext" }; changed(); return }
                    task = session.uploadTask(with: request, fromFile: path)
                } else { task = session.downloadTask(with: request) }
                task.taskDescription = state.transfer.id + ":" + String(state.attempt) + ":" + part.slot; task.resume()
            }
        }
    }
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        guard let (id,attempt,slot) = taskPart(downloadTask), let state = try? store?.snapshot(), state.transfer.id == id, state.attempt == attempt,
              let part = state.transfer.parts.first(where: { $0.slot == slot }), totalBytesWritten <= Int64(part.size), totalBytesExpectedToWrite <= Int64(part.size) else { downloadTask.cancel(); return }
    }
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        guard let (id,attempt,slot) = taskPart(downloadTask) else { return }
        do {
            guard let response = downloadTask.response as? HTTPURLResponse, (200..<300).contains(response.statusCode), let store else { throw CocoaError(.fileReadUnknown) }
            // Consume the temporary URL before returning to URLSession.
            try store.receive(id, attempt: attempt, slot: slot, temporary: location)
        } catch { try? store?.update(id) { if $0.attempt == attempt && !$0.paused { $0.failure = "download_verification_failed" } }; changed() }
    }
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard let (id,attempt,slot) = taskPart(task) else { return }
        try? store?.update(id) { value in
            guard !value.paused, value.attempt == attempt else { return }
            if error != nil || !((task.response as? HTTPURLResponse).map { (200..<300).contains($0.statusCode) } ?? false) { value.failure = "transfer_failed" }
            else if value.transfer.method == "PUT" { value.completed.insert(slot) }
        }
        changed(); schedule()
    }
    func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        Task { @MainActor in DMsgAppDelegate.finishBackgroundEvents() }
    }
}
