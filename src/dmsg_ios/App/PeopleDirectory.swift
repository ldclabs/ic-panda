import CryptoKit
import SwiftUI
@preconcurrency import DMsgCore

/// Names and avatars of the accounts on screen, shared by every view: a row never
/// starts blank, and a screen full of rows asks once.
@MainActor @Observable
final class PeopleDirectory {
    typealias NameReader = @MainActor ([String], Bool) async throws -> [String: MessagePerson]
    typealias AvatarReader = @MainActor (String, String, Bool) async throws -> Data?

    private struct AvatarRequest {
        let account: String
        let read: AvatarReader
    }

    private(set) var people: [String: MessagePerson] = [:]
    private var images: [String: UIImage] = [:]
    @ObservationIgnored private var imageDigests: [String: SHA256.Digest] = [:]
    @ObservationIgnored private var checked: [String: ContinuousClock.Instant] = [:]
    @ObservationIgnored private var waiting: Set<String> = []
    @ObservationIgnored private var worker: Task<Void, Never>?
    @ObservationIgnored private var avatarWaiting: [String: AvatarRequest] = [:]
    @ObservationIgnored private var avatarTasks: [String: Task<Void, Never>] = [:]
    @ObservationIgnored private var avatarAfter: [String: ContinuousClock.Instant] = [:]

    func avatar(_ account: String) -> UIImage? {
        people[account]?.avatarUrl.flatMap { images[$0] }
    }

    /// Accounts checked in the last ten minutes are left alone.
    func request(_ accounts: some Sequence<String>, model: SessionModel) {
        let epoch = model.epoch
        request(accounts, names: { [weak model] batch, refresh in
            guard let model, model.epoch == epoch, !model.concealed, let engine = model.engine else { throw CancellationError() }
            let snapshot = try await coreOperation { try await engine.channelNames(accounts: batch, refresh: refresh, operation: $0) }
            guard model.epoch == epoch, !model.concealed else { throw CancellationError() }
            return try model.decode(snapshot)
        }, avatar: { [weak model] account, url, refresh in
            guard let model, model.epoch == epoch, !model.concealed, let engine = model.engine else { throw CancellationError() }
            let bytes = try await engine.channelAvatarForUrl(account: account, url: url, refresh: refresh)
            guard model.epoch == epoch, !model.concealed else { throw CancellationError() }
            return bytes
        })
    }

    func request(_ accounts: some Sequence<String>, names readNames: @escaping NameReader, avatar readAvatar: @escaping AvatarReader) {
        request(accounts) { [weak self] batch in
            var completed = false
            for refresh in [false, true] {
                guard !Task.isCancelled else { return false }
                do {
                    let names = try await readNames(batch, refresh)
                    guard let self, !Task.isCancelled else { return false }
                    if names.contains(where: { people[$0.key] != $0.value }) {
                        people.merge(names) { _, fresh in fresh }
                    }
                    enqueueAvatars(names, read: readAvatar)
                    completed = true
                } catch { /* Keep published names available while offline. */ }
            }
            return completed
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
    func invalidate() { checked = [:]; avatarAfter = [:] }

    func suspend() {
        worker?.cancel(); worker = nil
        avatarTasks.values.forEach { $0.cancel() }
        avatarTasks = [:]; avatarWaiting = [:]
        waiting = []; checked = [:]
    }

    func reset() {
        suspend()
        avatarAfter = [:]; people = [:]; images = [:]; imageDigests = [:]
    }

    private func avatarOwner(for url: String, preferred: String) -> String? {
        if people[preferred]?.avatarUrl == url { return preferred }
        return people.first { $0.value.avatarUrl == url }?.key
    }

    private func enqueueAvatars(_ names: [String: MessagePerson], read: @escaping AvatarReader) {
        let now = ContinuousClock.now
        avatarAfter = avatarAfter.filter { $0.value > now }
        for (account, person) in names {
            guard let url = person.avatarUrl, avatarAfter[url] == nil, avatarTasks[url] == nil else { continue }
            avatarWaiting[url] = AvatarRequest(account: account, read: read)
        }
        startAvatars()
    }

    private func startAvatars() {
        while avatarTasks.count < 4, let (url, request) = avatarWaiting.first {
            avatarWaiting.removeValue(forKey: url)
            guard let owner = avatarOwner(for: url, preferred: request.account) else { continue }
            let active = AvatarRequest(account: owner, read: request.read)
            avatarTasks[url] = Task { [weak self] in
                guard let self else { return }
                await loadAvatar(url, request: active)
                guard !Task.isCancelled else { return }
                avatarTasks.removeValue(forKey: url)
                startAvatars()
            }
        }
    }

    private func loadAvatar(_ url: String, request: AvatarRequest) async {
        func activeOwner() -> String? {
            Task.isCancelled ? nil : avatarOwner(for: url, preferred: request.account)
        }
        guard let initialOwner = activeOwner() else { return }
        if images[url] == nil {
            let cached: Data?
            do {
                cached = try await request.read(initialOwner, url, false)
            } catch is CancellationError {
                return
            } catch {
                cached = nil
            }
            guard activeOwner() != nil else { return }
            if let cached {
                let digest = SHA256.hash(data: cached)
                let image = await AvatarImage.decode(cached)
                guard activeOwner() != nil else { return }
                if let image {
                    images[url] = image
                    imageDigests[url] = digest
                }
            }
        }
        guard let refreshOwner = activeOwner() else { return }
        do {
            let fresh = try await request.read(refreshOwner, url, true)
            guard activeOwner() != nil else { return }
            if let fresh {
                let digest = SHA256.hash(data: fresh)
                if images[url] == nil || imageDigests[url] != digest {
                    let image = await AvatarImage.decode(fresh)
                    guard activeOwner() != nil else { return }
                    images[url] = image
                    imageDigests[url] = image == nil ? nil : digest
                }
            } else if images[url] != nil {
                images[url] = nil
                imageDigests[url] = nil
            }
        } catch is CancellationError {
            return
        } catch { /* Keep a decoded image available when the refresh is offline. */ }
        if activeOwner() != nil {
            avatarAfter[url] = ContinuousClock.now.advanced(by: .seconds(images[url] == nil ? 60 : 600))
        }
    }
}

extension SessionModel {
    func personName(_ account: String) -> String {
        people.people[account]?.name ?? accountIdentities[account]?.name ?? String(account.prefix(8))
    }
}
