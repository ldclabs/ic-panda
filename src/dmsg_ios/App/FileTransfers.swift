import Foundation
import Observation
@preconcurrency import DMsgCore

/// Account-owned work survives navigation. The scene stops it on lock/background;
/// the Rust journals resume exactly those requested transfers on the next unlock.
@MainActor @Observable
final class FileTransfers {
    private struct Pass: Decodable { let completed: UInt64 }
    struct Backup: Decodable, Identifiable {
        let file: String; let version: String; let uploadId: String
        let sent: UInt64; let chunks: UInt64; let stage: String
        let bytes: UInt64; let totalBytes: UInt64; let error: String?; let retryAt: UInt64?
        var name: String? = nil
        var id: String { uploadId }
    }
    struct Download: Decodable, Identifiable {
        let uploadId: String; let stage: String; let done: UInt64; let total: UInt64
        let bytes: UInt64; let totalBytes: UInt64; let error: String?; let retryAt: UInt64?
        var name: String? = nil
        var id: String { uploadId }
    }
    private(set) var backups: [Backup] = []
    private(set) var downloads: [Download] = []
    private(set) var loadFailed = false
    var activeCount: Int { backups.count + downloads.filter { $0.stage != "done" }.count }
    private var backupWork: Task<Void, Never>?
    private var downloadWork: Task<Void, Never>?
    private var handingOff = false
    private var generation: UInt64 = 0

    func stop() {
        generation &+= 1
        backupWork?.cancel(); downloadWork?.cancel()
        backupWork = nil; downloadWork = nil; backups = []; downloads = []; loadFailed = false
    }

    func beginHandoff() async {
        handingOff = true
        let a = backupWork, b = downloadWork
        a?.cancel(); b?.cancel()
        await a?.value; await b?.value
        backupWork = nil; downloadWork = nil
    }

    func endHandoff() { handingOff = false }

    static func due(error: String?, retryAt: UInt64?, now: UInt64) -> Bool {
        error == nil || retryAt.map { $0 <= now } == true
    }

    func load(_ model: SessionModel) async {
        guard let engine = model.engine, model.unlocked, !model.concealed, !model.uiTesting else { return }
        let epoch = model.epoch
        do {
            async let a = engine.backupStatus()
            async let b = engine.downloadStatus()
            let (backup, download) = try await (a, b)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            backups = try model.decode(backup); downloads = try model.decode(download); loadFailed = false
        } catch { if epoch == model.epoch, !Task.isCancelled { loadFailed = true } }
    }

    func watch(_ model: SessionModel) async {
        #if DEBUG
        if model.uiTesting {
            if ProcessInfo.processInfo.arguments.contains("--ui-file-transfers") {
                let total: UInt64 = 150 * 1024 * 1024
                backups = [Backup(file: model.library.files.first?.id ?? "preview-file", version: "one", uploadId: "preview-backup", sent: 150, chunks: 150, stage: "committing", bytes: total, totalBytes: total, error: nil, retryAt: nil, name: "Project archive.zip")]
                downloads = [Download(uploadId: "preview-download", stage: "verifying", done: 150, total: 150, bytes: total, totalBytes: total, error: nil, retryAt: nil, name: "Project brief.pdf")]
            }
            return
        }
        #endif
        stop()
        let stamp = generation
        defer { if generation == stamp { stop() } }
        while !Task.isCancelled, model.unlocked, generation == stamp {
            // The scene can become active before foreground() finishes resuming
            // the core. Keep the watcher alive until the privacy shield clears.
            if model.concealed {
                do { try await Task.sleep(for: .seconds(1)) } catch { return }
                continue
            }
            await load(model)
            guard let engine = model.engine, !Task.isCancelled else { return }
            if model.concealed { continue }
            let now = UInt64(Date.now.timeIntervalSince1970 * 1000), epoch = model.epoch
            // An explicitly delegated OS transfer owns its upload until it is
            // finalized or cancelled; do not race it with foreground uploading.
            let backgroundUpload = (try? BackgroundTransfers.shared.store?.snapshot())?.transfer
            if !handingOff, backupWork == nil, backups.contains(where: { Self.due(error: $0.error, retryAt: $0.retryAt, now: now) }), backgroundUpload?.method != "PUT" {
                backupWork = Task {
                    var completed = false
                    do {
                        let result = try await coreOperation { try await engine.runBackups(operation: $0) }
                        let pass: Pass = try model.decode(result); completed = pass.completed > 0
                    }
                    catch { if epoch == model.epoch { _ = model.fileFailureMessage(error) } }
                    guard generation == stamp else { return }
                    backupWork = nil
                    if epoch == model.epoch, !Task.isCancelled {
                        await model.loadFiles()
                        if completed { await model.loadCloudBackups(refresh: true) }
                    }
                }
            }
            if !handingOff, downloadWork == nil, backgroundUpload?.method != "GET", downloads.contains(where: { $0.stage != "done" && Self.due(error: $0.error, retryAt: $0.retryAt, now: now) }) {
                downloadWork = Task {
                    do { _ = try await coreOperation { try await engine.runDownloads(operation: $0) } }
                    catch { if epoch == model.epoch { _ = model.fileFailureMessage(error) } }
                    guard generation == stamp else { return }
                    downloadWork = nil
                    if epoch == model.epoch, !Task.isCancelled { await model.loadCloudBackups(refresh: false); await model.loadFiles() }
                }
            }
            do { try await Task.sleep(for: .seconds(1)) } catch { return }
        }
    }
}
