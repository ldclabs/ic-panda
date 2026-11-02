import Foundation
import Observation
@preconcurrency import DMsgCore

/// Account-owned work survives navigation. The scene stops it on lock/background;
/// the Rust journals resume exactly those requested transfers on the next unlock.
@MainActor @Observable
final class FileTransfers {
    struct Backup: Decodable, Identifiable {
        let file: String; let version: String; let uploadId: String
        let sent: UInt64; let chunks: UInt64; let stage: String
        let bytes: UInt64; let totalBytes: UInt64; let error: String?; let retryAt: UInt64?
        var id: String { uploadId }
    }
    struct Download: Decodable, Identifiable {
        let uploadId: String; let stage: String; let done: UInt64; let total: UInt64
        let bytes: UInt64; let totalBytes: UInt64; let error: String?; let retryAt: UInt64?
        var id: String { uploadId }
    }
    private(set) var backups: [Backup] = []
    private(set) var downloads: [Download] = []
    private var backupWork: Task<Void, Never>?
    private var downloadWork: Task<Void, Never>?
    private var handingOff = false
    private var generation: UInt64 = 0

    func stop() {
        generation &+= 1
        backupWork?.cancel(); downloadWork?.cancel()
        backupWork = nil; downloadWork = nil; backups = []; downloads = []
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
            backups = try model.decode(backup); downloads = try model.decode(download)
        } catch { /* Keep the last task state; a missing response is never success. */ }
    }

    func watch(_ model: SessionModel) async {
        #if DEBUG
        if model.uiTesting {
            if ProcessInfo.processInfo.arguments.contains("--ui-file-transfers") {
                let total: UInt64 = 150 * 1024 * 1024
                backups = [Backup(file: model.library.files.first?.id ?? "preview-file", version: "one", uploadId: "preview-backup", sent: 150, chunks: 150, stage: "committing", bytes: total, totalBytes: total, error: nil, retryAt: nil)]
                downloads = [Download(uploadId: "preview-download", stage: "verifying", done: 150, total: 150, bytes: total, totalBytes: total, error: nil, retryAt: nil)]
            }
            return
        }
        #endif
        stop()
        let stamp = generation
        defer { if generation == stamp { stop() } }
        while !Task.isCancelled, model.unlocked, !model.concealed, generation == stamp {
            await load(model)
            guard let engine = model.engine, !Task.isCancelled else { return }
            let now = UInt64(Date.now.timeIntervalSince1970 * 1000), epoch = model.epoch
            // An explicitly delegated OS transfer owns its upload until it is
            // finalized or cancelled; do not race it with foreground uploading.
            let backgroundUpload = (try? BackgroundTransfers.shared.store?.snapshot())?.transfer
            if !handingOff, backupWork == nil, backups.contains(where: { Self.due(error: $0.error, retryAt: $0.retryAt, now: now) }), backgroundUpload?.method != "PUT" {
                backupWork = Task {
                    do { _ = try await coreOperation { try await engine.runBackups(operation: $0) } }
                    catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
                    guard generation == stamp else { return }
                    backupWork = nil
                    if epoch == model.epoch, !Task.isCancelled { await model.refresh() }
                }
            }
            if !handingOff, downloadWork == nil, backgroundUpload?.method != "GET", downloads.contains(where: { $0.stage != "done" && Self.due(error: $0.error, retryAt: $0.retryAt, now: now) }) {
                downloadWork = Task {
                    do { _ = try await coreOperation { try await engine.runDownloads(operation: $0) } }
                    catch { if epoch == model.epoch, !Task.isCancelled { model.report(error) } }
                    guard generation == stamp else { return }
                    downloadWork = nil
                    if epoch == model.epoch, !Task.isCancelled { await model.loadCloudBackups(refresh: false); await model.refresh() }
                }
            }
            do { try await Task.sleep(for: .seconds(1)) } catch { return }
        }
    }
}
