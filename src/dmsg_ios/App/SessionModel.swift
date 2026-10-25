import Foundation
import Observation
import SwiftUI
@preconcurrency import DMsgCore

@MainActor @Observable
final class SessionModel {
    private(set) var engine: MobileEngine?
    private(set) var files: PrivateFiles?
    private(set) var session: SessionSnapshot?
    private(set) var epoch: UInt64 = 0
    private(set) var viewGeneration: UInt64 = 0
    var concealed = true
    var busy = false
    var error: String?
    var notice: String?
    var vault: [VaultItem] = []
    var channels: [ChannelItem] = []
    /// Navigation/layout may recreate a detail column; drafts belong to the
    /// account session and are also persisted by the core.
    var drafts: [String: String] = [:]
    var library: LibraryListing = .empty
    var filesFolder = "root"
    var approvals: [ApprovalView] = []
    var selectedTab: AppTab = .vault
    var immediateBackgroundLock = false {
        didSet {
            guard let files else { return }
            do { try LocalPreferences(immediateBackgroundLock: immediateBackgroundLock, automaticSyncAccounts: automaticSyncAccounts).write(at: files.root) }
            catch { report(error) }
        }
    }
    var search = ""
    var vaultHasMore = false
    var authenticating = false
    let clipboard = PrivateClipboard()
    let webLogin = WebLogin()
    private let privacyShield = PrivacyShield()
    private var backgroundAt: ContinuousClock.Instant?
    private var lastActivity = ContinuousClock.now
    private var security: AppleSecurity?
    private var opening = false
    private var closing: Task<Void, Never>?
    private var authenticationGeneration: UInt64 = 0
    private var lastSessionSequence: UInt64 = 0
    private var fixtureLoaded = false
    private var automaticSyncAccounts: Set<String> = []
    private var automaticPassRunning = false
    private var automaticOperation: OperationToken?
    private var automaticChannelCursor = 0
    #if DEBUG
    let uiTesting = ProcessInfo.processInfo.arguments.contains("--ui-testing")
    #else
    let uiTesting = false
    #endif

    var unlocked: Bool { session?.state == "unlocked" }
    var automaticSyncEnabled: Bool { session?.account.map { automaticSyncAccounts.contains($0) } ?? false }

    #if DEBUG
    func installFixtures(_ state: SessionSnapshot, items: [VaultItem]) {
        session = state; vault = items
        channels = [ChannelItem(id: "fixture-channel", name: "Studio", unread: 0, draft: "", preview: "Private project conversation")]
    }
    #endif

    func start() async {
        guard !opening, engine == nil else { return }
        opening = true; defer { opening = false }
        #if DEBUG
        if uiTesting { if !fixtureLoaded { loadFixtures(); fixtureLoaded = true }; concealed = false; return }
        #endif
        guard UIApplication.shared.isProtectedDataAvailable else {
            error = String(localized: "Unlock this iPhone to open dMsg."); return
        }
        let openingEpoch = epoch
        do {
            await closing?.value; closing = nil
            guard UIApplication.shared.isProtectedDataAvailable, openingEpoch == epoch else { return }
            let files = try await Task.detached { try PrivateFiles() }.value
            let preferences = try LocalPreferences.read(at: files.root)
            automaticSyncAccounts = preferences.automaticSyncAccounts
            immediateBackgroundLock = preferences.immediateBackgroundLock
            let security = AppleSecurity(installation: files.installation)
            let engine = try await MobileEngine.open(directory: files.workspace.path, staging: files.staging.path, platform: security)
            guard UIApplication.shared.isProtectedDataAvailable, openingEpoch == epoch else { try? await engine.close(); return }
            self.files = files; self.security = security; self.engine = engine
            try adopt(try await engine.sessionState())
            concealed = false
        } catch { report(error) }
    }

    func activity() { lastActivity = .now; automaticOperation?.cancel() }

    private func adopt(_ state: SessionSnapshot?) throws {
        guard let state else { session = nil; return }
        guard state.sequence >= lastSessionSequence, let engine else { return }
        let handle = try engine.scoped(generation: state.generation)
        if (session?.state == "unlocked" && state.state != "unlocked") ||
            (session?.account != nil && session?.account != state.account) {
            epoch &+= 1; viewGeneration &+= 1
            vault = []; channels = []; drafts = [:]; library = .empty; filesFolder = "root"; approvals = []; search = ""
        }
        self.engine = handle; session = state; lastSessionSequence = state.sequence
    }

    /// Called only by the settings confirmation. Disabled by default and never
    /// inferred from login, notification permission, or a restored UI snapshot.
    func setAutomaticSync(_ enabled: Bool) {
        guard let account = session?.account else { return }
        if enabled { automaticSyncAccounts.insert(account) } else { automaticSyncAccounts.remove(account); automaticOperation?.cancel() }
        if let files {
            do { try LocalPreferences(immediateBackgroundLock: immediateBackgroundLock, automaticSyncAccounts: automaticSyncAccounts).write(at: files.root) }
            catch { report(error) }
        }
    }

    func optedInSyncPass() async {
        guard automaticSyncEnabled, unlocked, !concealed, !busy, !automaticPassRunning, let engine else { return }
        automaticPassRunning = true
        let operation = OperationToken(), current = epoch
        automaticOperation = operation
        defer { automaticOperation = nil; automaticPassRunning = false }
        do {
            try await withTaskCancellationHandler {
                _ = try await engine.reconcile(operation: operation)
                _ = try await engine.sync(operation: operation)
                _ = try await engine.syncConnect(operation: operation)
                let channels = try await engine.channels().items
                for index in 0..<min(4, channels.count) {
                    let item = channels[(automaticChannelCursor + index) % channels.count]
                    _ = try await engine.refreshChannel(channel: item.id, operation: operation)
                }
                if !channels.isEmpty { automaticChannelCursor = (automaticChannelCursor + 4) % channels.count }
                // File backup is deliberately absent: it requires its own
                // explicit selection and confirmation in Files.
            } onCancel: { operation.cancel() }
            guard current == epoch else { return }; await updateSession()
        } catch {
            // Keep local content visible; the explicit Sync action surfaces
            // detailed failures. Lock and account switches invalidate snapshots.
            if current == epoch, let failure = error as? MobileError, case .Failure(let code) = failure,
               code == "LOCKED" || code == "DEVICE_NOT_APPROVED" { report(error) }
        }
    }

    func updateSession() async {
        let current = epoch
        do {
            let state = try await engine?.sessionState()
            guard current == epoch else { return }; try adopt(state)
            if unlocked { await refresh() }
        } catch { report(error) }
    }

    func idleTick() async {
        if unlocked && !concealed && lastActivity.duration(to: .now) >= .seconds(15 * 60) { await lock() }
    }

    func inactive() { concealed = true; if unlocked { privacyShield.show() } }

    func background() async {
        concealed = true
        epoch &+= 1
        engine?.suspend()
        backgroundAt = .now
        if immediateBackgroundLock { await lock(clearClipboard: false) }
    }

    func foreground() async {
        let away = backgroundAt.map { $0.duration(to: .now) }
        backgroundAt = nil
        if let away, away >= .seconds(immediateBackgroundLock ? 0 : 180) { await lock(clearClipboard: false) }
        if engine == nil { await start() }
        else {
            let current = epoch
            do { let state = try await engine?.resume(); if current == epoch { try adopt(state) } }
            catch { if current == epoch { report(error) } }
        }
        concealed = false
        privacyShield.hide()
        if unlocked { await refresh() }
    }

    func protectedDataUnavailable() {
        authenticationGeneration &+= 1; webLogin.cancel(); busy = false; authenticating = false
        privacyShield.show()
        concealed = true; epoch &+= 1; viewGeneration &+= 1; drafts = [:]
        vault = []; channels = []; library = .empty; filesFolder = "root"; approvals = []; session = nil
        if let engine { closing = Task { _ = try? await engine.close() } }
        engine = nil; security?.cancelPresence()
    }

    func lock(clearClipboard: Bool = true) async {
        epoch &+= 1
        viewGeneration &+= 1; drafts = [:]
        vault = []; channels = []; library = .empty; filesFolder = "root"; approvals = []; search = ""
        busy = false; notice = nil; error = nil
        if clearClipboard { clipboard.clearOwned() }
        if clearClipboard { authenticationGeneration &+= 1; webLogin.cancel(); engine?.cancelLogin(); authenticating = false }
        security?.cancelPresence()
        // Remove the unlocked UI before awaiting the Rust barrier.
        session = nil
        do { try adopt(try await engine?.lock()) } catch { report(error) }
    }

    func login(legacy: Bool = false) async {
        guard let engine, !busy else { return }
        busy = true; authenticating = true; error = nil; activity()
        authenticationGeneration &+= 1
        let current = authenticationGeneration
        defer { if current == authenticationGeneration { busy = false; authenticating = false } }
        do {
            let start = try await engine.beginLogin(legacyOrigin: legacy)
            guard let url = URL(string: start.url) else { throw CocoaError(.coderInvalidValue) }
            let callback = try await webLogin.authenticate(url: url)
            await waitUntilActive()
            guard current == authenticationGeneration else { return }
            _ = try await engine.resume()
            let state = try await engine.completeLogin(callback: callback.absoluteString)
            guard current == authenticationGeneration else { return }
            try adopt(state); concealed = false
            if unlocked { await refresh() }
        } catch { if current == authenticationGeneration { engine.cancelLogin(); await updateSession(); report(error) } }
    }

    func waitUntilActive() async {
        guard UIApplication.shared.applicationState != .active else { return }
        for await _ in NotificationCenter.default.notifications(named: UIApplication.didBecomeActiveNotification) {
            if Task.isCancelled || UIApplication.shared.applicationState == .active { return }
        }
    }

    func fastUnlock() async {
        guard let engine, !busy else { return }
        busy = true; authenticating = true; activity()
        let current = epoch
        defer { if current == epoch { busy = false; authenticating = false } }
        do {
            let state = try await engine.unlockFast()
            guard current == epoch else { return }
            try adopt(state); await refresh()
        } catch { if current == epoch { report(error) } }
    }

    func refresh() async {
        guard unlocked, let engine else { return }
        let current = epoch
        do {
            let state = try await engine.sessionState()
            guard current == epoch else { return }; try adopt(state)
            switch selectedTab {
            case .vault: await loadVault()
            case .messages:
                let result = try await engine.channels()
                guard current == epoch else { return }; channels = result.items
            case .files:
                let folder = filesFolder
                let result = try await engine.files(folder: folder)
                let files: LibraryListing = try decode(result)
                guard current == epoch, folder == filesFolder else { return }; library = files
            case .account: break
            }
        } catch { if current == epoch { report(error) } }
    }

    func loadVault(more: Bool = false) async {
        guard unlocked, let engine else { return }
        let current = epoch, query = search
        do {
            let page = try await engine.vaultPage(query: query, offset: more ? UInt32(vault.count) : 0, limit: 60)
            guard current == epoch, query == search else { return }
            vault = more ? vault + page.items : page.items; vaultHasMore = page.hasMore
        } catch { if current == epoch { report(error) } }
    }

    func synchronize() async {
        activity()
        guard let engine, !busy else { return }
        busy = true; defer { busy = false }; let current = epoch
        do {
            _ = try await coreOperation { try await engine.reconcile(operation: $0) }
            _ = try await coreOperation { try await engine.sync(operation: $0) }
            _ = try await coreOperation { try await engine.syncConnect(operation: $0) }
            guard current == epoch else { return }; await refresh()
        } catch { if current == epoch { await updateSession(); report(error) } }
    }

    func loadApprovals() async {
        guard let engine else { return }; let current = epoch
        do {
            let result = try await engine.approvals()
            let values: [ApprovalView] = try decode(result)
            guard current == epoch else { return }; approvals = values
        } catch { if current == epoch { report(error) } }
    }

    func decode<T: Decodable>(_ snapshot: ObjectSnapshot) throws -> T {
        guard !Task.isCancelled, snapshot.generation == session?.generation else { throw CancellationError() }
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(T.self, from: Data(snapshot.json.utf8))
    }

    func report(_ error: Error) {
        if error is CancellationError || Task.isCancelled { return }
        if let error = error as? CocoaError {
            if error.code == .userCancelled { return }
            if error.code == .fileWriteOutOfSpace { self.error = String(localized: "Not enough storage on this device."); return }
            if error.code == .fileReadTooLarge { self.error = String(localized: "This file exceeds the 100 MiB limit."); return }
        }
        if let core = error as? MobileError {
            switch core {
            case .Failure(let code):
                if code == "CANCELLED" { return }
                // A stale UI handle must not lock a replacement account. The
                // lifecycle already clears views when it requests a lock.
                if code == "LOCKED" { return }
                if code == "DEVICE_NOT_APPROVED" {
                    epoch &+= 1; viewGeneration &+= 1; drafts = [:]; vault = []; channels = []; library = .empty; filesFolder = "root"; approvals = []; session = nil
                    let engine = engine
                    Task { _ = try? await engine?.lock() }
                }
                if code == "FORBIDDEN" { self.error = String(localized: "The operation was cancelled or is not permitted.") }
                else if code == "EXECUTION_UNKNOWN" { self.error = String(localized: "The result is not confirmed. Reconcile this request before retrying.") }
                else if code == "NETWORK_UNAVAILABLE" || code == "UNAVAILABLE" { self.error = String(localized: "Connection unavailable. Your saved content stays on this device.") }
                else { self.error = String(localized: "The operation could not be completed.") + " (\(code))" }
            }
        } else { self.error = String(localized: "The operation could not be completed. Please try again.") }
    }
}

enum AppTab: String, CaseIterable, Identifiable {
    case vault, messages, files, account
    var id: Self { self }
    var title: LocalizedStringKey {
        switch self { case .vault: "Vault"; case .messages: "Messages"; case .files: "Files"; case .account: "Account" }
    }
    var icon: String {
        switch self { case .vault: "key.horizontal"; case .messages: "bubble.left.and.bubble.right"; case .files: "folder"; case .account: "person.crop.circle" }
    }
}
