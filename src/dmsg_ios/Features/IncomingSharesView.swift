import SwiftUI

struct IncomingSharesView: View {
    @Environment(SessionModel.self) private var model
    @State private var entries: [IncomingShare] = []
    @State private var discarding: IncomingShare?
    @State private var working = false
    @State private var failed = false
    var body: some View {
        List {
            NoticeBar(text: String(localized: "Shared items are waiting on this device. Importing encrypts them into the currently selected account."))
            ForEach(entries) { entry in
                VStack(alignment: .leading, spacing: 8) {
                    Text(entry.name).font(.headline)
                    Text(ByteCountFormatter.string(fromByteCount: entry.size, countStyle: .file)).font(.caption).foregroundStyle(.secondary)
                    if entry.sha256.isEmpty {
                        Text("The source copy was interrupted. Discard it and select the source again.").font(.footnote).foregroundStyle(.secondary)
                    } else if entry.accountHash == nil || entry.accountHash == model.session?.account.map(IncomingShares.accountHash) {
                        Button("Import into this account") { Task { await receive(entry) } }.disabled(working)
                    } else { Text("Continue this import in the account you originally selected.").font(.footnote).foregroundStyle(.secondary) }
                    Button("Discard staged copy", role: .destructive) { discarding = entry }.disabled(working)
                }
            }
            if entries.isEmpty { Text(LocalizedStringKey(failed ? "Shared items are unavailable. Open the app again after unlocking this device." : "No shared items waiting")).foregroundStyle(.secondary) }
        }.privateScreen().navigationTitle("Shared with dMsg").task { await load() }.refreshable { await load() }
            .confirmationDialog("Discard this staged copy?", isPresented: Binding(get: { discarding != nil }, set: { if !$0 { discarding = nil } })) {
                Button("Discard", role: .destructive) { if let discarding { Task { do { try IncomingShares().remove(discarding); await load() } catch { model.report(error) } } } }
            }
    }
    private func load() async {
        do { entries = try await Task.detached { try IncomingShares().list() }.value; failed = false }
        catch { failed = true }
    }
    private func receive(_ entry: IncomingShare) async {
        guard let account = model.session?.account, let paths = model.files, let engine = model.engine, !working else { return }
        let epoch = model.epoch, folder = model.filesFolder
        working = true; defer { working = false }
        do {
            let storage = try IncomingShares()
            var claimed = entry; claimed.accountHash = IncomingShares.accountHash(account); try storage.write(claimed)
            let staged = try await Task.detached {
                let source = try storage.file(entry)
                let staged = try paths.importDocument(source, account: account, folder: folder, sharedId: entry.id)
                guard try IncomingShares.digest(staged) == entry.sha256 else { try? paths.finishImport(staged); throw CocoaError(.fileReadCorruptFile) }
                return staged
            }.value
            guard epoch == model.epoch else { return }
            let handle = account + "/" + staged.deletingLastPathComponent().lastPathComponent + "/" + staged.lastPathComponent
            _ = try await model.transfer.run(entry.name) { try await engine.importFile(name: handle, folder: folder, sharedId: entry.id, operation: $0) }
            try paths.finishImport(staged); try storage.remove(entry)
            if epoch == model.epoch { await load(); await model.refresh() }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
