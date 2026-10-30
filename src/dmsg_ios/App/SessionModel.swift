import Foundation
import Observation
import SwiftUI
import AuthenticationServices
import FileProvider
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
    private(set) var unlocking = false
    private(set) var synchronizing = false
    var error: String?
    var notice: String?
    var reminderEntry: ReminderEntry?
    var vault: [VaultItem] = []
    var channels: [ChannelItem] = []
    private(set) var blockedAccounts: Set<String> = []
    var activeChannel: String?
    private(set) var contentRevision: UInt64 = 0
    private(set) var profileRevision: UInt64 = 0
    private(set) var favorites: [VaultItem] = []
    var vaultFilter = "all"
    private(set) var loadingVault = false
    private var vaultRequest: UUID?
    private var vaultQuery = ""
    /// Navigation/layout may recreate a detail column; drafts belong to the
    /// account session and are also persisted by the core.
    var drafts: [String: String] = [:]
    private var dirtyDrafts: Set<String> = []
    var library: LibraryListing = .empty
    private(set) var cloudBackups: [CloudBackup] = []
    private(set) var cloudBackupsFailed = false
    private var cloudBackupRequest: UUID?
    var loadingCloudBackups: Bool { cloudBackupRequest != nil }
    var filesFolder = "root"
    var filesSearch = ""
    var approvals: [ApprovalView] = []
    private(set) var recoveryWarning: RecoveryObservation?
    private(set) var recoveryRefreshFailed = false
    private(set) var checkingRecovery = false
    var selectedTab: AppTab = .vault
    var immediateBackgroundLock = false {
        didSet {
            guard let files else { return }
            do { try writePreferences(at: files.root) }
            catch { report(error) }
        }
    }
    /// Accounts whose security-alert prompt was set aside on this device.
    private(set) var notificationSetupDismissed: Set<String> = []
    var search = ""
    var vaultHasMore = false
    let clipboard = PrivateClipboard()
    let transfer = TransferModel()
    let deviceTransfer = DeviceTransferModel()
    let apple = NativeApple()
    let push = NativePush()
    let notifications = NativeNotifications()
    let purchases = PurchaseModel()
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
    private var automaticUnlock = AutomaticUnlockPolicy()
    private var busyOperation: UUID?
    private var lastSessionSequence: UInt64 = 0
    private var fixtureLoaded = false
    /// Accounts that turned automatic sync off; every other account syncs.
    private var manualSyncAccounts: Set<String> = []
    /// Accounts already told that they sync automatically.
    private var syncIntroduced: Set<String> = []
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
    var automaticSyncEnabled: Bool { session?.account.map { !manualSyncAccounts.contains($0) } ?? false }

    #if DEBUG
    func installFixtures(_ state: SessionSnapshot, items: [VaultItem]) {
        session = state; vault = items; favorites = Array(items.filter(\.favorite).prefix(4))
        if let account = state.account {
            notifications.installFixtures(account: account)
            recoveryWarning = RecoveryObservation(account: account, pending: false, digest: nil,
                observedAtMs: UInt64(Date.now.timeIntervalSince1970 * 1000), stale: false)
        }
        channels = [ChannelItem(id: "fixture-channel", name: "Studio", unread: 0, draft: "", preview: "Private project conversation")]
        cloudBackups = [CloudBackup(uploadId: "fixture-backup", name: "Project brief.pdf", size: 2048, bytes: 4096, needsRoot: false, local: false)]
    }
    #endif

    private(set) var environment = "production"

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
            environment = MobileDeployment.wantsStaging ? "staging" : "production"
            let deployment = try MobileDeployment.load()
            if try deployment.recordSelection() { invalidateSystemExports() }
            let name = deployment.name
            let files = try await Task.detached { try PrivateFiles(environment: name) }.value
            let preferences = try LocalPreferences.read(at: files.root)
            manualSyncAccounts = preferences.manualSyncAccounts ?? []
            syncIntroduced = preferences.syncIntroduced ?? []
            notificationSetupDismissed = preferences.notificationSetupDismissed ?? []
            immediateBackgroundLock = preferences.immediateBackgroundLock
            let security = AppleSecurity(installation: files.installation, environment: deployment.name)
            let engine: MobileEngine
            switch deployment {
            case .production: engine = try await MobileEngine.open(directory: files.workspace.path, staging: files.staging.path, platform: security)
            case .staging(let configuration): engine = try await MobileEngine.openStaging(directory: files.workspace.path, staging: files.staging.path, platform: security, deployment: configuration)
            }
            guard UIApplication.shared.isProtectedDataAvailable, openingEpoch == epoch else { try? await engine.close(); return }
            self.files = files; self.security = security; self.engine = engine
            try adopt(try await engine.sessionState())
            uncoverIfActive()
            if let route = DMsgAppDelegate.pendingRoute { await openPushRoute(route) }
            await unlockAutomaticallyIfNeeded()
        } catch {
            if openingEpoch == epoch {
                report(error)
                if session == nil { uncoverIfActive() }
            }
        }
    }

    func profileDidChange() { profileRevision &+= 1 }

    func loadBlockedAccounts() async {
        let current = epoch
        do { if let values = try await engine?.blockedAccounts(), current == epoch { blockedAccounts = Set(values) } }
        catch { if current == epoch { report(error) } }
    }

    func blockAccount(_ account: String, blocked: Bool) async {
        let current = epoch
        do { _ = try await engine?.setBlockedAccount(account: account, blocked: blocked) }
        catch { if current == epoch { report(error) } }
        if current == epoch { await loadBlockedAccounts() }
    }

    var integrationReset: Task<Void, Never>?

    func invalidateSystemExports() {
        for purpose in [ExportPurpose.credentials, .files] {
            do { try SystemExports(purpose).invalidate() } catch { report(error) }
        }
        let previous = integrationReset
        integrationReset = Task {
            await previous?.value
            try? await ASCredentialIdentityStore.shared.removeAllCredentialIdentities()
            let domain = NSFileProviderDomain(identifier: NSFileProviderDomainIdentifier("net.dmsg.ios.exports"), displayName: "dMsg")
            try? await NSFileProviderManager.remove(domain)
        }
    }

    func removeLocalAccount() async {
        guard let account = session?.account, let engine, let files else { return }
        await cancelBackgroundForAccount(account)
        invalidateSystemExports()
        clearContent(clearClipboard: true)
        do {
            _ = try await engine.removeLocalAccount(account: account)
            try push.forget(account)
            if let incoming = try? IncomingShares() {
                for entry in try incoming.list() where entry.accountHash == IncomingShares.accountHash(account) { try incoming.remove(entry) }
            }
            manualSyncAccounts.remove(account); syncIntroduced.remove(account); notificationSetupDismissed.remove(account)
            try writePreferences(at: files.root)
            let staging = files.staging.appending(path: account)
            if FileManager.default.fileExists(atPath: staging.path) { try FileManager.default.removeItem(at: staging) }
            for suffix in [".json", ".development.json", ".production.json"] {
                let native = files.root.appending(path: "apple").appending(path: account + suffix)
                if FileManager.default.fileExists(atPath: native.path) { try FileManager.default.removeItem(at: native) }
            }
            clearContent(clearClipboard: true); await updateSession()
        } catch { report(error) }
    }

    func setDraft(channel: String, text: String) {
        drafts[channel] = text; dirtyDrafts.insert(channel)
    }

    func draftSaved(channel: String, text: String) {
        if drafts[channel] == text { dirtyDrafts.remove(channel) }
    }

    func clearQueuedDraft(channel: String, sent: String) {
        if drafts[channel] == sent { drafts[channel] = "" }
    }

    func openPushRoute(_ route: String) async {
        automaticUnlock.suppress()
        guard let files, let engine else { DMsgAppDelegate.pendingRoute = route; return }
        let current = epoch
        try? push.load(files)
        DMsgAppDelegate.pendingRoute = nil
        do {
            let destination: ReminderRoute = try decode(try await engine.notificationResolve(route: route))
            guard current == epoch else { return }
            reminderEntry = ReminderEntry(account: destination.account, target: destination.target, ended: destination.ended)
        } catch {
            guard current == epoch else { return }
            // Legacy routes identify only an original account; they never select
            // another pending request or silently switch the active workspace.
            if let failure = error as? MobileError, case .Failure(let code) = failure,
               ["NOT_FOUND", "FORBIDDEN", "AUTH_REQUIRED", "DEVICE_NOT_APPROVED", "INVALID_INPUT"].contains(code) {
                reminderEntry = ReminderEntry(account: push.account(for: route), ended: true)
            } else {
                reminderEntry = ReminderEntry(account: push.account(for: route), retryRoute: route)
            }
        }
    }

    func activity() { lastActivity = .now; automaticOperation?.cancel(); recoveryOperation?.cancel() }

    /// A device transfer moved bytes, which keeps the app from locking for idleness.
    func transferMoved() { lastActivity = .now }

    func beginBusyOperation(synchronizing: Bool = false) -> UUID {
        let token = UUID(); busyOperation = token; busy = true
        unlocking = false
        self.synchronizing = synchronizing
        return token
    }

    @discardableResult
    func finishBusyOperation(_ token: UUID) -> Bool {
        guard busyOperation == token else { return false }
        busyOperation = nil; busy = false; synchronizing = false; unlocking = false
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
            vault = []; blockedAccounts = []; favorites = []; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; drafts = [:]; library = .empty; filesFolder = "root"; filesSearch = ""; approvals = []; search = ""
            clearCloudBackups()
        }
        if let old = session?.account, old != state.account { invalidateSystemExports() }
        if let account = state.account, [ExportPurpose.credentials, .files].contains(where: { purpose in
            guard let header = try? SystemExports(purpose).header(checkExpiry: false) else { return false }
            return header.accountHash != IncomingShares.accountHash(account)
        }) { invalidateSystemExports() }
        self.engine = handle; session = state; lastSessionSequence = state.sequence
        uncoverIfActive()
    }

    private func uncoverIfActive() {
        guard isForeground, UIApplication.shared.applicationState == .active else { return }
        concealed = false; privacyShield.hide()
    }

    /// On for every account until the person turns it off in settings,
    /// which this device keeps for that account.
    func setAutomaticSync(_ enabled: Bool) {
        guard let account = session?.account else { return }
        if enabled { manualSyncAccounts.remove(account) } else { manualSyncAccounts.insert(account); automaticOperation?.cancel() }
        if let files {
            do { try writePreferences(at: files.root) }
            catch { report(error) }
        }
    }

    /// "Not now" on the security-alert prompt; binding another login offers it again.
    func setNotificationSetupDismissed(_ dismissed: Bool, account: String) {
        guard notificationSetupDismissed.contains(account) != dismissed else { return }
        if dismissed { notificationSetupDismissed.insert(account) } else { notificationSetupDismissed.remove(account) }
        if let files {
            do { try writePreferences(at: files.root) }
            catch { report(error) }
        }
    }

    private func writePreferences(at root: URL) throws {
        try LocalPreferences(immediateBackgroundLock: immediateBackgroundLock, manualSyncAccounts: manualSyncAccounts,
                             syncIntroduced: syncIntroduced, notificationSetupDismissed: notificationSetupDismissed).write(at: root)
    }

    func automaticSyncPass() async {
        guard automaticSyncEnabled, unlocked, !concealed, !busy, !automaticPassRunning,
              let account = session?.account else { return }
        if !syncIntroduced.contains(account) {
            // Said once per account, where it first syncs by itself.
            syncIntroduced.insert(account)
            notice = String(localized: "While dMsg is open and unlocked, encrypted edits will upload and messages and approvals will refresh. File backups still require a separate confirmation. You can turn this off at any time.")
            if let files {
                do { try writePreferences(at: files.root) }
                catch { report(error) }
            }
        }
        automaticPassRunning = true
        let operation = OperationToken(), current = epoch
        automaticOperation = operation
        defer { automaticOperation = nil; automaticPassRunning = false }
        do {
            try await withTaskCancellationHandler {
                try await syncPass(operation: operation, automatic: true)
            } onCancel: { operation.cancel() }
        } catch {
            if current == epoch, let failure = error as? MobileError, case .Failure(let code) = failure,
               code == "DEVICE_NOT_APPROVED" { report(error) }
        }
    }

    /// One pass owns its handle. Independent read failures do not starve the
    /// current channel; cancellation and session changes still end the pass.
    private func syncPass(operation: OperationToken, automatic: Bool) async throws {
        guard let engine, unlocked else { return }
        let current = epoch
        var failures: [Error] = []
        func attempt(_ work: () async throws -> Void) async throws {
            guard current == epoch, !Task.isCancelled else { throw CancellationError() }
            do { try await work() }
            catch {
                if let failure = error as? MobileError, case .Failure(let code) = failure,
                   ["LOCKED", "CANCELLED", "DEVICE_NOT_APPROVED"].contains(code) { throw error }
                if error is CancellationError { throw error }
                failures.append(error)
            }
        }
        try await attempt { _ = try await engine.reconcile(operation: operation) }
        // This verifies current device authority before content is refreshed.
        _ = try await engine.sync(operation: operation)
        try await attempt { _ = try await engine.syncConnect(operation: operation) }
        let available = try await engine.channels().items
        let ids = Self.channelBatch(available.map(\.id), active: activeChannel,
                                    cursor: automaticChannelCursor, limit: automatic ? 4 : available.count)
        let remaining = available.filter { $0.id != activeChannel }.count
        if remaining > 0 { automaticChannelCursor = (automaticChannelCursor + max(1, ids.filter { $0 != activeChannel }.count)) % remaining }
        for id in ids {
            try await attempt { _ = try await engine.refreshChannel(channel: id, operation: operation) }
        }
        try await attempt { _ = try await engine.inbox(refresh: true) }
        try await attempt { try await fetchCloudBackups(refresh: true, operation: operation) }
        guard current == epoch else { throw CancellationError() }
        try await attempt { _ = try await engine.syncBlockedAccounts() }
        await loadBlockedAccounts()
        try await attempt { _ = try await engine.maintainFiles() }
        contentRevision &+= 1
        await updateSession()
        await loadApprovals()
        if let error = failures.first { throw error }
    }

    static func channelBatch(_ ids: [String], active: String?, cursor: Int, limit: Int) -> [String] {
        guard !ids.isEmpty, limit > 0 else { return [] }
        var batch: [String] = []
        if let active, ids.contains(active) { batch.append(active) }
        let remaining = ids.filter { $0 != active }
        for index in 0..<remaining.count where batch.count < limit {
            batch.append(remaining[(cursor + index) % remaining.count])
        }
        return batch
    }

    func loadChannels() async {
        guard let engine, unlocked else { return }; let current = epoch
        do {
            let result = try await engine.channels()
            guard current == epoch, !Task.isCancelled else { return }; channels = result.items
        } catch { if current == epoch { report(error) } }
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
        automaticUnlock.rearm()
        lifecycleGeneration &+= 1
        isForeground = false
        concealed = true
        recoveryOperation?.cancel()
        epoch &+= 1
        transfer.cancel()
        deviceTransfer.background()
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
        if unlocked {
            let pending = dirtyDrafts.compactMap { channel in drafts[channel].map { MessageDraft(channel: channel, text: $0) } }
            let current = epoch
            do {
                try await engine?.saveMessageDrafts(drafts: pending)
                guard current == epoch else { return }
                for draft in pending { draftSaved(channel: draft.channel, text: draft.text) }
            } catch { report(error) }
            await refresh()
            await deviceTransfer.foreground(self)
        } else {
            await unlockAutomaticallyIfNeeded()
        }
    }

    func protectedDataUnavailable() {
        automaticUnlock.rearm()
        (try? SystemExports(.files))?.lockFiles()
        lifecycleGeneration &+= 1
        isForeground = false
        transfer.cancel()
        deviceTransfer.reset()
        recoveryOperation?.cancel(); recoveryRequest = nil; checkingRecovery = false
        recoveryWarning = nil; recoveryRefreshFailed = false
        authenticationGeneration &+= 1; webLogin.cancel(); busy = false
        busyOperation = nil; synchronizing = false; unlocking = false
        privacyShield.show()
        concealed = true; epoch &+= 1; viewGeneration &+= 1; drafts = [:]; dirtyDrafts = []
        vault = []; blockedAccounts = []; favorites = []; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; library = .empty; filesFolder = "root"; filesSearch = ""; approvals = []; session = nil
        clearCloudBackups()
        if let engine { engine.suspend(); engine.lockNow(); closing = Task { _ = try? await engine.close() } }
        engine = nil; security?.cancelPresence()
    }

    func lock(clearClipboard: Bool = true) async {
        if clearClipboard { automaticUnlock.suppress() }
        if clearClipboard { lifecycleGeneration &+= 1 }
        let account = session?.account, engine = engine
        let drafts = dirtyDrafts.compactMap { channel in self.drafts[channel].map { MessageDraft(channel: channel, text: $0) } }
        let flush = clearClipboard && unlocked && account != nil && !drafts.isEmpty
        clearContent(clearClipboard: clearClipboard)
        deviceTransfer.reset()
        let work = beginBusyOperation(); defer { finishBusyOperation(work) }
        do {
            if flush, let account { try adopt(try await engine?.lockWithDrafts(account: account, drafts: drafts)) }
            else { try adopt(try await engine?.lock()) }
        } catch {
            _ = try? await engine?.lock()
            await updateSession(); report(error)
        }
    }

    private func clearContent(clearClipboard: Bool) {
        notifications.clearVisibility()
        (try? SystemExports(.files))?.lockFiles()
        transfer.cancel()
        recoveryOperation?.cancel()
        epoch &+= 1
        viewGeneration &+= 1; drafts = [:]; dirtyDrafts = []
        vault = []; blockedAccounts = []; favorites = []; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; library = .empty; filesFolder = "root"; filesSearch = ""; approvals = []; search = ""
        clearCloudBackups()
        busy = false; notice = nil; error = nil
        busyOperation = nil; synchronizing = false; unlocking = false
        if clearClipboard { clipboard.clearOwned() }
        if clearClipboard { authenticationGeneration &+= 1; webLogin.cancel(); engine?.cancelLogin() }
        security?.cancelPresence()
        // Remove the unlocked UI before awaiting the Rust barrier.
        session = nil
    }

    func login() async {
        guard let engine, !busy else { return }
        automaticUnlock.suppress()
        let operation = beginBusyOperation()
        error = nil; activity()
        authenticationGeneration &+= 1
        let current = authenticationGeneration
        defer { finishBusyOperation(operation) }
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
        automaticUnlock.suppress()
        let operation = beginBusyOperation()
        unlocking = true; error = nil
        activity()
        let current = epoch
        defer { finishBusyOperation(operation) }
        do {
            let state = try await engine.unlockFast()
            guard current == epoch else { return }
            try adopt(state); await refresh()
        } catch { if current == epoch { await updateSession(); report(error) } }
    }

    private func unlockAutomaticallyIfNeeded() async {
        guard isForeground, !concealed, engine != nil,
              UIApplication.shared.applicationState == .active,
              UIApplication.shared.isProtectedDataAvailable,
              automaticUnlock.consume(locked: session?.state == "locked",
                                      fastUnlock: session?.fastUnlock == true, busy: busy) else { return }
        await fastUnlock()
    }

    func setFastUnlock(enabled: Bool) async {
        guard let engine, unlocked, !busy else { return }
        let operation = beginBusyOperation(), current = epoch
        activity()
        defer { finishBusyOperation(operation) }
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
                await loadBlockedAccounts()
                let result = try await engine.channels()
                guard current == epoch else { return }; channels = result.items
            case .files:
                _ = try await engine.maintainFiles()
                let folder = filesFolder
                let query = filesSearch
                let files: LibraryListing
                if query.isEmpty { files = try decode(try await engine.files(folder: folder)) }
                else {
                    let matches: [LibraryFile] = try decode(try await engine.searchFiles(query: query, folder: folder == "root" ? nil : folder))
                    files = LibraryListing(folder: library.folder, folders: [], files: matches)
                }
                guard query == filesSearch, !Task.isCancelled else { return }
                guard current == epoch, folder == filesFolder else { return }; library = files
                await loadCloudBackups(refresh: false)
            case .account: break
            }
        } catch { if current == epoch { report(error) } }
    }

    func loadVault(more: Bool = false) async {
        guard unlocked, let engine, !more || !loadingVault else { return }
        let current = epoch, query = search, kind = vaultFilter, request = UUID()
        let queryKey = query + "\u{0}" + kind
        let count = more ? 60 : (queryKey == vaultQuery ? max(60, vault.count) : 60)
        let offset = more ? vault.count : 0
        vaultRequest = request; loadingVault = true
        defer { if vaultRequest == request { vaultRequest = nil; loadingVault = false } }
        do {
            var items: [VaultItem] = [], hasMore = false
            repeat {
                let size = min(100, count - items.count)
                let page = try await engine.vaultPage(query: query, kind: kind == "all" ? nil : kind,
                    favoritesOnly: false, offset: UInt32(offset + items.count), limit: UInt32(size))
                guard current == epoch, request == vaultRequest, query == search, kind == vaultFilter, !Task.isCancelled else { return }
                items += page.items; hasMore = page.hasMore
                if !hasMore { break }
            } while items.count < count
            vault = more ? vault + items : items; vaultHasMore = hasMore; vaultQuery = queryKey
            let starred = try await engine.vaultPage(query: "", kind: nil, favoritesOnly: true, offset: 0, limit: 4)
            guard current == epoch, request == vaultRequest, !Task.isCancelled else { return }
            favorites = starred.items
        } catch { if current == epoch, request == vaultRequest { report(error) } }
    }

    func synchronize() async {
        activity()
        guard engine != nil, !busy else { return }
        let operation = beginBusyOperation(synchronizing: true), current = epoch
        defer { finishBusyOperation(operation) }
        do { try await coreOperation { try await syncPass(operation: $0, automatic: false) } }
        catch { if current == epoch { await updateSession(); report(error) } }
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

    func loadApprovals(refresh: Bool = false) async {
        guard let engine else { return }; let current = epoch
        do {
            if refresh { _ = try await coreOperation { try await engine.syncConnect(operation: $0) } }
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
                    epoch &+= 1; viewGeneration &+= 1; drafts = [:]; dirtyDrafts = []; vault = []; blockedAccounts = []; favorites = []; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; library = .empty; filesFolder = "root"; filesSearch = ""; approvals = []; session = nil
                    clearCloudBackups()
                    let engine = engine
                    Task { _ = try? await engine?.lock() }
                }
                if code == "TEST_DEPLOYMENT_UNAVAILABLE" { self.error = String(localized: "The test deployment is not configured. The local demo and privacy policy remain available.") }
                else if code == "APP_INTEGRITY_UNAVAILABLE" { self.error = String(localized: "App verification is unavailable on this device. Your saved content remains available.") }
                else if code == "FORBIDDEN" { self.error = String(localized: "The operation was cancelled or is not permitted.") }
                else if code == "VERSION_CONFLICT" { self.error = String(localized: "The record changed. Refresh and review your edits before saving again.") }
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
    var icon: AppGlyph {
        switch self { case .vault: .vault; case .messages: .channels; case .files: .folder; case .account: .account }
    }
}

/// One automatic attempt per real visit. Authentication dialogs make the app
/// inactive, so only backgrounding (or protected-data loss) rearms the request.
struct AutomaticUnlockPolicy {
    private var pending = true

    mutating func rearm() { pending = true }

    mutating func suppress() { pending = false }

    mutating func consume(locked: Bool, fastUnlock: Bool, busy: Bool) -> Bool {
        guard pending, locked, fastUnlock, !busy else { return false }
        pending = false
        return true
    }
}
