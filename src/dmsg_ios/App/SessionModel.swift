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
    private(set) var epoch: UInt64 = 0 { didSet { messageSending.reset() } }
    // Background invalidates reads through epoch while retaining this workspace's forms.
    private(set) var viewGeneration: UInt64 = 0 { didSet { settingsDrafts = [:] } }
    var concealed = true
    var busy = false
    private(set) var unlocking = false
    private(set) var synchronizing = false
    var contentSync: ContentSyncStatus?
    private var syncClock = Date.now
    var error: String?
    var notice: String? { didSet { noticeRevision &+= 1 } }
    private(set) var noticeRevision: UInt64 = 0
    /// Visible screens form a presentation stack; only its last screen shows feedback.
    var noticeHosts: [UUID] = []
    var reminderEntry: ReminderEntry?
    var vault: [VaultItem] = []
    var channels: [ChannelItem] = []
    private(set) var blockedAccounts: Set<String> = []
    var activeChannel: String?
    private(set) var contentRevision: UInt64 = 0
    private(set) var profileRevision: UInt64 = 0
    var vaultFilter = "all"
    var vaultTag = ""
    var vaultSort = "updated"
    private(set) var vaultCounts = VaultCounts()
    private(set) var vaultLoaded = false
    private(set) var vaultFailed = false
    var vaultQueryKey: String { [search, vaultFilter, vaultTag, vaultSort].joined(separator: "\u{0}") }
    var vaultQueryIsCurrent: Bool { vaultQuery == vaultQueryKey }
    private(set) var loadingVault = false
    private var vaultRequest: UUID?
    private var vaultQuery = ""
    /// Navigation/layout may recreate a detail column; drafts belong to the
    /// account session and are also persisted by the core.
    var drafts: [String: String] = [:]
    /// In-memory form drafts never cross a lock or account transition.
    var settingsDrafts: [String: Data] = [:]
    private(set) var accountIdentities: [String: LocalAccountIdentity] = [:]
    private(set) var deviceLabels: [String: String] = [:]
    let messageSending = MessageSending()
    private var dirtyDrafts: Set<String> = []
    var library: LibraryListing = .empty
    private(set) var cloudBackups: [CloudBackup] = []
    private(set) var cloudBackupsFailed = false
    private(set) var cloudBackupsKnown = false
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
    let fileTransfers = FileTransfers()
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
    private var nextAutomaticSync: ContinuousClock.Instant?
    private var lastLocalChange: UInt64?
    private var lastSyncThrough: UInt64?
    private var recoveryOperation: OperationToken?
    private var recoveryRequest: UUID?
    private var seenRecoveryDigests: [String: String] = [:]
    #if DEBUG
    let uiTesting = ProcessInfo.processInfo.arguments.contains("--ui-testing")
    var fixtureVaultItems: [VaultItem] = []
    var fixtureVaultHistory: [String: [VaultItem]] = [:]
    var fixtureVaultSaveAttempts = 0
    var fixtureVaultRefreshScheduled = false
    var fixtureChannelMessages: [MessageItem] = []
    var fixtureChannelRefreshes = 0
    var fixtureInboxLetters: [InboxLetter] = []
    var fixtureInboxRefreshes = 0
    #else
    let uiTesting = false
    #endif

    var syncCoolingDown: Bool {
        contentSync?.error == "SYNC_BUSY" && (contentSync?.retryAtMs ?? 0) > UInt64(syncClock.timeIntervalSince1970 * 1000)
    }

    var unlocked: Bool { session?.state == "unlocked" }
    var automaticSyncEnabled: Bool { session?.account.map { !manualSyncAccounts.contains($0) } ?? false }

    #if DEBUG
    func installFixtures(_ state: SessionSnapshot, items: [VaultItem]) {
        contentSync = .fixture(initialized: true)
        session = state; vault = items; fixtureVaultItems = items
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
            accountIdentities = preferences.accountIdentities ?? [:]
            deviceLabels = preferences.deviceLabels ?? [:]
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

    func rememberIdentity(name: String?, avatar: Data?) {
        guard unlocked, let account = session?.account else { return }
        let identity = LocalAccountIdentity(name: name, avatar: avatar)
        guard accountIdentities[account] != identity else { return }
        accountIdentities[account] = identity
        if let files {
            do { try writePreferences(at: files.root) } catch { report(error) }
        }
    }

    func rememberAccountNames(_ names: [String: String?], epoch: UInt64) {
        guard unlocked, epoch == self.epoch else { return }
        var changed = false
        for (account, name) in names {
            // The current account's own profile can be newer than its public
            // directory publication, just as in the desktop switcher.
            if account == session?.account, accountIdentities[account]?.name != nil { continue }
            let trimmed = name?.trimmingCharacters(in: .whitespacesAndNewlines)
            let identity = LocalAccountIdentity(name: trimmed?.isEmpty == false ? trimmed : nil,
                                                avatar: accountIdentities[account]?.avatar)
            if accountIdentities[account] != identity { accountIdentities[account] = identity; changed = true }
        }
        if changed, let files {
            do { try writePreferences(at: files.root) } catch { report(error) }
        }
    }

    func deviceLabel(_ device: String) -> String? {
        guard let account = session?.account else { return nil }
        return deviceLabels[account + "/" + device]
    }

    func setDeviceLabel(_ device: String, label: String) {
        guard unlocked, let account = session?.account else { return }
        let value = String(label.trimmingCharacters(in: .whitespacesAndNewlines).prefix(80))
        deviceLabels[account + "/" + device] = value.isEmpty ? nil : value
        if let files { do { try writePreferences(at: files.root) } catch { report(error) } }
    }

    func formDraft<T: Decodable>(_ key: String, as: T.Type) -> T? {
        settingsDrafts[key].flatMap { try? JSONDecoder().decode(T.self, from: $0) }
    }

    func keepFormDraft<T: Encodable>(_ key: String, value: T, generation: UInt64) {
        guard unlocked, generation == viewGeneration else { return }
        settingsDrafts[key] = try? JSONEncoder().encode(value)
    }

    @discardableResult
    func loadBlockedAccounts() async -> Bool {
        if uiTesting { return true }
        let current = epoch
        do {
            guard let values = try await engine?.blockedAccounts(), current == epoch else { return false }
            blockedAccounts = Set(values); return true
        } catch { return false }
    }

    @discardableResult
    func blockAccount(_ account: String, blocked: Bool) async -> Bool {
        let current = epoch
        do {
            guard let engine else { return false }
            _ = try await engine.setBlockedAccount(account: account, blocked: blocked)
            guard current == epoch else { return false }
            if blocked { blockedAccounts.insert(account) } else { blockedAccounts.remove(account) }
            return true
        } catch { if current == epoch { report(error) }; return false }
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
            accountIdentities.removeValue(forKey: account)
            deviceLabels = deviceLabels.filter { !$0.key.hasPrefix(account + "/") }
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

    func deliverMessage(channel: String, item: OutgoingMessage) {
        #if DEBUG
        if uiTesting {
            messageSending.start(channel: channel, item: item, work: {
                try await self.sendConversationFixture(item)
            })
            return
        }
        #endif
        guard let engine, unlocked, !concealed else { return }
        messageSending.start(channel: channel, item: item, work: {
            let result = try await coreOperation { try await engine.sendMessage(channel: channel, messageId: item.id, operation: $0) }
            return result.message
        }, finished: { [weak self] error in
            guard let self else { return }
            self.contentRevision &+= 1
            if let error, let failure = error as? MobileError, case .Failure(let code) = failure, code == "DEVICE_NOT_APPROVED" { self.report(error) }
            Task { await self.loadChannels() }
        })
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

    func activity() { lastActivity = .now }

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
            fileTransfers.stop()
            stopAutomaticSync(.sessionChanged); contentSync = nil
            nextAutomaticSync = nil; lastLocalChange = nil; lastSyncThrough = nil
            epoch &+= 1; viewGeneration &+= 1
            vault = []; blockedAccounts = []; vaultCounts = VaultCounts(); vaultLoaded = false; vaultFailed = false; vaultFilter = "all"; vaultTag = ""; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; drafts = [:]; library = .empty; filesFolder = "root"; filesSearch = ""; approvals = []; search = ""
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
        if enabled { manualSyncAccounts.remove(account); nextAutomaticSync = nil } else { manualSyncAccounts.insert(account); stopAutomaticSync(.disabled) }
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
                             syncIntroduced: syncIntroduced, notificationSetupDismissed: notificationSetupDismissed,
                             accountIdentities: accountIdentities, deviceLabels: deviceLabels).write(at: root)
    }

    func automaticSyncPass() async {
        guard automaticSyncEnabled, unlocked, !concealed, !busy, !automaticPassRunning,
              let account = session?.account else { return }
        guard let engine else { return }
        await updateSyncStatus()
        let change = try? await engine.localChange()
        let edited = change != nil && change != lastLocalChange && change != 0
        guard ContentSyncPolicy.isDue(status: contentSync, edited: edited,
            idleDue: nextAutomaticSync.map { ContinuousClock.now >= $0 } ?? true,
            now: UInt64(Date.now.timeIntervalSince1970 * 1000)) else { return }
        lastLocalChange = change
        if !syncIntroduced.contains(account) {
            // Said once per account, where it first syncs by itself.
            syncIntroduced.insert(account)
            notice = String(localized: "While dMsg is open and unlocked, your vault syncs automatically. Messages refresh when you open their conversation or inbox. File backups still require a separate confirmation. You can turn this off at any time.")
            if let files {
                do { try writePreferences(at: files.root) }
                catch { report(error) }
            }
        }
        await runAutomaticSync { try await self.syncPass(operation: $0, automatic: true) }
    }

    /// The pass belongs to the account session, so navigation never cancels it.
    func runAutomaticSync(_ work: @MainActor (OperationToken) async throws -> Void) async {
        guard !automaticPassRunning else { return }
        automaticPassRunning = true
        let operation = OperationToken(), current = epoch
        automaticOperation = operation
        defer { automaticOperation = nil; automaticPassRunning = false }
        do {
            try await withTaskCancellationHandler { try await work(operation) }
                onCancel: { operation.cancel() }
        } catch {
            if current == epoch, let failure = error as? MobileError, case .Failure(let code) = failure,
               code == "DEVICE_NOT_APPROVED" { report(error) }
        }
    }

    private func stopAutomaticSync(_ reason: ContentSyncEvent) {
        automaticOperation?.cancel()
        nextAutomaticSync = nil
        ContentSyncLog.record(reason)
    }

    func updateSyncStatus() async {
        guard unlocked, !concealed else { return }
        // The deadline must expire even when an otherwise equal status snapshot
        // does not notify Observation, including with automatic sync disabled.
        syncClock = .now
        guard let engine else { return }
        let current = epoch
        do {
            let status = try await engine.contentSyncStatus()
            guard current == epoch, !Task.isCancelled else { return }
            let changed = contentSync?.lastAttemptAtMs != status.lastAttemptAtMs || contentSync?.initialized != status.initialized
            contentSync = status
            // Inbox recovery also applies account content; publish it without
            // waiting for a separate automatic pass to finish.
            if changed { contentRevision &+= 1; await refresh() }
        } catch { /* Lifecycle changes invalidate the handle; the next visit reloads it. */ }
    }

    /// Vault completion is published before any independent service work.
    private func syncPass(operation: OperationToken, automatic: Bool) async throws {
        guard let engine, unlocked else { return }
        let current = epoch
        do {
            let value = try await engine.sync(automatic: automatic, operation: operation)
            guard current == epoch, !Task.isCancelled else { throw CancellationError() }
            struct Report: Decodable { let through: UInt64?; let sent: UInt64?; let paused: Bool?; let coalesced: Bool? }
            let report: Report = try decode(value)
            if report.paused != true && report.coalesced != true {
                let changed = report.through != nil && report.through != lastSyncThrough || (report.sent ?? 0) > 0
                nextAutomaticSync = .now.advanced(by: .seconds(changed ? 30 : 60))
            }
            lastSyncThrough = report.through ?? lastSyncThrough
            contentRevision &+= 1
            await updateSyncStatus()
            await updateSession()
        } catch {
            if current == epoch { contentRevision &+= 1; await updateSyncStatus(); await updateSession() }
            throw error
        }
    }

    /// Mailbox/approval work does not own the vault or fetch message bodies.
    func maintenancePass() async {
        guard unlocked, !concealed, let engine else { return }
        let current = epoch
        do { _ = try await coreOperation { try await engine.reconcile(operation: $0) } } catch { }
        guard current == epoch, !Task.isCancelled else { return }
        do { _ = try await coreOperation { try await engine.syncConnect(operation: $0) } } catch { }
        guard current == epoch, !Task.isCancelled else { return }
        _ = try? await engine.syncBlockedAccounts()
        await loadBlockedAccounts(); await loadApprovals()
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

    func inactive() { stopAutomaticSync(.inactive); lifecycleGeneration &+= 1; isForeground = false; concealed = true; if unlocked { privacyShield.show() } }

    func background() {
        stopAutomaticSync(.background)
        automaticUnlock.rearm()
        nextAutomaticSync = nil
        lifecycleGeneration &+= 1
        isForeground = false
        concealed = true
        recoveryOperation?.cancel()
        epoch &+= 1
        fileTransfers.stop()
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
        stopAutomaticSync(.locked); contentSync = nil
        automaticUnlock.rearm()
        (try? SystemExports(.files))?.lockFiles()
        lifecycleGeneration &+= 1
        isForeground = false
        fileTransfers.stop()
        transfer.cancel()
        deviceTransfer.reset()
        recoveryOperation?.cancel(); recoveryRequest = nil; checkingRecovery = false
        recoveryWarning = nil; recoveryRefreshFailed = false
        authenticationGeneration &+= 1; webLogin.cancel(); busy = false
        busyOperation = nil; synchronizing = false; unlocking = false
        privacyShield.show()
        concealed = true; epoch &+= 1; viewGeneration &+= 1; drafts = [:]; dirtyDrafts = []
        vault = []; blockedAccounts = []; vaultCounts = VaultCounts(); vaultLoaded = false; vaultFailed = false; vaultFilter = "all"; vaultTag = ""; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; library = .empty; filesFolder = "root"; filesSearch = ""; approvals = []; session = nil
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
        stopAutomaticSync(.locked); contentSync = nil
        notifications.clearVisibility()
        (try? SystemExports(.files))?.lockFiles()
        fileTransfers.stop()
        transfer.cancel()
        recoveryOperation?.cancel()
        epoch &+= 1
        nextAutomaticSync = nil; lastLocalChange = nil; lastSyncThrough = nil
        viewGeneration &+= 1; drafts = [:]; dirtyDrafts = []
        vault = []; blockedAccounts = []; vaultCounts = VaultCounts(); vaultLoaded = false; vaultFailed = false; vaultFilter = "all"; vaultTag = ""; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; library = .empty; filesFolder = "root"; filesSearch = ""; approvals = []; search = ""
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
        guard unlocked, !more || !loadingVault else { return }
        let current = epoch, key = vaultQueryKey, request = UUID()
        let count = more ? 60 : (key == vaultQuery ? max(60, vault.count) : 60)
        let offset = more ? vault.count : 0
        vaultRequest = request; loadingVault = true; vaultFailed = false
        defer { if vaultRequest == request { vaultRequest = nil; loadingVault = false } }
        do {
            #if DEBUG
            if uiTesting {
                let page = fixtureVaultPage(offset: offset, limit: count)
                vault = more ? vault + page.items : page.items
                vaultCounts = page.counts; vaultHasMore = page.more
                vaultLoaded = true; vaultQuery = key
                return
            }
            #endif
            guard let engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
            let result = try await engine.browseVault(request: VaultBrowseRequest(query: search,
                kind: ["all", "favorites"].contains(vaultFilter) ? nil : vaultFilter,
                favorites: vaultFilter == "favorites", tag: vaultTag.isEmpty ? nil : vaultTag,
                sort: vaultSort, offset: UInt32(offset), limit: UInt32(min(count, 10_000)), trash: false))
            guard current == epoch, request == vaultRequest, key == vaultQueryKey, !Task.isCancelled else { return }
            let page: VaultBrowsePage = try decode(result)
            vault = more ? vault + page.items.map(\.item) : page.items.map(\.item)
            vaultCounts = VaultCounts(total: page.total, all: page.all, favorites: page.favorites, kinds: page.kinds, tags: page.tags)
            vaultHasMore = page.hasMore; vaultLoaded = true; vaultQuery = key
        } catch {
            if current == epoch, request == vaultRequest, !Task.isCancelled { vaultFailed = true }
        }
    }

    func synchronize() async {
        activity(); syncClock = .now
        guard engine != nil, !busy, !syncCoolingDown else { return }
        let operation = beginBusyOperation(synchronizing: true), current = epoch
        defer { finishBusyOperation(operation) }
        async let backups: Void = refreshSyncBackups()
        do { try await coreOperation { try await syncPass(operation: $0, automatic: false) } }
        catch { if current == epoch { await updateSession(); report(error) } }
        await backups
    }

    private func refreshSyncBackups() async {
        // An independent result: a catalog failure cannot hide verified vault data.
        do { try await coreOperation { try await fetchCloudBackups(refresh: true, operation: $0) } }
        catch { /* fetchCloudBackups keeps its retryable failure state. */ }
    }

    func loadCloudBackups(refresh: Bool) async {
        // Reading a cache must not replace the identity of an in-flight refresh.
        if !refresh && loadingCloudBackups { return }
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
            struct Observation: Decodable { let complete: Bool }
            let observation: Observation = try decode(try await engine.cloudBackupStatus())
            guard !Task.isCancelled, current == epoch, cloudBackupRequest == request else { return }
            cloudBackupsKnown = observation.complete || !cloudBackups.isEmpty
            if refresh { cloudBackupsFailed = false }
        } catch {
            if !Task.isCancelled, current == epoch, cloudBackupRequest == request {
                // Inventory pages already committed before a later page failed
                // remain useful offline; keep the failure visible after rereading.
                if let saved = try? await engine.cloudBackups(refresh: false, operation: operation),
                   current == epoch, cloudBackupRequest == request {
                    cloudBackups = (try? decode(saved)) ?? cloudBackups
                }
                guard !Task.isCancelled, current == epoch, cloudBackupRequest == request else { return }
                cloudBackupsFailed = true; throw error
            }
        }
    }

    private func clearCloudBackups() {
        cloudBackups = []; cloudBackupsFailed = false; cloudBackupsKnown = false; cloudBackupRequest = nil
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
            if error.code == .fileReadTooLarge { self.error = String(localized: "This file exceeds the size limit for this operation."); return }
        }
        if let core = error as? MobileError {
            switch core {
            case .Failure(let code):
                if code == "CANCELLED" { return }
                // A stale UI handle must not lock a replacement account. The
                // lifecycle already clears views when it requests a lock.
                if code == "LOCKED" { return }
                if code == "DEVICE_NOT_APPROVED" {
                    epoch &+= 1; viewGeneration &+= 1; drafts = [:]; dirtyDrafts = []; vault = []; blockedAccounts = []; vaultCounts = VaultCounts(); vaultLoaded = false; vaultFailed = false; vaultFilter = "all"; vaultTag = ""; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; library = .empty; filesFolder = "root"; filesSearch = ""; approvals = []; session = nil
                    clearCloudBackups()
                    let engine = engine
                    Task { _ = try? await engine?.lock() }
                }
                if code == "TEST_DEPLOYMENT_UNAVAILABLE" { self.error = String(localized: "The test deployment is not configured.") }
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
                else if code == "SYNC_BUSY" { self.error = String(localized: "Sync is waiting for the server.") }
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
