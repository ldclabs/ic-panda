import SwiftUI
@preconcurrency import DMsgCore

struct BackgroundTransferSection: View {
    @Environment(SessionModel.self) private var model
    @State private var state: CipherTransferState?
    @State private var working = false
    var body: some View {
        Section("Background ciphertext transfer") {
            Text("Only the selected encrypted file can transfer while dMsg is locked. iOS controls scheduling; force quitting stops delivery until you open the app again. Open the original account to verify and finish.").font(.footnote).foregroundStyle(.secondary)
            if let state {
                if state.transfer.account == model.session?.account {
                    Text(LocalizedStringKey(state.transfer.method == "PUT" ? "Uploading encrypted backup" : "Downloading encrypted backup"))
                    ProgressView(value: Double(state.completed.count), total: Double(state.transfer.parts.count))
                    if state.ready { Button("Verify and finish") { Task { await finish(state) } }.disabled(working) }
                    else if Double(state.transfer.expiresAt) <= Date.now.timeIntervalSince1970 * 1000 {
                        Text("The transfer authorization expired. Cancel this task, then continue the original backup in the foreground or start a new download.").font(.footnote)
                    } else if state.paused || state.failure != nil {
                        Text("The transfer paused. Retry the original encrypted blocks.").font(.footnote)
                        Button("Retry") { do { try BackgroundTransfers.shared.retry(); load() } catch { model.report(error) } }.disabled(working)
                    } else { Button("Pause transfer") { Task { await BackgroundTransfers.shared.pause(); load() } } }
                    Button("Cancel background transfer", role: .destructive) { Task { await cancel(state) } }.disabled(working)
                } else {
                    Text("A background transfer belongs to another account. Switch to that account to finish or cancel it.").font(.footnote)
                    Button("Stop this device's transfer", role: .destructive) { Task {
                        do { try await BackgroundTransfers.shared.discard(state.transfer); load() } catch { model.report(error) }
                    } }
                }
            } else { Text("No background transfer").foregroundStyle(.secondary) }
            if working { ProgressView() }
        }.task { load() }.onReceive(NotificationCenter.default.publisher(for: .dmsgCipherTransfer).receive(on: RunLoop.main)) { _ in load() }
    }
    private func load() { state = try? BackgroundTransfers.shared.store?.snapshot() }
    private func cancel(_ state: CipherTransferState) async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false; load() }
        await BackgroundTransfers.shared.pause()
        do { try await engine.backgroundRevoke(id: state.transfer.id); try await BackgroundTransfers.shared.discard(state.transfer) }
        catch { model.report(error) }
    }
    private func finish(_ state: CipherTransferState) async {
        guard let engine = model.engine, let store = BackgroundTransfers.shared.store, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false; load() }
        do {
            if state.transfer.method == "GET" {
                for part in state.transfer.parts {
                    guard epoch == model.epoch, !Task.isCancelled else { return }
                    let path = try store.path(state.transfer, part.slot)
                    let bytes = try await Task.detached { () -> Data in
                        guard (try path.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) <= 1024 * 1024 + 64 else { throw CocoaError(.fileReadTooLarge) }
                        return try Data(contentsOf: path)
                    }.value
                    try await engine.backgroundAcceptPart(id: state.transfer.id, slot: part.slot, bytes: bytes)
                }
            }
            try await coreOperation { try await engine.backgroundFinish(id: state.transfer.id, operation: $0) }
            try await BackgroundTransfers.shared.discard(state.transfer)
            guard epoch == model.epoch else { return }; await model.refresh(); await model.loadCloudBackups(refresh: true)
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

extension SessionModel {
    func startBackgroundBackup(file: String, version: String) async {
        guard let engine, !busy else { return }; let current = epoch, work = beginBusyOperation(); defer { finishBusyOperation(work) }
        do {
            guard try BackgroundTransfers.shared.store?.snapshot() == nil else { throw CocoaError(.fileWriteFileExists) }
            let result = try await coreOperation { try await engine.backgroundBackup(file: file, version: version, operation: $0) }
            try await stageBackground(result, engine: engine, epoch: current)
        } catch { if current == epoch { report(error) } }
    }
    func startBackgroundDownload(upload: String) async {
        guard let engine, !busy else { return }; let current = epoch, work = beginBusyOperation(); defer { finishBusyOperation(work) }
        do {
            guard try BackgroundTransfers.shared.store?.snapshot() == nil else { throw CocoaError(.fileWriteFileExists) }
            let result = try await coreOperation { try await engine.backgroundDownload(upload: upload, folder: filesFolder, operation: $0) }
            try await stageBackground(result, engine: engine, epoch: current)
        } catch { if current == epoch { report(error) } }
    }
    private func stageBackground(_ result: ObjectSnapshot, engine: MobileEngine, epoch: UInt64) async throws {
        let transfer: CipherTransfer = try decode(result)
        guard let store = BackgroundTransfers.shared.store, transfer.account == session?.account, self.epoch == epoch else { throw CancellationError() }
        var published = false; defer { if !published { try? store.discard(transfer) } }
        if let available = try store.root.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage,
           available < Int64(transfer.parts.reduce(UInt64(0)) { $0 + $1.size }) + 8 * 1024 * 1024 { throw CocoaError(.fileWriteOutOfSpace) }
        if transfer.method == "PUT" {
            for part in transfer.parts {
                let bytes = try await engine.backgroundUploadPart(id: transfer.id, slot: part.slot)
                guard self.epoch == epoch, !Task.isCancelled else { throw CancellationError() }
                try await Task.detached { try store.stage(transfer, slot: part.slot, bytes: bytes) }.value
            }
        }
        guard self.epoch == epoch, !Task.isCancelled else { throw CancellationError() }
        try BackgroundTransfers.shared.start(transfer); published = true
        notice = String(localized: "Background transfer started. Review its progress in Transfers.")
    }
    func cancelBackgroundForAccount(_ account: String) async {
        guard let state = try? BackgroundTransfers.shared.store?.snapshot(), state.transfer.account == account else { return }
        await BackgroundTransfers.shared.pause()
        try? await engine?.backgroundRevoke(id: state.transfer.id)
        do { try await BackgroundTransfers.shared.discard(state.transfer) } catch { report(error) }
    }
}
