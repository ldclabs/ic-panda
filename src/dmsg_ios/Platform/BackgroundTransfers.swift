import CryptoKit
import Foundation

struct CipherTransfer: Codable, Sendable, Identifiable {
    struct Part: Codable, Sendable { let slot: String; let size: UInt64; let digest: String; let url: URL }
    let id: String; let account: String; let device: String; let method: String
    let uploadId: String; let expiresAt: UInt64; let parts: [Part]
}
struct CipherTransferState: Codable, Sendable {
    enum Phase: Equatable, Sendable {
        case transferring, paused, failed(String), awaitingVerification
    }
    let transfer: CipherTransfer
    private(set) var phase: Phase = .transferring
    private(set) var completed: Set<String> = []
    private(set) var attempt = 0
    private(set) var failures: [String: Int]?
    private(set) var retryAfter: [String: Date]?

    init(transfer: CipherTransfer) { self.transfer = transfer }

    var ready: Bool { phase == .awaitingVerification }
    var failure: String? { if case .failed(let reason) = phase { reason } else { nil } }

    func accepts(_ attempt: Int) -> Bool { self.attempt == attempt && phase == .transferring }

    mutating func pause() { if phase == .transferring { phase = .paused } }

    mutating func retry(now: Date = .now) throws {
        guard Double(transfer.expiresAt) > now.timeIntervalSince1970 * 1000 else { throw CocoaError(.userCancelled) }
        attempt += 1; failures = nil; retryAfter = nil
        phase = completed.count == transfer.parts.count ? .awaitingVerification : .transferring
    }

    mutating func complete(_ slot: String) {
        guard phase == .transferring else { return }
        completed.insert(slot); retryAfter?[slot] = nil; failures?[slot] = nil
        if completed.count == transfer.parts.count { phase = .awaitingVerification }
    }

    mutating func fail(_ reason: String) {
        guard phase == .transferring else { return }
        phase = .failed(reason)
    }

    mutating func failed(slot: String, status: Int?, transient: Bool, now: Date = .now) {
        guard phase == .transferring else { return }
        if now.timeIntervalSince1970 * 1000 >= Double(transfer.expiresAt) || status == 403 {
            fail("expired"); return
        }
        let count = (failures?[slot] ?? 0) + 1
        if failures == nil { failures = [:] }; failures?[slot] = count
        if transient, count <= 3 {
            if retryAfter == nil { retryAfter = [:] }
            retryAfter?[slot] = now.addingTimeInterval(Double(1 << count))
        } else { fail("transfer_failed") }
    }

    // Keep the existing on-disk shape: installed versions may already have a
    // paused or partially completed OS transfer. Runtime stages have one owner.
    private enum CodingKeys: String, CodingKey { case transfer, completed, failure, paused, attempt, failures, retryAfter }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        transfer = try values.decode(CipherTransfer.self, forKey: .transfer)
        completed = try values.decode(Set<String>.self, forKey: .completed)
        attempt = try values.decode(Int.self, forKey: .attempt)
        failures = try values.decodeIfPresent([String: Int].self, forKey: .failures)
        retryAfter = try values.decodeIfPresent([String: Date].self, forKey: .retryAfter)
        let failure = try values.decodeIfPresent(String.self, forKey: .failure)
        let paused = try values.decode(Bool.self, forKey: .paused)
        if completed.count == transfer.parts.count { phase = .awaitingVerification }
        else if let failure { phase = .failed(failure) }
        else { phase = paused ? .paused : .transferring }
    }

    func encode(to encoder: Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(transfer, forKey: .transfer)
        try values.encode(completed, forKey: .completed)
        try values.encode(attempt, forKey: .attempt)
        try values.encode(phase == .paused, forKey: .paused)
        try values.encodeIfPresent(failure, forKey: .failure)
        try values.encodeIfPresent(failures, forKey: .failures)
        try values.encodeIfPresent(retryAfter, forKey: .retryAfter)
    }
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
        guard (try index.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) <= 2 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
        let value = try JSONDecoder().decode(CipherTransferState.self, from: Data(contentsOf: index)); try validate(value.transfer); return value
    }
    func snapshot() throws -> CipherTransferState? { try lock.withLock { try read() } }
    private func save(_ state: CipherTransferState) throws {
        try JSONEncoder().encode(state).write(to: index, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }
    func validate(_ transfer: CipherTransfer) throws {
        let hex: (String) -> Bool = { $0.range(of: "^[0-9a-f]{64}$", options: .regularExpression) != nil }
        guard hex(transfer.id), hex(transfer.uploadId), ["GET","PUT"].contains(transfer.method), !transfer.parts.isEmpty, transfer.parts.count <= 1001,
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
            guard var value = try read(), value.transfer.id == id, value.accepts(attempt),
                  let part = value.transfer.parts.first(where: { $0.slot == slot }),
                  (try temporary.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) == Int(part.size) else { throw CocoaError(.userCancelled) }
            let bytes = try Data(contentsOf: temporary); try stage(value.transfer, slot: slot, bytes: bytes)
            value.complete(slot); try save(value)
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
    private enum Scheduling { case idle, running, rerun }
    private var scheduling = Scheduling.idle
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
        if let state = try? store?.snapshot() { try? store?.update(state.transfer.id) { $0.pause() } }
        let tasks = await session.allTasks; for task in tasks { task.cancel() }; changed()
    }
    func retry() async throws {
        await pause()
        guard let store, let state = try store.snapshot(), Double(state.transfer.expiresAt) > Date.now.timeIntervalSince1970 * 1000 else { throw CocoaError(.userCancelled) }
        try store.update(state.transfer.id) { try $0.retry() }; schedule(); changed()
    }
    func discard(_ transfer: CipherTransfer) async throws { await pause(); try store?.discard(transfer); changed() }
    private func changed() { NotificationCenter.default.post(name: .dmsgCipherTransfer, object: nil) }
    private func taskPart(_ task: URLSessionTask) -> (String, Int, String)? {
        let values = (task.taskDescription ?? "").split(separator: ":"); guard values.count == 3, let attempt = Int(values[1]) else { return nil }; return (String(values[0]), attempt, String(values[2]))
    }
    private func schedule() {
        // A completion that arrives mid-pass may be missing from its task list;
        // run once more so the last finished task always schedules the rest.
        guard mutex.withLock({ if scheduling != .idle { scheduling = .rerun; return false }; scheduling = .running; return true }) else { return }
        session.getAllTasks { [self] tasks in
            defer {
                let again = mutex.withLock { () -> Bool in let again = scheduling == .rerun; scheduling = .idle; return again }
                if again { schedule() }
            }
            guard let store, let state = try? store.snapshot(), state.phase == .transferring else { return }
            guard Double(state.transfer.expiresAt) > Date.now.timeIntervalSince1970 * 1000 else {
                try? store.update(state.transfer.id) { $0.fail("expired") }; changed(); return
            }
            let active = tasks.filter { $0.state != .completed && $0.state != .canceling }
            let scheduled = Set(active.compactMap { taskPart($0) }.filter { $0.0 == state.transfer.id && $0.1 == state.attempt }.map { $0.2 })
            let next = state.transfer.parts.filter { !state.completed.contains($0.slot) && !scheduled.contains($0.slot) }.prefix(max(0, 3-active.count))
            for part in next {
                var request = URLRequest(url: part.url); request.httpMethod = state.transfer.method
                request.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
                let task: URLSessionTask
                if state.transfer.method == "PUT" {
                    guard let path = try? store.path(state.transfer, part.slot), FileManager.default.fileExists(atPath: path.path) else { try? store.update(state.transfer.id) { $0.fail("missing_ciphertext") }; changed(); return }
                    task = session.uploadTask(with: request, fromFile: path)
                } else { task = session.downloadTask(with: request) }
                task.earliestBeginDate = state.retryAfter?[part.slot]
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
            guard let response = downloadTask.response as? HTTPURLResponse, (200..<300).contains(response.statusCode) else { return }
            guard let store else { throw CocoaError(.fileReadUnknown) }
            // Consume the temporary URL before returning to URLSession.
            try store.receive(id, attempt: attempt, slot: slot, temporary: location)
        } catch { try? store?.update(id) { if $0.accepts(attempt) { $0.fail("download_verification_failed") } }; changed() }
    }
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard let (id,attempt,slot) = taskPart(task) else { return }
        try? store?.update(id) { value in
            guard value.accepts(attempt) else { return }
            let status = (task.response as? HTTPURLResponse)?.statusCode
            if error != nil || !(status.map { (200..<300).contains($0) } ?? false) {
                let network = (error as? URLError).map { [.timedOut, .networkConnectionLost, .notConnectedToInternet, .cannotConnectToHost, .cannotFindHost, .dnsLookupFailed].contains($0.code) } ?? false
                value.failed(slot: slot, status: status, transient: network || [408, 429, 500, 502, 503, 504].contains(status ?? 0))
            } else {
                if value.transfer.method == "PUT" { value.complete(slot) }
            }
        }
        changed(); schedule()
    }
    func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        Task { @MainActor in DMsgAppDelegate.finishBackgroundEvents() }
    }
}
