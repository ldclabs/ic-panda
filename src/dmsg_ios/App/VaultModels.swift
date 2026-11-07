import Foundation
@preconcurrency import DMsgCore

struct VaultSummary: Decodable {
    let id: String
    let revision: String
    let kind: String
    let title: String
    let subtitle: String
    let favorite: Bool
    let tags: [String]
    let synced: Bool
    let conflict: Bool
    let reserved: Bool
    let deleted: Bool
    let createdAt: UInt64
    let updatedAt: UInt64

    var item: VaultItem {
        VaultItem(id: id, revision: revision, kind: kind, title: title, subtitle: subtitle,
            notes: "", favorite: favorite, tags: tags, synced: synced, conflict: conflict,
            reserved: reserved, files: [], fields: [], deleted: deleted, createdAt: createdAt, updatedAt: updatedAt)
    }
}

struct VaultBrowsePage: Decodable {
    let items: [VaultSummary]
    let total: Int
    let all: Int
    let favorites: Int
    let kinds: [String: Int]
    let tags: [String]
    let hasMore: Bool
}

struct VaultCounts: Equatable {
    var total = 0
    var all = 0
    var favorites = 0
    var kinds: [String: Int] = [:]
    var tags: [String] = []

    func count(_ filter: String) -> Int {
        filter == "all" ? all : filter == "favorites" ? favorites : kinds[filter, default: 0]
    }
}

/// An expiry belongs to the displayed value, not to the next copy/reveal task.
struct RevealedVaultValue {
    let text: String
    let expires: Date?

    func value(at now: Date) -> String? { expires.map { now >= $0 } == true ? nil : text }
    func remaining(at now: Date) -> Int? { expires.map { max(0, Int(ceil($0.timeIntervalSince(now)))) } }
}

extension VaultItem {
    var changeToken: String { "\(revision):\(synced):\(conflict):\(deleted)" }
    var canEdit: Bool { !reserved && !deleted }

    func replacing(favorite: Bool? = nil, deleted: Bool? = nil, revision: String? = nil, fields: [VaultField]? = nil, title: String? = nil) -> VaultItem {
        VaultItem(id: id, revision: revision ?? self.revision, kind: kind, title: title ?? self.title, subtitle: subtitle,
            notes: notes, favorite: favorite ?? self.favorite, tags: tags, synced: synced, conflict: conflict,
            reserved: reserved, files: files, fields: fields ?? self.fields, deleted: deleted ?? self.deleted, createdAt: createdAt, updatedAt: updatedAt)
    }
}

extension SessionModel {
    func readVaultItem(id: String, revision: String? = nil) async throws -> VaultItem {
        #if DEBUG
        if uiTesting {
            scheduleVaultFixtureRefresh(id: id)
            guard let value = fixtureVaultItems.first(where: { $0.id == id }) else { throw MobileError.Failure(code: "NOT_FOUND") }
            let item = revision.flatMap { wanted in fixtureVaultHistory[id]?.first { $0.revision == wanted } } ?? value
            return item.replacing(fields: item.fields.map { field in
                VaultField(id: field.id, kind: field.kind, label: field.label, section: field.section,
                    value: field.concealed && !field.value.isEmpty ? "••••••••" : field.value, concealed: field.concealed)
            })
        }
        #endif
        guard let engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        let result = try await engine.vaultItem(id: id, revision: revision)
        guard result.generation == session?.generation else { throw CancellationError() }
        return result.item
    }

    /// A field's real value, read only when the person asks for it.
    func vaultFieldValue(_ item: VaultItem, field: VaultField) async throws -> SecretValue {
        #if DEBUG
        if uiTesting {
            fixtureVaultFieldReadAttempts += 1
            let attempt = fixtureVaultFieldReadAttempts
            if ProcessInfo.processInfo.arguments.contains("--ui-vault-field-slow") { try await Task.sleep(for: .seconds(2)) }
            if ProcessInfo.processInfo.arguments.contains("--ui-vault-field-failure") && attempt == 1 { throw MobileError.Failure(code: "UNAVAILABLE") }
            let secret = item.id == "fixture-api" ? "L0ng_API-key_0123456789_ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz\nSecond-line_0O1Il_0123456789" : "fixture-secret"
            return SecretValue(generation: 0, value: field.kind == "otp" ? "123456" : field.concealed ? secret : field.value,
                remainingMs: field.kind == "otp" ? (ProcessInfo.processInfo.arguments.contains("--ui-vault-long-code") ? 6000 : 2000) : nil)
        }
        #endif
        guard let engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        return try await engine.revealField(id: item.id, revision: item.revision, field: field.id)
    }

    /// Copies a login's username or password from the list, where only summaries are loaded.
    func copyVaultField(_ summary: VaultItem, field id: String) async {
        let current = epoch
        do {
            let item = try await readVaultItem(id: summary.id)
            guard current == epoch, let field = item.fields.first(where: { $0.id == id }), !field.value.isEmpty else {
                if current == epoch { notice = String(localized: "No fields or notes") }
                return
            }
            let value = try await vaultFieldValue(item, field: field)
            guard current == epoch, value.generation == session?.generation, !concealed else { return }
            clipboard.copy(value.value)
            notice = String(localized: "Copied for 30 seconds")
        } catch { if current == epoch { report(error) } }
    }

    func setVaultFavorite(_ item: VaultItem) async throws {
        let current = epoch
        #if DEBUG
        if uiTesting {
            if let index = fixtureVaultItems.firstIndex(where: { $0.id == item.id }) {
                let value = fixtureVaultItems[index]
                fixtureVaultItems[index] = value.replacing(favorite: !value.favorite, revision: value.revision + "f")
            }
            await loadVault(); return
        }
        #endif
        guard let engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        let result = try await engine.favoriteVaultItem(id: item.id, revision: item.revision, favorite: !item.favorite)
        guard epoch == current else { return }
        struct Mutation: Decodable { let conflict: Bool }
        let mutation: Mutation = try decode(result)
        if mutation.conflict { notice = String(localized: "Saved as a separate version. Review sync conflicts.") }
        await updateSession()
    }

    func trashVaultItem(_ item: VaultItem, restore: Bool = false) async throws {
        let current = epoch
        #if DEBUG
        if uiTesting {
            if let index = fixtureVaultItems.firstIndex(where: { $0.id == item.id }) {
                let value = fixtureVaultItems[index]
                fixtureVaultItems[index] = value.replacing(deleted: !restore, revision: value.revision + "d")
            }
            await loadVault(); return
        }
        #endif
        guard let engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
        if restore { try await engine.restoreItem(id: item.id) }
        else { try await engine.deleteItem(id: item.id) }
        guard epoch == current else { return }
        await updateSession()
    }
}
