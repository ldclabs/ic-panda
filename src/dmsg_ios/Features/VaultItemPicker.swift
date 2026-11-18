import SwiftUI
@preconcurrency import DMsgCore

@MainActor @Observable
final class VaultPickerSearch {
    enum Phase: Equatable { case waiting, reading(UUID), ready(hasMore: Bool), failed(String) }
    var query = "" { didSet { if query != oldValue { reset() } } }
    private(set) var items: [VaultItem] = []
    private(set) var phase = Phase.waiting

    var reading: Bool { if case .reading = phase { true } else { false } }
    var failure: String? { if case let .failed(message) = phase { message } else { nil } }

    func reset() {
        items = []; phase = .waiting
    }

    func load(_ read: (String, UInt32) async throws -> VaultPage) async {
        guard !Task.isCancelled, !reading, phase != .ready(hasMore: false) else { return }
        let previous = phase, request = Phase.reading(UUID())
        phase = request
        do {
            let page = try await read(query, UInt32(items.count))
            guard phase == request else { return }
            try Task.checkCancellation()
            items.append(contentsOf: page.items)
            phase = .ready(hasMore: page.hasMore)
        } catch {
            guard phase == request else { return }
            phase = Task.isCancelled || error is CancellationError ? previous : .failed(InboxLoading.message(error))
        }
    }
}

/// The picker owns search and pagination. Its caller owns the selection.
struct VaultItemPicker<Header: View, Row: View>: View {
    @Bindable var search: VaultPickerSearch
    let kind: String?
    let limit: UInt32
    var placement: SearchFieldPlacement = .automatic
    @ViewBuilder let header: () -> Header
    @ViewBuilder let row: (VaultItem) -> Row
    @Environment(SessionModel.self) private var model

    var body: some View {
        List {
            header()
            SettingsLoadStatus(loading: search.items.isEmpty && (search.phase == .waiting || search.reading),
                               failed: search.failure != nil, failureText: search.failure) { Task { await load() } }
            ForEach(search.items, id: \.id, content: row)
            if search.items.isEmpty && search.phase == .ready(hasMore: false) { ContentUnavailableView.search(text: search.query) }
            if search.phase == .ready(hasMore: true) || (search.reading && !search.items.isEmpty) {
                LoadMoreRow(working: search.reading, trigger: search.items.count) { await load() }
            }
        }
        .searchable(text: $search.query, placement: placement)
        .task(id: "\(model.epoch):\(model.concealed):\(search.query)") {
            search.reset()
            guard !model.concealed else { return }
            do { try await Task.sleep(for: .milliseconds(200)) } catch { return }
            await load()
        }
    }

    private func load() async {
        await search.load { query, offset in
            try await model.readVaultPickerPage(query: query, kind: kind, offset: offset, limit: limit)
        }
    }
}
