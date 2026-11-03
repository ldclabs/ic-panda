import Foundation
import Observation
@preconcurrency import DMsgCore

struct LetterDraft: Codable, Identifiable, Equatable {
    var id = NativeApple.requestID()
    var recipient = ""
    var text = ""
    var invitation: String?
    var updatedAt: UInt64 = 0
    var empty: Bool { recipient.isEmpty && text.isEmpty && (invitation ?? "").isEmpty }
    var invitationValid: Bool { invitation?.isEmpty != false || invitation?.range(of: "^[0-9a-f]{64}$", options: .regularExpression) != nil }
    var valid: Bool {
        validMessageAccount(recipient) && !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        && text.utf8.count <= 6000 && invitationValid
    }
}

struct SentLetter: Decodable, Identifiable {
    let id: String
    let recipient: String
    let text: String?
    let createdAt: UInt64?
    let state: String
    let error: String?
    let pending: Bool
    let editable: Bool
}

func validMessageAccount(_ value: String) -> Bool {
    value.range(of: "^[0-9a-v]{19}[0g]$", options: .regularExpression) != nil
}

func letterStateTitle(_ state: String) -> String {
    switch state {
    case "visible": String(localized: "Accepted by service")
    case "queued": String(localized: "Waiting to submit")
    case "unconfirmed": String(localized: "Waiting for confirmation")
    case "failed": String(localized: "Send failed")
    case "aborted", "refunded", "expired": String(localized: "Not delivered")
    case "quoted": String(localized: "Payment required")
    case "admitted", "settling": String(localized: "Processing")
    default: String(localized: "Check delivery status")
    }
}

/// Drafts and delivery belong to the unlocked account, not a navigation page.
@MainActor @Observable
final class LetterWorkspace {
    var drafts: [String: LetterDraft] = [:]
    private(set) var history: [SentLetter] = []
    private(set) var dirty: Set<String> = []
    private(set) var working: Set<String> = []
    private(set) var loading = false
    private(set) var failed = false
    private(set) var failures: [String: String] = [:]
    private var writes: [String: Task<Void, Never>] = [:]
    private var sends: [String: Task<Void, Never>] = [:]
    private var generation: UInt64 = 0
    private(set) var revision: UInt64 = 0
    private var reload = false

    var pendingInputs: [LetterDraftInput] {
        drafts.values.filter { dirty.contains($0.id) }.map { LetterDraftInput(id: $0.id, recipient: $0.recipient, text: $0.text, invitation: $0.invitation) }
    }

    func suspend() {
        generation &+= 1
        for task in sends.values { task.cancel() }
        sends = [:]; working = []
        loading = false; reload = false
    }

    func reset() {
        generation &+= 1
        revision &+= 1
        for task in writes.values { task.cancel() }
        for task in sends.values { task.cancel() }
        writes = [:]; sends = [:]; drafts = [:]; history = []; dirty = []; working = []
        failures = [:]; loading = false; failed = false; reload = false
    }

    func update(_ draft: LetterDraft, model: SessionModel) {
        var draft = draft; draft.updatedAt = UInt64(Date.now.timeIntervalSince1970 * 1000)
        revision &+= 1
        drafts[draft.id] = draft; dirty.insert(draft.id); model.activity()
        let previous = writes[draft.id], stamp = generation
        writes[draft.id] = Task {
            await previous?.value
            guard !Task.isCancelled, stamp == generation else { return }
            await persist(draft, model: model)
        }
    }

    private func persist(_ draft: LetterDraft, model: SessionModel) async {
        let epoch = model.epoch
        do {
            if !model.uiTesting {
                guard let engine = model.engine else { return }
                try await engine.saveLetterDraft(id: draft.id, recipient: draft.recipient, text: draft.text, invitation: draft.invitation)
            }
            guard epoch == model.epoch else { return }
            if drafts[draft.id] == draft { dirty.remove(draft.id) }
            failures.removeValue(forKey: draft.id)
        } catch { if epoch == model.epoch { failures[draft.id] = InboxLoading.message(error) } }
    }

    func flush(_ model: SessionModel) async {
        for id in Array(dirty) {
            let previous = writes[id], stamp = generation
            // Join the same write queue so an older autosave cannot follow Done.
            let task = Task {
                await previous?.value
                guard !Task.isCancelled, stamp == generation, dirty.contains(id), let draft = drafts[id] else { return }
                await persist(draft, model: model)
            }
            writes[id] = task
            await task.value
        }
    }

    func load(_ model: SessionModel) async {
        guard model.unlocked else { return }
        guard !loading else { reload = true; return }
        if model.uiTesting { return }
        guard let engine = model.engine else { return }
        let epoch = model.epoch, revision = revision; loading = true
        defer {
            if epoch == model.epoch {
                loading = false
                if reload { reload = false; Task { await self.load(model) } }
            }
        }
        do {
            let saved: [LetterDraft] = try model.decode(try await engine.letterDrafts())
            let sent: [SentLetter] = try model.decode(try await engine.letterHistory())
            guard epoch == model.epoch, !Task.isCancelled else { return }
            accept(saved: saved, sent: sent, startedAt: revision)
        } catch { if epoch == model.epoch, !Task.isCancelled { failed = true } }
    }

    /// A read begun before typing or queueing cannot restore an older editor.
    func accept(saved: [LetterDraft], sent: [SentLetter], startedAt: UInt64) {
        if startedAt == revision {
            var next = Dictionary(uniqueKeysWithValues: saved.map { ($0.id, $0) })
            for id in dirty { if let draft = drafts[id] { next[id] = draft } }
            drafts = next
        }
        history = sent; failed = false
    }

    func discard(_ id: String, model: SessionModel) async -> Bool {
        let epoch = model.epoch
        await writes[id]?.value
        do {
            if !model.uiTesting { try await model.engine?.discardLetterDraft(id: id) }
            guard epoch == model.epoch else { return false }
            revision &+= 1
            drafts.removeValue(forKey: id); dirty.remove(id); failures.removeValue(forKey: id)
            return true
        } catch { if epoch == model.epoch { failures[id] = InboxLoading.message(error) }; return false }
    }

    func send(_ draft: LetterDraft?, id: String, model: SessionModel) {
        guard !working.contains(id), model.unlocked else { return }
        let epoch = model.epoch, stamp = generation, engine = model.engine
        working.insert(id); failures.removeValue(forKey: id)
        sends[id] = Task {
            defer { if stamp == generation { working.remove(id); sends.removeValue(forKey: id) } }
            await writes[id]?.value
            guard !Task.isCancelled, epoch == model.epoch else { return }
            do {
                #if DEBUG
                if model.uiTesting {
                    if let draft {
                        history.insert(SentLetter(id: id, recipient: draft.recipient, text: draft.text, createdAt: UInt64(Date.now.timeIntervalSince1970 * 1000), state: "visible", error: nil, pending: false, editable: false), at: 0)
                        revision &+= 1; drafts.removeValue(forKey: id); dirty.remove(id)
                    }
                    return
                }
                #endif
                guard let engine else { return }
                if let draft {
                    try await engine.queueFreeLetter(id: id, recipient: draft.recipient, text: draft.text, invitation: draft.invitation?.isEmpty == false ? draft.invitation : nil)
                    guard epoch == model.epoch else { return }
                    revision &+= 1; drafts.removeValue(forKey: id); dirty.remove(id)
                }
                await load(model)
                _ = try await coreOperation { try await engine.sendFreeLetter(id: id, operation: $0) }
            } catch { if epoch == model.epoch, !Task.isCancelled { failures[id] = InboxLoading.message(error) } }
            if epoch == model.epoch, !Task.isCancelled { await load(model) }
        }
    }

    func check(_ id: String, model: SessionModel) async {
        guard !working.contains(id), let engine = model.engine else { return }
        let epoch = model.epoch; working.insert(id); defer { if epoch == model.epoch { working.remove(id) } }
        do {
            _ = try await coreOperation { try await engine.refreshSentLetter(id: id, operation: $0) }
            if epoch == model.epoch { failures.removeValue(forKey: id); await load(model) }
        } catch { if epoch == model.epoch, !Task.isCancelled { failures[id] = InboxLoading.message(error) } }
    }

    func edit(_ id: String, model: SessionModel) async -> LetterDraft? {
        guard !working.contains(id), let engine = model.engine else { return nil }
        let epoch = model.epoch; working.insert(id); defer { if epoch == model.epoch { working.remove(id) } }
        do {
            try await engine.editUnsentLetter(id: id)
            guard epoch == model.epoch else { return nil }
            await load(model); return drafts[id]
        } catch { if epoch == model.epoch { failures[id] = InboxLoading.message(error) }; return nil }
    }
}
