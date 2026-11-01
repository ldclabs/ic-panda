import Foundation
import Observation
@preconcurrency import DMsgCore

/// Session-owned delivery. Views persist the intent first, then hand it here.
/// Channel navigation does not cancel work; a session/lifecycle change does.
@MainActor @Observable
final class MessageSending {
    private var generation: UInt64 = 0
    private var jobs: [String: Task<Void, Never>] = [:]
    private var tails: [String: (id: String, task: Task<Void, Never>)] = [:]
    private var pending: [String: [OutgoingMessage]] = [:]
    private var confirmed: [String: [(id: String, message: MessageItem)]] = [:]
    private(set) var active: Set<String> = []
    private(set) var revision: UInt64 = 0
    private(set) var lastConfirmation: (channel: String, at: ContinuousClock.Instant)?

    func contains(_ id: String) -> Bool { jobs[id] != nil }

    func items(channel: String, stored: [OutgoingMessage], received: Set<String> = []) -> [OutgoingMessage] {
        let local = pending[channel] ?? []
        let known = Set(local.map(\.id))
        let completed = Set((confirmed[channel] ?? []).map(\.id)).union(received)
        return (stored.filter { !known.contains($0.id) } + local).filter { !completed.contains($0.id) }
    }

    func messages(channel: String, stored: [MessageItem]) -> [MessageItem] {
        var rows = Dictionary(stored.map { ($0.sequence, $0) }, uniquingKeysWith: { _, next in next })
        for result in confirmed[channel] ?? [] {
            rows[result.message.sequence] = result.message
        }
        return rows.values.sorted { $0.sequence < $1.sequence }
    }

    /// A local snapshot started after the ACK owns these rows again, including
    /// any newer edits. A snapshot started before the ACK cannot replace it.
    func loaded(channel: String, items: [MessageItem], since: UInt64) {
        guard since == revision else { return }
        let sequences = Set(items.map(\.sequence))
        confirmed[channel]?.removeAll { sequences.contains($0.message.sequence) }
    }

    func start(channel: String, item: OutgoingMessage,
               work: @escaping @MainActor () async throws -> MessageItem,
               finished: @escaping @MainActor (Error?) -> Void = { _ in }) {
        guard jobs[item.id] == nil else { return }
        let current = generation, previous = tails[channel]?.task
        replace(channel: channel, item: item, attempted: item.attempted, error: nil)
        let task = Task(priority: .userInitiated) { [weak self] in
            await previous?.value
            guard let self, self.generation == current, !Task.isCancelled else { return }
            defer {
                if self.generation == current {
                    self.active.remove(item.id)
                    self.jobs.removeValue(forKey: item.id)
                    if self.tails[channel]?.id == item.id { self.tails.removeValue(forKey: channel) }
                }
            }
            self.active.insert(item.id)
            self.replace(channel: channel, item: item, attempted: true, error: nil)
            var failure: Error?
            do {
                let message = try await work()
                guard self.generation == current, !Task.isCancelled else { return }
                self.lastConfirmation = (channel, .now)
                var results = self.confirmed[channel] ?? []
                results.removeAll { $0.id == item.id || $0.message.sequence == message.sequence }
                results.append((item.id, message))
                self.confirmed[channel] = Array(results.suffix(100))
                self.revision &+= 1
                self.pending[channel]?.removeAll { $0.id == item.id }
            } catch {
                guard self.generation == current, !Task.isCancelled else { return }
                failure = error
                let code: String
                if let failure = error as? MobileError, case .Failure(let value) = failure { code = value }
                else { code = "UNAVAILABLE" }
                self.replace(channel: channel, item: item, attempted: true, error: code)
            }
            finished(failure)
        }
        jobs[item.id] = task
        tails[channel] = (item.id, task)
    }

    func reset() {
        generation &+= 1
        revision &+= 1
        for job in jobs.values { job.cancel() }
        jobs = [:]; tails = [:]; pending = [:]; confirmed = [:]; active = []; lastConfirmation = nil
    }

    private func replace(channel: String, item: OutgoingMessage, attempted: Bool, error: String?) {
        let next = OutgoingMessage(id: item.id, text: item.text, attachmentName: item.attachmentName,
                                   attempted: attempted, error: error)
        var rows = pending[channel] ?? []
        if let index = rows.firstIndex(where: { $0.id == item.id }) { rows[index] = next }
        else { rows.append(next) }
        pending[channel] = rows
    }
}
