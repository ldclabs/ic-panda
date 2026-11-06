import SwiftUI
@preconcurrency import DMsgCore

/// Names and avatars of the accounts on screen, shared by every view: a row never
/// starts blank, and a screen full of rows asks once.
@MainActor @Observable
final class PeopleDirectory {
    private(set) var people: [String: MessagePerson] = [:]
    private var images: [String: UIImage] = [:]
    @ObservationIgnored private var checked: [String: ContinuousClock.Instant] = [:]
    @ObservationIgnored private var waiting: Set<String> = []
    @ObservationIgnored private var worker: Task<Void, Never>?

    func avatar(_ account: String) -> UIImage? {
        people[account]?.avatarUrl.flatMap { images[$0] }
    }

    /// Accounts checked in the last ten minutes are left alone.
    func request(_ accounts: some Sequence<String>, model: SessionModel) {
        let now = ContinuousClock.now
        for account in accounts where validMessageAccount(account) {
            if checked[account].map({ $0.duration(to: now) >= .seconds(600) }) ?? true {
                checked[account] = now; waiting.insert(account)
            }
        }
        guard !waiting.isEmpty, worker == nil else { return }
        worker = Task { [weak self, weak model] in
            // Rows appearing together arrive as one batch.
            try? await Task.sleep(for: .milliseconds(80))
            if let self, let model { await self.drain(model) }
            self?.worker = nil
        }
    }

    /// A changed profile is read again the next time it is requested.
    func invalidate() { checked = [:] }

    func reset() {
        worker?.cancel(); worker = nil
        waiting = []; checked = [:]; people = [:]; images = [:]
    }

    private func drain(_ model: SessionModel) async {
        let epoch = model.epoch
        while !waiting.isEmpty {
            guard let engine = model.engine, epoch == model.epoch, !Task.isCancelled else { return forget() }
            let batch = Array(waiting.prefix(100))
            waiting.subtract(batch)
            for refresh in [false, true] {
                do {
                    let snapshot = try await coreOperation { try await engine.channelNames(accounts: batch, refresh: refresh, operation: $0) }
                    let names: [String: MessagePerson] = try model.decode(snapshot)
                    guard epoch == model.epoch, !Task.isCancelled else { return forget(batch) }
                    people.merge(names) { _, next in next }
                    var seen: Set<String> = []
                    for (account, person) in names {
                        guard let url = person.avatarUrl, seen.insert(url).inserted, refresh || images[url] == nil else { continue }
                        let bytes = try? await engine.channelAvatar(account: account, refresh: refresh)
                        let image = await AvatarImage.decode(bytes)
                        guard epoch == model.epoch, !Task.isCancelled else { return forget(batch) }
                        if refresh || image != nil { images[url] = image }
                    }
                } catch { /* Names and initials remain available while offline. */ }
            }
        }
    }

    /// Work a lock or a trip to the background cut short is asked for again.
    private func forget(_ batch: [String] = []) {
        for account in batch + Array(waiting) { checked[account] = nil }
        waiting = []
    }
}

extension SessionModel {
    func personName(_ account: String) -> String {
        people.people[account]?.name ?? accountIdentities[account]?.name ?? String(account.prefix(8))
    }
}
