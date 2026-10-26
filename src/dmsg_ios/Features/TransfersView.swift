import SwiftUI

struct TransfersView: View {
    @Environment(SessionModel.self) private var model
    @State private var jobs: [Job] = []
    @State private var action: Task<Void, Never>?
    struct Job: Decodable, Identifiable {
        let file: String; let version: String; let sent: UInt64; let chunks: UInt64; let error: String?
        var id: String { file + ":" + version }
    }
    struct Run: Decodable { let pending: UInt64; let failed: UInt64; let paused: Bool }
    var body: some View {
        List {
            TransferStatusView()
            BackgroundTransferSection()
            NavigationLink("Shared with dMsg") { IncomingSharesView() }
            NavigationLink("Pending imports") { PendingImportsView() }
            Section("Pending backups") {
                ForEach(jobs) { job in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(job.file).font(.caption.monospaced()).lineLimit(1)
                        ProgressView(value: Double(job.sent), total: Double(max(1, job.chunks)))
                        Button("Continue this backup in background") { action = Task { await model.startBackgroundBackup(file: job.file, version: job.version) } }.disabled(model.busy)
                        if let error = job.error { Text(error).font(.caption).foregroundStyle(.secondary) }
                    }
                }
                if !jobs.isEmpty {
                    Button("Continue selected backups") { action = Task { await resume() } }.disabled(model.transfer.running)
                } else { Text("No pending backups").foregroundStyle(.secondary) }
            }
        }.privateScreen().navigationTitle("Transfers").task { await load() }.refreshable { await load() }
            .onDisappear { action?.cancel() }
    }
    private func load() async {
        do { if let result = try await model.engine?.backupStatus() { jobs = try model.decode(result) } }
        catch { model.report(error) }
    }
    private func resume() async {
        guard let engine = model.engine else { return }; let epoch = model.epoch
        do {
            try await model.transfer.run(String(localized: "Backing up…")) { token in
                repeat {
                    let result = try await engine.runBackups(operation: token)
                    guard epoch == model.epoch else { throw CancellationError() }
                    let progress: Run = try model.decode(result)
                    await load()
                    if progress.pending == 0 || progress.failed > 0 || !progress.paused { break }
                } while !Task.isCancelled && !model.concealed
            }
            if epoch == model.epoch { await model.refresh() }
        } catch { if epoch == model.epoch { model.report(error); await load() } }
    }
}
