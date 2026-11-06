import SwiftUI
@preconcurrency import DMsgCore

struct VaultTrashView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var items: [VaultItem] = []
    @State private var more = false
    @State private var loading = true
    @State private var failed = false
    @State private var restoring: String?
    var body: some View {
        List {
            ForEach(items, id: \.id) { item in
                HStack {
                    NavigationLink { VaultDetailView(id: item.id) } label: { VaultRow(item: item) }
                    if restoring == item.id { ProgressView() }
                    else { Button("Restore") { Task { await restore(item) } }.buttonStyle(.bordered).disabled(restoring != nil) }
                }
            }
            if loading { ProgressView() }
            else if failed { Button("Retry") { Task { await load(reset: true) } } }
            else if items.isEmpty { ContentUnavailableView("Trash is empty", systemImage: "trash") }
            if more { LoadMoreRow(working: loading, trigger: items.count) { await load(reset: false) } }
            Text("Moving to Trash does not free cloud storage.").font(.footnote).foregroundStyle(.secondary)
        }.privateScreen().navigationTitle("Trash").task(id: model.contentRevision) { await load(reset: true) }
            .refreshable { await load(reset: true) }
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
    }

    private func restore(_ item: VaultItem) async {
        restoring = item.id; defer { restoring = nil }; let epoch = model.epoch
        do {
            try await model.trashVaultItem(item, restore: true)
            guard epoch == model.epoch else { return }
            model.notice = String(localized: "Saved on this device"); await load(reset: true)
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func load(reset: Bool) async {
        loading = true; failed = false; defer { loading = false }; let epoch = model.epoch
        do {
            #if DEBUG
            if model.uiTesting { items = model.fixtureVaultItems.filter(\.deleted); more = false; return }
            #endif
            guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
            let result = try await engine.browseVault(request: VaultBrowseRequest(query: "", kind: nil, favorites: false, tag: nil,
                sort: "updated", offset: reset ? 0 : UInt32(items.count), limit: 60, trash: true))
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let page: VaultBrowsePage = try model.decode(result)
            items = reset ? page.items.map(\.item) : items + page.items.map(\.item); more = page.hasMore
        } catch { if epoch == model.epoch && !Task.isCancelled { failed = true } }
    }
}
