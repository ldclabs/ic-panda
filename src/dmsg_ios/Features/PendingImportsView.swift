import SwiftUI

struct PendingImportsView: View {
    @Environment(SessionModel.self) private var model
    @State private var pending: [PrivateFiles.PendingImport] = []
    @State private var discarding: PrivateFiles.PendingImport?
    @State private var loading = true
    @State private var failure: String?
    @State private var visible = true
    var body: some View {
        List {
            TransferStatusView()
            if loading { ProgressView("Loading…") }
            if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
            ForEach(pending) { record in
                VStack(alignment: .leading, spacing: 8) {
                    Text(record.name).font(.headline)
                    if record.ready && record.purpose == "file" {
                        Button("Continue import") { Task { await resume(record) } }.disabled(model.transfer.running)
                    } else if record.ready && record.purpose == "vault" {
                        NavigationLink("Continue password import") { VaultImportView(pending: record) }
                    } else { Text("The source copy was interrupted. Discard it and select the source again.").font(.footnote).foregroundStyle(.secondary) }
                    Button("Discard staged copy", role: .destructive) { discarding = record }.disabled(model.transfer.running)
                }
            }
            if !loading && failure == nil && pending.isEmpty { ContentUnavailableView("No interrupted imports", systemImage: "checkmark.circle") }
        }.privateScreen().navigationTitle("Pending imports").task { await load() }
            .refreshable { await load() }
            .onAppear { visible = true }.onDisappear { visible = false }
            .confirmationDialog("Discard this staged copy?", isPresented: Binding(get: { discarding != nil }, set: { if !$0 { discarding = nil } })) {
                Button("Discard", role: .destructive) { if let record = discarding { Task {
                    do { try model.files?.discard(record); await load(); await model.loadFileSummary() } catch { model.reportFileError(error) }
                } } }
            }
    }
    private func load() async {
        loading = true; failure = nil; defer { loading = false }
        guard let files = model.files, let account = model.session?.account else { return }; let epoch = model.epoch
        do {
            let values = try await Task.detached { try files.pending(account: account) }.value
            guard epoch == model.epoch, !Task.isCancelled else { return }; pending = values
        } catch { if epoch == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }
    private func resume(_ record: PrivateFiles.PendingImport) async {
        guard let files = model.files, let engine = model.engine, model.session?.account == record.account else { return }
        let epoch = model.epoch
        do {
            _ = try await model.transfer.run(record.name) { try await engine.importFile(name: record.handle, folder: record.folder, sharedId: record.sharedId, operation: $0) }
            try files.finishImport(files.staging.appending(path: record.handle))
            if let id = record.sharedId, let store = try? IncomingShares(), let shared = try? store.list().first(where: { $0.id == id }) { try? store.remove(shared) }
            guard epoch == model.epoch else { return }; await load(); await model.loadFiles()
        } catch { if epoch == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }
}

struct TransferStatusView: View {
    @Environment(SessionModel.self) private var model
    var body: some View {
        if let label = model.transfer.label {
            VStack(alignment: .leading, spacing: 8) {
                Text(label).font(.subheadline).lineLimit(2)
                if let phase = model.transfer.phase { Text(phase).font(.footnote).foregroundStyle(.secondary) }
                if model.transfer.total > 0 { ProgressView(value: Double(model.transfer.completed), total: Double(model.transfer.total)) }
                else { ProgressView() }
                Button("Pause transfer") { model.transfer.cancel() }
            }
        }
    }
}
