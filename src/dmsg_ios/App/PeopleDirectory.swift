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
        request(accounts) { [weak self, weak model] batch in
            guard let self, let model, !model.concealed, let engine = model.engine else { return false }
            return await self.read(batch, engine: engine, model: model)
        }
    }

    func request(_ accounts: some Sequence<String>, read: @escaping @MainActor ([String]) async -> Bool) {
        let now = ContinuousClock.now
        for account in accounts where validMessageAccount(account) {
            if checked[account].map({ $0.duration(to: now) >= .seconds(600) }) ?? true {
                checked[account] = now; waiting.insert(account)
            }
        }
        guard !waiting.isEmpty, worker == nil else { return }
        worker = Task { [weak self] in
            do { try await Task.sleep(for: .milliseconds(80)) } catch { return }
            guard let self else { return }
            while !waiting.isEmpty {
                let batch = Array(waiting.prefix(100))
                waiting.subtract(batch)
                let completed = await read(batch)
                // reset() already released this worker and its queue.
                guard !Task.isCancelled else { return }
                if !completed { for account in batch { checked.removeValue(forKey: account) } }
            }
            worker = nil
        }
    }

    /// A changed profile is read again the next time it is requested.
    func invalidate() { checked = [:] }

    func reset() {
        worker?.cancel(); worker = nil
        waiting = []; checked = [:]; people = [:]; images = [:]
    }

    private func read(_ batch: [String], engine: MobileEngine, model: SessionModel) async -> Bool {
        let epoch = model.epoch
        var completed = false
        for refresh in [false, true] {
            guard epoch == model.epoch, !Task.isCancelled else { return false }
            do {
                let snapshot = try await coreOperation { try await engine.channelNames(accounts: batch, refresh: refresh, operation: $0) }
                let names: [String: MessagePerson] = try model.decode(snapshot)
                guard epoch == model.epoch, !Task.isCancelled else { return false }
                for (account, person) in names where people[account] != person { people[account] = person }
                completed = true
                var seen: Set<String> = []
                for (account, person) in names {
                    guard epoch == model.epoch, !Task.isCancelled else { return false }
                    guard let url = person.avatarUrl, seen.insert(url).inserted, refresh || images[url] == nil else { continue }
                    do {
                        let bytes = try await engine.channelAvatar(account: account, refresh: refresh)
                        let image = await AvatarImage.decode(bytes)
                        guard epoch == model.epoch, !Task.isCancelled else { return false }
                        if refresh || image != nil { images[url] = image }
                    } catch { /* Keep the saved image when the refresh is offline. */ }
                }
            } catch { /* Keep published names available while offline. */ }
        }
        return completed
    }
}

extension SessionModel {
    func personName(_ account: String) -> String {
        people.people[account]?.name ?? accountIdentities[account]?.name ?? String(account.prefix(8))
    }
}
