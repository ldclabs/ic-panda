import Foundation
import Observation
@preconcurrency import DMsgCore

struct VaultEditorSession: Identifiable, Codable {
    var id: String
    var itemID: String?
    var draft: VaultDraft
    var initial: VaultDraft
    var revision: UInt64
    var active: Bool

    var input: VaultDraftInput {
        get throws {
            let encoder = JSONEncoder(); encoder.outputFormatting = .sortedKeys
            let form = try encoder.encode(Form(draft: draft, initial: initial))
            return VaultDraftInput(id: id, itemId: itemID, baseRevision: draft.baseRevision,
                revision: revision, form: String(decoding: form, as: UTF8.self), active: active)
        }
    }
    struct Form: Codable { var draft: VaultDraft; var initial: VaultDraft }
    struct Stored: Decodable {
        let id: String; let itemId: String?; let baseRevision: String?; let revision: UInt64; let form: String; let active: Bool
        func session() throws -> VaultEditorSession {
            let value = try JSONDecoder().decode(Form.self, from: Data(form.utf8))
            guard value.draft.baseRevision == baseRevision else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
            return VaultEditorSession(id: id, itemID: itemId, draft: value.draft, initial: value.initial, revision: revision, active: active)
        }
    }
}

/// Owns authored input independently of SwiftUI sheets and request cancellation.
@MainActor @Observable
final class VaultEditing {
    var presented: VaultEditorSession?
    private(set) var presenter: UUID?
    private(set) var failure: String?
    private var records: [String: VaultEditorSession] = [:]
    private var dirty: Set<String> = []
    private var loaded = false
    var ready: Bool { loaded }
    private var loading = false
    private var generation: UInt64 = 0
    private var writing: Task<Void, Never>?
    private var reading: Task<Void, Never>?
    #if DEBUG
    var usesFixtures = false
    private var fixtureRecords: [String: [String: VaultEditorSession]] = [:]
    #else
    let usesFixtures = false
    #endif

    var pendingInputs: [VaultDraftInput] { dirty.compactMap { try? records[$0]?.input } }

    #if DEBUG
    /// Presentation fixture for restoring a native checkpoint into a fresh UI owner.
    func installFixtureCheckpoint(_ input: VaultDraftInput, account: String) throws {
        usesFixtures = true
        fixtureRecords[account, default: [:]][input.id] = try VaultEditorSession.Stored(
            id: input.id, itemId: input.itemId, baseRevision: input.baseRevision,
            revision: input.revision, form: input.form, active: input.active).session()
    }
    #endif

    func prepareLockFixture(_ model: SessionModel) {
        #if DEBUG
        if usesFixtures, let account = model.session?.account {
            for id in dirty { fixtureRecords[account, default: [:]][id] = records[id] }
        }
        #endif
    }

    func suspend() { generation &+= 1; writing?.cancel(); writing = nil; reading?.cancel(); reading = nil; loaded = false; loading = false }

    func reset() { suspend(); records = [:]; dirty = []; presented = nil; presenter = nil; failure = nil }

    func open(_ target: VaultEditorTarget, model: SessionModel, presenter: UUID? = nil) async {
        await load(model, restore: false)
        guard model.unlocked, !model.concealed, loaded else { return }
        let entry: VaultEditorSession
        if var saved = records.values.first(where: { $0.itemID == target.item?.id }) {
            saved.active = true; saved.revision += 1; entry = saved
        } else {
            let form = VaultDraft(target: target)
            entry = VaultEditorSession(id: NativeApple.requestID(), itemID: target.item?.id, draft: form, initial: form, revision: 1, active: true)
        }
        records[entry.id] = entry; dirty.insert(entry.id); self.presenter = presenter; presented = entry
        write(model)
    }

    func update(_ draft: VaultDraft, id: String, model: SessionModel) {
        guard model.unlocked, var entry = records[id], entry.draft != draft else { return }
        entry.draft = draft; entry.revision += 1
        records[id] = entry; dirty.insert(id)
        if presented?.id == id { presented = entry }
        model.activity(); write(model)
    }

    func input(id: String) throws -> VaultDraftInput {
        guard let entry = records[id] else { throw CancellationError() }
        return try entry.input
    }

    func form(id: String) -> VaultDraft? { records[id]?.draft }

    func finished(_ id: String) {
        records[id] = nil; dirty.remove(id)
        if presented?.id == id { presented = nil; presenter = nil }
        #if DEBUG
        for account in fixtureRecords.keys { fixtureRecords[account]?[id] = nil }
        #endif
    }

    func discard(_ id: String, model: SessionModel) async throws {
        let scope = model.viewGeneration
        if !usesFixtures {
            guard let engine = model.engine else { throw CancellationError() }
            try await engine.discardVaultDraft(id: id)
        }
        guard scope == model.viewGeneration else { return }
        finished(id)
    }

    func load(_ model: SessionModel, restore: Bool = true) async {
        if let reading { await reading.value; return }
        let stamp = generation
        let task = Task { await read(model, restore: restore) }
        reading = task
        await task.value
        if stamp == generation { reading = nil }
    }

    private func read(_ model: SessionModel, restore: Bool) async {
        guard model.unlocked, !model.concealed, !loaded, !loading else { return }
        loading = true
        let stamp = generation, epoch = model.epoch
        defer { if generation == stamp { loading = false } }
        do {
            await flush(model)
            guard generation == stamp, model.epoch == epoch, dirty.isEmpty else { return }
            let values: [VaultEditorSession]
            #if DEBUG
            if usesFixtures { values = Array(fixtureRecords[model.session?.account ?? "", default: [:]].values) }
            else {
                guard let engine = model.engine else { return }
                let stored: [VaultEditorSession.Stored] = try model.decode(try await engine.vaultDrafts())
                values = try stored.map { try $0.session() }
            }
            #else
            guard let engine = model.engine else { return }
            let stored: [VaultEditorSession.Stored] = try model.decode(try await engine.vaultDrafts())
            values = try stored.map { try $0.session() }
            #endif
            guard generation == stamp, model.epoch == epoch, !Task.isCancelled else { return }
            records = records.filter { dirty.contains($0.key) }
            for value in values where !dirty.contains(value.id) { records[value.id] = value }
            if let id = presented?.id, records[id] == nil { presented = nil }
            loaded = true; failure = nil
            if restore, presented == nil { presenter = nil; presented = values.first(where: \.active) }
        } catch {
            if generation == stamp, model.epoch == epoch, !Task.isCancelled {
                failure = vaultReadError(error); model.report(error)
            }
        }
    }

    private func write(_ model: SessionModel) {
        guard writing == nil, !dirty.isEmpty else { return }
        let stamp = generation, scope = model.viewGeneration
        writing = Task {
            defer { if stamp == generation { writing = nil } }
            do {
                try await Task.sleep(for: .milliseconds(200))
                while stamp == generation, scope == model.viewGeneration, !Task.isCancelled,
                      let id = dirty.first, let entry = records[id] {
                    let accepted: Bool
                    #if DEBUG
                    if usesFixtures {
                        let account = model.session?.account ?? ""
                        if entry.active { for key in fixtureRecords[account, default: [:]].keys { fixtureRecords[account]?[key]?.active = false } }
                        fixtureRecords[account, default: [:]][id] = entry; accepted = true
                    }
                    else {
                        guard let engine = model.engine else { return }
                        accepted = try await engine.checkpointVaultDraft(draft: entry.input)
                    }
                    #else
                    guard let engine = model.engine else { return }
                    accepted = try await engine.checkpointVaultDraft(draft: entry.input)
                    #endif
                    guard stamp == generation, scope == model.viewGeneration else { return }
                    if !accepted { finished(id) }
                    else if records[id]?.revision == entry.revision { dirty.remove(id) }
                    failure = nil
                }
            } catch { if stamp == generation, !Task.isCancelled { failure = vaultReadError(error) } }
        }
    }

    func flush(_ model: SessionModel) async { write(model); await writing?.value }
}
