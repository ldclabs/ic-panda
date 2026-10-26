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
    private(set) var synchronizing = false
    var error: String?
    var notice: String?
    var vault: [VaultItem] = []
    var channels: [ChannelItem] = []
    /// Navigation/layout may recreate a detail column; drafts belong to the
    /// account session and are also persisted by the core.
    var drafts: [String: String] = [:]
    var library: LibraryListing = .empty
    private(set) var cloudBackups: [CloudBackup] = []
    private(set) var cloudBackupsFailed = false
    private var cloudBackupRequest: UUID?
    var loadingCloudBackups: Bool { cloudBackupRequest != nil }
    var filesFolder = "root"
    var approvals: [ApprovalView] = []
    private(set) var recoveryWarning: RecoveryObservation?
    private(set) var recoveryRefreshFailed = false
    private(set) var checkingRecovery = false
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
    private var lifecycleGeneration: UInt64 = 0
    private var isForeground = false
    private var busyOperation: UUID?
    private var lastSessionSequence: UInt64 = 0
    private var fixtureLoaded = false
    private var automaticSyncAccounts: Set<String> = []
    private var automaticPassRunning = false
    private var automaticOperation: OperationToken?
    private var automaticChannelCursor = 0
    private var recoveryOperation: OperationToken?
    private var recoveryRequest: UUID?
    private var seenRecoveryDigests: [String: String] = [:]
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
        if let account = state.account {
            recoveryWarning = RecoveryObservation(account: account, pending: false, digest: nil,
                observedAtMs: UInt64(Date.now.timeIntervalSince1970 * 1000), stale: false)
        }
        channels = [ChannelItem(id: "fixture-channel", name: "Studio", unread: 0, draft: "", preview: "Private project conversation")]
        cloudBackups = [CloudBackup(uploadId: "fixture-backup", name: "Project brief.pdf", size: 2048, bytes: 4096, needsRoot: false, local: false)]
    }
    #endif

    func start() async {
        guard !opening, engine == nil else { return }
        isForeground = UIApplication.shared.applicationState == .active
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
            uncoverIfActive()
        } catch { report(error) }
    }

    func activity() { lastActivity = .now; automaticOperation?.cancel(); recoveryOperation?.cancel() }

    func beginBusyOperation(synchronizing: Bool = false) -> UUID {
        let token = UUID(); busyOperation = token; busy = true
        self.synchronizing = synchronizing
        return token
    }

    @discardableResult
    func finishBusyOperation(_ token: UUID) -> Bool {
        guard busyOperation == token else { return false }
        busyOperation = nil; busy = false; synchronizing = false
        return true
    }

    private func adopt(_ state: SessionSnapshot?) throws {
        guard let state else { session = nil; return }
        guard state.sequence >= lastSessionSequence, let engine else { return }
        let handle = try engine.scoped(generation: state.generation)
        if recoveryWarning?.account != state.account {
            recoveryWarning = nil; recoveryRefreshFailed = false
        }
        if (session?.state == "unlocked" && state.state != "unlocked") ||
            (session?.account != nil && session?.account != state.account) ||
            (session?.deviceId != nil && session?.deviceId != state.deviceId) {
            epoch &+= 1; viewGeneration &+= 1
            vault = []; channels = []; drafts = [:]; library = .empty; filesFolder = "root"; approvals = []; search = ""
            clearCloudBackups()
        }
        self.engine = handle; session = state; lastSessionSequence = state.sequence
        uncoverIfActive()
    }

    private func uncoverIfActive() {
        guard isForeground, UIApplication.shared.applicationState == .active else { return }
        concealed = false; privacyShield.hide()
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
                guard current == epoch else { return }
                try await fetchCloudBackups(refresh: true, operation: operation)
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

    func inactive() { lifecycleGeneration &+= 1; isForeground = false; concealed = true; if unlocked { privacyShield.show() } }

    func background() {
        lifecycleGeneration &+= 1
        isForeground = false
        concealed = true
        recoveryOperation?.cancel()
        epoch &+= 1
        engine?.suspend()
        backgroundAt = .now
        if immediateBackgroundLock {
            clearContent(clearClipboard: false)
            engine?.lockNow()
        }
    }

    func foreground() async {
        guard UIApplication.shared.applicationState == .active else { return }
        isForeground = true
        lifecycleGeneration &+= 1
        let activation = lifecycleGeneration
        let away = backgroundAt.map { $0.duration(to: .now) }
        backgroundAt = nil
        if let away, away >= .seconds(immediateBackgroundLock ? 0 : 180) { await lock(clearClipboard: false) }
        guard activation == lifecycleGeneration, UIApplication.shared.applicationState == .active else { return }
        if engine == nil { await start() }
        else if let engine {
            while activation == lifecycleGeneration && UIApplication.shared.applicationState == .active && !Task.isCancelled {
                do {
                    let token = engine.controlGeneration()
                    let state = try await engine.resume(expectedGeneration: token)
                    guard activation == lifecycleGeneration, UIApplication.shared.applicationState == .active else { return }
                    try adopt(state)
                    break
                } catch {
                    guard activation == lifecycleGeneration, UIApplication.shared.applicationState == .active else { return }
                    if let failure = error as? MobileError, case .Failure(let code) = failure, code == "LOCKED" {
                        // Identity cancellation can briefly hold the transition
                        // gate. Retry metadata only, without revealing old UI.
                        do { try await Task.sleep(for: .milliseconds(100)) } catch { return }
                        continue
                    }
                    report(error); return
                }
            }
        }
        guard activation == lifecycleGeneration, UIApplication.shared.applicationState == .active, !Task.isCancelled else { return }
        uncoverIfActive()
        if unlocked { await refresh() }
    }

    func protectedDataUnavailable() {
        lifecycleGeneration &+= 1
        isForeground = false
        recoveryOperation?.cancel(); recoveryRequest = nil; checkingRecovery = false
        recoveryWarning = nil; recoveryRefreshFailed = false
        authenticationGeneration &+= 1; webLogin.cancel(); busy = false; authenticating = false
        busyOperation = nil; synchronizing = false
        privacyShield.show()
        concealed = true; epoch &+= 1; viewGeneration &+= 1; drafts = [:]
        vault = []; channels = []; library = .empty; filesFolder = "root"; approvals = []; session = nil
        clearCloudBackups()
        if let engine { engine.suspend(); engine.lockNow(); closing = Task { _ = try? await engine.close() } }
        engine = nil; security?.cancelPresence()
    }

    func lock(clearClipboard: Bool = true) async {
        if clearClipboard { lifecycleGeneration &+= 1 }
        clearContent(clearClipboard: clearClipboard)
        do { try adopt(try await engine?.lock()) } catch { report(error) }
    }

    private func clearContent(clearClipboard: Bool) {
        recoveryOperation?.cancel()
        epoch &+= 1
        viewGeneration &+= 1; drafts = [:]
        vault = []; channels = []; library = .empty; filesFolder = "root"; approvals = []; search = ""
        clearCloudBackups()
        busy = false; notice = nil; error = nil
        busyOperation = nil; synchronizing = false
        if clearClipboard { clipboard.clearOwned() }
        if clearClipboard { authenticationGeneration &+= 1; webLogin.cancel(); engine?.cancelLogin(); authenticating = false }
        security?.cancelPresence()
        // Remove the unlocked UI before awaiting the Rust barrier.
        session = nil
    }

    func login() async {
        guard let engine, !busy else { return }
        let operation = beginBusyOperation()
        authenticating = true; error = nil; activity()
        authenticationGeneration &+= 1
        let current = authenticationGeneration
        defer { if finishBusyOperation(operation) { authenticating = false } }
        do {
            let start = try await engine.beginLogin()
            guard let url = URL(string: start.url) else { throw CocoaError(.coderInvalidValue) }
            let callback = try await webLogin.authenticate(url: url)
            await waitUntilActive()
            guard current == authenticationGeneration else { return }
            _ = try await engine.resume(expectedGeneration: engine.controlGeneration())
            let state = try await engine.completeLogin(callback: callback.absoluteString)
            guard current == authenticationGeneration else { return }
            try adopt(state); uncoverIfActive()
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
        let operation = beginBusyOperation()
        authenticating = true; activity()
        let current = epoch
        defer { if finishBusyOperation(operation) { authenticating = false } }
        do {
            let state = try await engine.unlockFast()
            guard current == epoch else { return }
            try adopt(state); await refresh()
        } catch { if current == epoch { await updateSession(); report(error) } }
    }

    func setFastUnlock(enabled: Bool) async {
        guard let engine, unlocked, !busy else { return }
        let operation = beginBusyOperation(), current = epoch
        authenticating = true; activity()
        defer { if finishBusyOperation(operation) { authenticating = false } }
        do {
            let state = try await engine.setFastUnlock(enabled: enabled)
            guard current == epoch else { return }; try adopt(state)
        } catch { if current == epoch { await updateSession(); report(error) } }
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
                await loadCloudBackups(refresh: false)
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
        let operation = beginBusyOperation(synchronizing: true), current = epoch
        defer { finishBusyOperation(operation) }
        do {
            _ = try await coreOperation { try await engine.reconcile(operation: $0) }
            _ = try await coreOperation { try await engine.sync(operation: $0) }
            _ = try await coreOperation { try await engine.syncConnect(operation: $0) }
            guard current == epoch else { return }
            try await coreOperation { try await fetchCloudBackups(refresh: true, operation: $0) }
            guard current == epoch else { return }; await refresh()
        } catch { if current == epoch { await updateSession(); report(error) } }
    }

    func loadCloudBackups(refresh: Bool) async {
        let current = epoch
        do { try await coreOperation { try await fetchCloudBackups(refresh: refresh, operation: $0) } }
        catch { if current == epoch { report(error) } }
    }

    private func fetchCloudBackups(refresh: Bool, operation: OperationToken) async throws {
        guard let engine, unlocked else { return }
        let current = epoch, request = UUID()
        cloudBackupRequest = request
        defer { if cloudBackupRequest == request { cloudBackupRequest = nil } }
        do {
            let result = try await engine.cloudBackups(refresh: refresh, operation: operation)
            guard !Task.isCancelled, current == epoch, cloudBackupRequest == request else { return }
            cloudBackups = try decode(result)
            if refresh { cloudBackupsFailed = false }
        } catch {
            if !Task.isCancelled, current == epoch, cloudBackupRequest == request {
                cloudBackupsFailed = true; throw error
            }
        }
    }

    private func clearCloudBackups() {
        cloudBackups = []; cloudBackupsFailed = false; cloudBackupRequest = nil
    }

    func loadApprovals() async {
        guard let engine else { return }; let current = epoch
        do {
            let result = try await engine.approvals()
            let values: [ApprovalView] = try decode(result)
            guard current == epoch else { return }; approvals = values
        } catch { if current == epoch { report(error) } }
    }

    /// Public certified metadata only. It does not unlock content, upload it,
    /// or depend on the account's automatic-content-sync preference.
    func refreshRecoveryWarning(refresh: Bool = true) async {
        guard let engine, let account = session?.account else { return }
        let current = epoch, request = UUID(), operation = OperationToken()
        recoveryOperation?.cancel()
        recoveryOperation = operation; recoveryRequest = request; checkingRecovery = true
        defer {
            if recoveryRequest == request {
                recoveryOperation = nil; recoveryRequest = nil; checkingRecovery = false
            }
        }
        do {
            let cached = try await engine.recoveryWarning()
            guard current == epoch, recoveryRequest == request, session?.account == account else { return }
            recordRecoveryObservation(try decode(cached), for: account)
            guard refresh, !concealed, !Task.isCancelled else { return }
            let fresh = try await withTaskCancellationHandler {
                try await engine.refreshRecoveryWarning(operation: operation)
            } onCancel: { operation.cancel() }
            guard current == epoch, recoveryRequest == request, session?.account == account else { return }
            recordRecoveryObservation(try decode(fresh), for: account)
            recoveryRefreshFailed = false
        } catch {
            guard current == epoch, recoveryRequest == request, session?.account == account,
                  !Task.isCancelled else { return }
            if let failure = error as? MobileError, case .Failure(let code) = failure,
               code == "CANCELLED" || code == "LOCKED" { return }
            // Failure never clears a previously verified pending warning.
            recoveryRefreshFailed = true
        }
    }

    func recordRecoveryObservation(_ next: RecoveryObservation?, for account: String) {
        guard session?.account == account else { return }
        guard let next else {
            if recoveryWarning?.account != account { recoveryWarning = nil }
            return
        }
        guard next.account == account else { return }
        if let previous = recoveryWarning, previous.account == account,
           previous.observedAtMs > next.observedAtMs { return }
        recoveryWarning = next
        if next.pending, let digest = next.digest, seenRecoveryDigests[account] != digest {
            seenRecoveryDigests[account] = digest
            notice = String(localized: "An account recovery is pending. Review it in Account security.")
        }
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
                    clearCloudBackups()
                    let engine = engine
                    Task { _ = try? await engine?.lock() }
                }
                if code == "FORBIDDEN" { self.error = String(localized: "The operation was cancelled or is not permitted.") }
                else if code == "EXECUTION_UNKNOWN" { self.error = String(localized: "The result is not confirmed. Reconcile this request before retrying.") }
                else if code == "HARDWARE_UNAVAILABLE" { self.error = String(localized: "Device authentication is temporarily unavailable. Your keys are kept; try again when this device is unlocked.") }
                else if code == "HARDWARE_KEY_INVALIDATED" { self.error = String(localized: "This device key is no longer usable. Use another authorized device or account recovery. Your workspace has not been erased.") }
                else if code == "INTEGRITY_FAILED" { self.error = String(localized: "A record or device key could not be verified. Your workspace has not been erased.") }
                else if code == "HARDWARE_AUTHORIZATION_REQUIRED" || code == "POLICY_STALE" { self.error = String(localized: "Security requirements changed. Review the request again before approving it.") }
                else if code == "APPROVAL_KEY_REQUIRED" { self.error = String(localized: "This account requires a registered approval key. Set up this device in Hardware security, or ask another authorized device.") }
                else if code == "NETWORK_UNAVAILABLE" { self.error = String(localized: "Connection unavailable. Your saved content stays on this device.") }
                else if code == "UNAVAILABLE" { self.error = String(localized: "This operation is temporarily unavailable. Your saved data is kept.") }
                else if code == "LOCAL_STORAGE_UNAVAILABLE" { self.error = String(localized: "Local storage is unavailable. Unlock this device and try again; do not reset the workspace to recover an interrupted operation.") }
                else if code == "QUOTA_EXCEEDED" { self.error = String(localized: "This operation exceeds its size or capacity limit. Secret snapshots on this device are limited to 8 MiB.") }
                else if code == "UNSUPPORTED_PROTOCOL" { self.error = String(localized: "This action is not supported on this device. Secret snapshots must use their dedicated import flow.") }
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
