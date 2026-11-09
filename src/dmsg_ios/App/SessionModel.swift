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
    private(set) var epoch: UInt64 = 0 { didSet { messageSending.reset(); letters.suspend(); resetFilePresentation() } }
    // Background invalidates reads through epoch while retaining this workspace's forms.
    private(set) var viewGeneration: UInt64 = 0 { didSet { settingsDrafts = [:]; letters.reset(); people.reset(); thumbnails.reset() } }
    var lifecycle = SessionLifecycle.inactive
    var concealed: Bool { lifecycle != .active }
    var busy: Bool { busyOperation != nil }
    var unlocking: Bool { busyOperation?.kind == .unlocking }
    var initializingAccount: Bool { busyOperation?.kind == .initializingAccount }
    var synchronizing: Bool { busyOperation?.kind == .synchronizing }
    var contentSync: ContentSyncStatus?
    private var syncClock = Date.now
    var error: String?
    private(set) var passkeySessionLimitCredential: Data?
    var canResetPasskeySessions: Bool { passkeySessionLimitCredential != nil }
    var signingIntoAnotherAccount = false
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
    // Category counts depend on search and tag, not the selected category or sort.
    private var vaultCountsKey: String { [search, vaultTag].joined(separator: "\u{0}") }
    var vaultCountsAreCurrent: Bool { vaultLoaded && vaultCountsQuery == vaultCountsKey }
    private var vaultCountsQuery = ""
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
    let letters = LetterWorkspace()
    let people = PeopleDirectory()
    let thumbnails = ThumbnailStore()
    private var dirtyDrafts: Set<String> = []
    var library: LibraryListing = .empty
    private(set) var cloudBackups: [CloudBackup] = []
    private(set) var cloudBackupsFailed = false
    private(set) var cloudBackupsKnown = false
    private var cloudBackupRequest: UUID?
    var loadingCloudBackups: Bool { cloudBackupRequest != nil }
    /// The folder on top of the Files stack.
    var filesFolder: String {
        for entry in route.filesPath.reversed() { if case .folder(let id) = entry { return id } }
        return "root"
    }
    /// What each visited folder held, so going back shows it at once.
    var filesListings: [String: LibraryListing] = [:]
    var filesSearch = ""
    var filesScope = FileScope.local
    var filesSort = FileSort.name
    var filesLoading = false
    var filesLoaded = false
    var filesFailed = false
    var filesRequest: UUID?
    var filesQuery = ""
    var filesRevision: UInt64 = 0
    var filesLocalOnly: LocalOnly?
    var filesPendingImports = 0
    var filesSharedImports = 0
    var incomingSharesArrived: UInt64 = 0
    @ObservationIgnored var announcedShares: Set<String> = []
    @ObservationIgnored private var tabRefreshedAt: [AppTab: Date] = [:]
    var fileOperations: Set<String> = []
    var cloudBackupsObservedAt: UInt64?
    var cloudBackupsComplete = false
    var cloudBackupsError: String?
    var filesQueryKey: String { filesFolder + "\u{0}" + filesSearch }
    var filesQueryIsCurrent: Bool { filesQuery == filesQueryKey }
    var approvals: [ApprovalView] = []
    private(set) var approvalsFailed = false
    private(set) var recoveryWarning: RecoveryObservation?
    private(set) var recoveryRefreshFailed = false
    private(set) var checkingRecovery = false
    var selectedTab: AppTab = .vault {
        didSet {
            if selectedTab != oldValue, let files { try? writePreferences(at: files.root) }
        }
    }
    /// Where the person is in each tab. Unlocking returns there; it stays in memory,
    /// so a restart or another account starts fresh.
    var route = WorkspaceRoute()
    private var routeAccount: String?
    /// Accounts that dismissed the reminder about files without a cloud backup.
    private(set) var backupHintDismissed: Set<String> = []
    /// Space the active screen keeps free at the bottom, such as the tab bar or keyboard.
    var screenBottomInset: CGFloat = 0
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
    let passkeyLogin = PasskeyLogin()
    private let privacyShield = PrivacyShield()
    private var backgroundAt: ContinuousClock.Instant?
    private var lastActivity = ContinuousClock.now
    private var security: AppleSecurity?
    private var opening = false
    private var closing: Task<Void, Never>?
    private var authenticationGeneration: UInt64 = 0
    private var lifecycleGeneration: UInt64 = 0
    private var backgroundFinish: (operation: OperationToken, task: UIBackgroundTaskIdentifier)?
    private var cursorReports: [String: Task<Void, Never>] = [:]
    private var channelChecked: [String: ContinuousClock.Instant] = [:]
    private var channelReadAhead: [String: ContinuousClock.Instant] = [:]
    private var isForeground: Bool { lifecycle == .active || lifecycle == .resuming }
    private var automaticUnlock = AutomaticUnlockPolicy()
    private var busyOperation: (id: UUID, kind: SessionOperation)?
    private var lastSessionSequence: UInt64 = 0
    private var fixtureLoaded = false
    /// Accounts that turned automatic sync off; every other account syncs.
    private var manualSyncAccounts: Set<String> = []
    /// Accounts already told that they sync automatically.
    private var syncIntroduced: Set<String> = []
    private var automaticPassRunning = false
    private var automaticOperation: OperationToken?
    private var unlockRetryDue = false
    /// The relay is telling this device what moved, so polling is only a backstop.
    private(set) var hintsConnected = false
    /// Counts hints per channel; an open conversation reads when its count moves.
    private(set) var channelHints: [String: UInt64] = [:]
    /// Moves on each letter hint; an open inbox reads when it does.
    private(set) var inboxHints: UInt64 = 0
    private var recoveryOperation: OperationToken?
    private var recoveryRequest: UUID?
    private var seenRecoveryDigests: [String: String] = [:]
    #if DEBUG
    let uiTesting = ProcessInfo.processInfo.arguments.contains("--ui-testing")
    var fixtureFiles: [LibraryFile] = []
    var fixtureFileFolders: [FolderItem] = []
    var fixtureFileVersions: [String: [LibraryVersion]] = [:]
    var fixtureFileLoads = 0
    var fixtureFileCloudLoads = 0
    var fixtureFileSaves = 0
    var fixtureFileMoveFailed = false
    var fixtureFileLinks: [FileLink] = []
    var fixtureFileRequests: [FileRequest]?
    var fixtureReceivedFiles: [ReceivedFile] = []
    var fixtureFileRequestAttempts = 0
    var fixtureFileLinkAttempts = 0
    var fixtureFileLinkRefreshes = 0
    var fixtureVaultItems: [VaultItem] = []
    var fixtureVaultHistory: [String: [VaultItem]] = [:]
    var fixtureVaultSaveAttempts = 0
    var fixtureVaultFieldReadAttempts = 0
    var fixtureVaultRefreshScheduled = false
    var fixtureChannelMessages: [MessageItem] = []
    var fixtureChannelRefreshes = 0
    var fixtureInboxLetters: [InboxLetter] = []
    var fixtureInboxRefreshes = 0
    var fixtureInboxComplete = false
    var fixtureInitializationAttempts = 0
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
        cloudBackups = [CloudBackup(uploadId: "fixture-backup", name: "Project brief.pdf", size: 2048, bytes: 4096, needsRoot: false, local: false,
                                   fileId: "fixture-cloud", versionId: "v1", path: ["Projects"], mime: "application/pdf", createdAt: 1_793_000_000_000,
                                   committedAt: 1_793_000_100_000, device: "fixture-device")]
    }

    func installEngineFixture(_ engine: MobileEngine) async throws {
        self.engine = engine
        try adopt(try await engine.sessionState())
    }

    func setFileCloudFixture(_ backups: [CloudBackup], failed: Bool = false) {
        cloudBackups = backups; cloudBackupsKnown = true; cloudBackupsComplete = !failed
        cloudBackupsFailed = failed; cloudBackupsError = failed ? String(localized: "Connection unavailable. Your saved content stays on this device.") : nil
        cloudBackupsObservedAt = 1_793_000_100_000
    }
    #endif

    private(set) var environment = "production"

    func start() async {
        guard !opening, engine == nil else { return }
        lifecycle = UIApplication.shared.applicationState == .active ? .resuming : .inactive
        opening = true; defer { opening = false }
        #if DEBUG
        if uiTesting {
            if !fixtureLoaded { loadFixtures(); fixtureLoaded = true }
            // Reproduce an unlocked view appearing before authentication uncovers it.
            if ProcessInfo.processInfo.arguments.contains("--ui-account-unlock") {
                selectedTab = .account; lifecycle = .resuming
                try? await Task.sleep(for: .seconds(2))
            }
            lifecycle = .active; return
        }
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
            backupHintDismissed = preferences.backupHintDismissed ?? []
            selectedTab = preferences.lastTab.flatMap(AppTab.init(rawValue:)) ?? .vault
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

    func profileDidChange() { people.invalidate(); profileRevision &+= 1 }

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

    /// Replace the AutoFill snapshot and the identities the system suggests;
    /// false when the session moved on meanwhile.
    func publishCredentials(_ entries: [AutoFillEntry], biometryOnly: Bool, account: String, epoch current: UInt64, replacing snapshot: String? = nil) async throws -> Bool {
        let selected = entries.map { CredentialInput(title: $0.title, username: $0.username, password: $0.password, origin: AutoFillEntry.origin($0.url)) }
        let store = try SystemExports(.credentials)
        let prepared = try await Task.detached { try store.prepareCredentials(selected, account: account, expires: UInt64(Date.now.timeIntervalSince1970 * 1000) + 86_400_000) }.value
        guard current == epoch, !Task.isCancelled,
              snapshot == nil || (try? store.header(checkExpiry: false).id) == snapshot else { store.discard(prepared); return false }
        do { try store.publish(prepared, biometryOnly: biometryOnly) } catch { store.discard(prepared); throw error }
        let metadata = try store.credentialMetadata(OpenExport(header: prepared.header, key: prepared.key))
        let identities = metadata.filter { !$0.origin.isEmpty }.map { ASPasswordCredentialIdentity(serviceIdentifier: ASCredentialServiceIdentifier(identifier: $0.origin, type: .URL), user: $0.username, recordIdentifier: prepared.header.id + ":" + $0.id) }
        try await ASCredentialIdentityStore.shared.replaceCredentialIdentities(identities)
        return current == epoch
    }

    /// An existing AutoFill snapshot follows the items the person chose,
    /// without asking again, and renews before it expires. A failure leaves
    /// the snapshot as it is until the next try.
    func refreshAutoFill() async {
        struct Security: Decodable { let boundBiometryOnly: Bool? }
        guard let engine, let account = session?.account, unlocked, !concealed,
              let store = try? SystemExports(.credentials), let header = try? store.header(checkExpiry: false),
              header.accountHash == IncomingShares.accountHash(account) else { return }
        let current = epoch, renewing = UInt64(Date.now.timeIntervalSince1970 * 1000) + 12 * 3_600_000
        do {
            let refresh: AutoFillRefresh? = try decode(try await engine.refreshAutofill(force: header.expires < renewing))
            guard current == epoch, !Task.isCancelled, let refresh, refresh.changed,
                  (try? store.header(checkExpiry: false).id) == header.id else { return }
            if refresh.entries.isEmpty {
                try store.invalidate()
                try await ASCredentialIdentityStore.shared.removeAllCredentialIdentities()
            } else {
                let security: Security = try decode(try await engine.hardwareSecurity())
                guard try await publishCredentials(refresh.entries, biometryOnly: refresh.biometryOnly || security.boundBiometryOnly == true, account: account, epoch: current, replacing: header.id) else { return }
            }
            guard current == epoch else { return }
            try await engine.autofillPublished(digest: refresh.digest)
        } catch { }
    }

    func dismissBackupHint() {
        guard let account = session?.account, backupHintDismissed.insert(account).inserted else { return }
        if let files { try? writePreferences(at: files.root) }
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
            manualSyncAccounts.remove(account); syncIntroduced.remove(account); notificationSetupDismissed.remove(account); backupHintDismissed.remove(account)
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

    /// The read position follows to this account's other devices, once reading pauses.
    func channelRead(_ channel: String, through: UInt64) async {
        guard let engine else { return }
        try? await engine.markRead(channel: channel, through: through)
        cursorReports[channel]?.cancel()
        cursorReports[channel] = Task {
            do { try await Task.sleep(for: .seconds(3)) } catch { return }
            _ = try? await coreOperation { try await engine.reportChannelCursor(channel: channel, operation: $0) }
        }
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

    func beginBusyOperation(_ kind: SessionOperation = .other) -> UUID {
        let token = UUID(); busyOperation = (token, kind)
        return token
    }

    @discardableResult
    func finishBusyOperation(_ token: UUID) -> Bool {
        guard busyOperation?.id == token else { return false }
        busyOperation = nil
        return true
    }

    private func adopt(_ state: SessionSnapshot?) throws {
        guard let state else { session = nil; return }
        guard state.sequence >= lastSessionSequence, let engine else { return }
        let handle = try engine.scoped(generation: state.generation)
        if recoveryWarning?.account != state.account {
            recoveryWarning = nil; recoveryRefreshFailed = false
        }
        if routeAccount != state.account { route = WorkspaceRoute(); routeAccount = state.account }
        if (session?.state == "unlocked" && state.state != "unlocked") ||
            (session?.account != nil && session?.account != state.account) ||
            (session?.deviceId != nil && session?.deviceId != state.deviceId) {
            fileTransfers.stop()
            stopAutomaticSync(.sessionChanged); contentSync = nil
            passkeySessionLimitCredential = nil
            epoch &+= 1; viewGeneration &+= 1
            vault = []; blockedAccounts = []; vaultCounts = VaultCounts(); vaultLoaded = false; vaultFailed = false; vaultFilter = "all"; vaultTag = ""; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; drafts = [:]; library = .empty; filesListings = [:]; filesSearch = ""; approvals = []; search = ""
            clearCloudBackups()
        }
        if session?.state != "unlocked" && state.state == "unlocked" { unlockRetryDue = true }
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
        lifecycle = .active; privacyShield.hide()
    }

    /// On for every account until the person turns it off in settings,
    /// which this device keeps for that account.
    func setAutomaticSync(_ enabled: Bool) {
        guard let account = session?.account else { return }
        if enabled { manualSyncAccounts.remove(account) } else { manualSyncAccounts.insert(account); stopAutomaticSync(.disabled) }
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
                             accountIdentities: accountIdentities, deviceLabels: deviceLabels,
                             lastTab: selectedTab.rawValue, backupHintDismissed: backupHintDismissed).write(at: root)
    }

    /// The engine decides when a pass is due and says what moved; this keeps
    /// the views current. It belongs to the account session, so navigation
    /// never cancels it.
    func watchContentSync() async {
        guard let engine, let account = session?.account else {
            // Fixtures have no engine; only the status's deadline moves.
            while !Task.isCancelled {
                syncClock = .now
                do { try await Task.sleep(for: .seconds(2)) } catch { return }
            }
            return
        }
        if unlockRetryDue {
            unlockRetryDue = false
            _ = try? await engine.retrySyncEarly(network: false)
        }
        let automatic = automaticSyncEnabled
        if automatic && !syncIntroduced.contains(account) {
            // Said once per account, where it first syncs by itself.
            syncIntroduced.insert(account)
            notice = String(localized: "Your vault syncs automatically while dMsg is open.")
            if let files {
                do { try writePreferences(at: files.root) }
                catch { report(error) }
            }
        }
        let (events, continuation) = AsyncStream.makeStream(of: ContentSyncBridge.Event.self)
        let bridge = ContentSyncBridge(continuation)
        let driver = Task { [weak self] in
            // A foreground that is still resuming refuses the first attempt.
            while !Task.isCancelled {
                await self?.runAutomaticSync { try await engine.runContentSync(automatic: automatic, observer: bridge, operation: $0) }
                try? await Task.sleep(for: .seconds(2))
            }
            continuation.finish()
        }
        defer { driver.cancel() }
        let current = epoch
        for await event in events {
            guard current == epoch else { return }
            // Covered views stay as they are; coming back reads the status again.
            if concealed { continue }
            switch event {
            case .status(let status):
                await adoptSyncStatus(status)
            case let .passed(changed, error):
                if let error, error == "DEVICE_NOT_APPROVED" { report(MobileError.Failure(code: error)) }
                if changed { contentRevision &+= 1; await refresh() }
                await readSession()
            }
        }
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
        ContentSyncLog.record(reason)
    }

    func watchNetwork() async {
        var online = true
        for await satisfied in NetworkPath.updates() {
            if satisfied && !online { await networkReturned() }
            online = satisfied
        }
    }

    /// Polling is a backstop while hints arrive: five times as long, at least five minutes.
    /// Polling is the first thing to give up when the device is short of power or running hot.
    func pollSeconds(_ alone: Int) -> Int {
        let seconds = hintsConnected ? max(alone * 5, 300) : alone
        let device = ProcessInfo.processInfo
        return device.isLowPowerModeEnabled || device.thermalState == .serious || device.thermalState == .critical ? seconds * 3 : seconds
    }

    /// Going back and forth between tabs shows what is there; a recent look need not be repeated.
    func refreshSelectedTab() async {
        if let at = tabRefreshedAt[selectedTab], Date.now.timeIntervalSince(at) < 30 { return }
        tabRefreshedAt[selectedTab] = .now
        await refresh()
    }

    func watchHints() async {
        guard let engine else { return }
        let current = epoch
        let (events, continuation) = AsyncStream.makeStream(of: HintBridge.Event.self)
        let listening = Task {
            // A foreground that is still resuming refuses the first attempt.
            while !Task.isCancelled {
                _ = try? await coreOperation { try await engine.listenHints(observer: HintBridge(continuation), operation: $0) }
                try? await Task.sleep(for: .seconds(2))
            }
            continuation.finish()
        }
        defer { listening.cancel(); hintsConnected = false }
        for await event in events {
            guard current == epoch, !Task.isCancelled else { return }
            switch event {
            case .connected(let connected):
                hintsConnected = connected
                // Hints sent while it was down are lost: catch up.
                if connected { await hintChannelConnected() }
            case let .hint(kind, resource, news):
                receiveHint(kind: kind, resource: resource, news: news)
            }
        }
    }

    /// The latest conversations, for the list: how far each reaches, at most
    /// once a minute, without their messages.
    func checkChannels() async {
        guard let engine, unlocked, !concealed else { return }
        let current = epoch, now = ContinuousClock.now
        let due = channels.prefix(20).map(\.id).filter { channelChecked[$0].map { $0.duration(to: now) >= .seconds(60) } ?? true }
        for id in due {
            channelChecked[id] = now
            _ = try? await coreOperation { try await engine.readChannelControls(channel: id, operation: $0) }
            guard current == epoch, !Task.isCancelled else { return }
        }
        await loadChannels()
        await readChannelsAhead()
    }

    /// While on screen, a conversation with news that is not muted has its
    /// latest page read ahead, at most every 30 seconds.
    private func readChannelsAhead() async {
        guard let engine, unlocked, !concealed, isForeground else { return }
        let current = epoch, now = ContinuousClock.now
        let due = channels.filter { channel in
            channel.news && !channel.muted && channel.id != activeChannel
                && (channelReadAhead[channel.id].map { $0.duration(to: now) >= .seconds(30) } ?? true)
        }
        guard !due.isEmpty else { return }
        for channel in due.prefix(8) {
            channelReadAhead[channel.id] = now
            _ = try? await coreOperation { try await engine.readChannelTail(channel: channel.id, operation: $0) }
            guard current == epoch, !Task.isCancelled else { return }
        }
        await loadChannels()
    }

    private func hintChannelConnected() async {
        await networkReturned(redial: false)
        await checkChannels()
        if let activeChannel { channelHints[activeChannel, default: 0] &+= 1 }
        inboxHints &+= 1
        await notifications.refresh(self, receive: true)
    }

    private func receiveHint(kind: String, resource: String?, news: Bool) {
        let current = epoch
        // The relay hints at most every five seconds: read once more after that.
        func again(_ work: @escaping @MainActor () async -> Void) {
            Task {
                do { try await Task.sleep(for: .seconds(6)) } catch { return }
                if current == epoch, !concealed { await work() }
            }
        }
        switch kind {
        case "content":
            if news { engine?.requestContentSync() }
            again { self.engine?.requestContentSync() }
        case "channel":
            guard let resource else { return }
            Task { await refreshHintedChannel(resource) }
            channelHints[resource, default: 0] &+= 1
            again {
                self.channelHints[resource, default: 0] &+= 1
                await self.refreshHintedChannel(resource)
            }
        case "read":
            // Another device of this account read further: the channel's head says how far.
            guard let resource, let engine else { return }
            Task {
                _ = try? await coreOperation { try await engine.readChannelControls(channel: resource, operation: $0) }
                if current == epoch { await loadChannels() }
            }
        case "inbox":
            inboxHints &+= 1
            Task { await notifications.refresh(self, receive: true) }
        case "notification":
            Task { await notifications.refresh(self, receive: true) }
        case "connect":
            Task { await maintenancePass() }
        default: break
        }
    }

    /// The delayed read also updates the list when no conversation is open.
    private func refreshHintedChannel(_ channel: String) async {
        guard let engine, unlocked, !concealed else { return }
        let current = epoch
        _ = try? await coreOperation { try await engine.readChannelControls(channel: channel, operation: $0) }
        guard current == epoch, !Task.isCancelled else { return }
        await loadChannels()
        await readChannelsAhead()
    }

    /// A waiting retry starts now; the automatic pass picks it up.
    private func networkReturned(redial: Bool = true) async {
        guard let engine, unlocked, !concealed else { return }
        if redial { engine.reconnectHints() }
        _ = try? await engine.retrySyncEarly(network: true)
        engine.requestContentSync()
        await recoverMessages()
    }

    /// Sends an interruption left open leave again, under their own IDs and in
    /// their original order per channel.
    private func recoverMessages() async {
        guard let engine else { return }
        let current = epoch
        guard let waiting = try? await engine.recoverableMessages(), current == epoch else { return }
        for row in waiting where !messageSending.contains(row.message.id) {
            deliverMessage(channel: row.channel, item: row.message)
        }
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
            await adoptSyncStatus(status)
        } catch { /* Lifecycle changes invalidate the handle; the next visit reloads it. */ }
    }

    private func adoptSyncStatus(_ status: ContentSyncStatus) async {
        syncClock = .now
        let changed = contentSync?.through != status.through || contentSync?.initialized != status.initialized
        contentSync = status
        // Inbox recovery also applies account content; publish it without
        // waiting for a separate automatic pass to finish. A pass that
        // verified nothing new leaves the views alone.
        if changed { contentRevision &+= 1; await refresh() }
    }

    /// Vault completion is published before any independent service work.
    private func syncPass(operation: OperationToken, automatic: Bool) async throws {
        guard let engine, unlocked else { return }
        let current = epoch
        do {
            let value = try await engine.sync(automatic: automatic, operation: operation)
            guard current == epoch, !Task.isCancelled else { throw CancellationError() }
            struct Report: Decodable { let sent: UInt64?; let blocked: UInt64?; let changes: UInt64? }
            let report: Report = try decode(value)
            let revision = contentRevision
            await updateSyncStatus()
            if contentRevision == revision, (report.changes ?? 0) + (report.sent ?? 0) + (report.blocked ?? 0) > 0 {
                contentRevision &+= 1; await refresh()
            }
            await readSession()
        } catch {
            if current == epoch { await updateSyncStatus(); await readSession() }
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
        _ = try? await engine.maintainFiles()
        await loadBlockedAccounts(); await loadApprovals()
    }

    func loadChannels() async {
        guard let engine, unlocked else { return }; let current = epoch
        do {
            let result = try await engine.channels()
            guard current == epoch, !Task.isCancelled else { return }; channels = result.items
        } catch { if current == epoch { reportBackground(error) } }
    }

    /// The counts and state a pass moved, without reloading any view.
    private func readSession() async {
        let current = epoch
        do {
            let state = try await engine?.sessionState()
            guard current == epoch else { return }; try adopt(state)
        } catch { reportBackground(error) }
    }

    @discardableResult
    func updateSession() async -> Bool {
        guard let engine else { return uiTesting }
        let current = epoch
        do {
            let state = try await engine.sessionState()
            guard current == epoch else { return false }; try adopt(state)
            if unlocked { await refresh() }
            return true
        } catch { report(error); return false }
    }

    func idleTick() async {
        if unlocked && !concealed && lastActivity.duration(to: .now) >= .seconds(15 * 60) { await lock() }
    }

    func inactive() { lifecycleGeneration &+= 1; lifecycle = .inactive; if unlocked { privacyShield.show() } }

    func background() {
        let letterDrafts = letters.pendingInputs
        let messageDrafts = dirtyDrafts.compactMap { channel in drafts[channel].map { MessageDraft(channel: channel, text: $0) } }
        let account = session?.account
        let finishing = unlocked && !immediateBackgroundLock
        stopAutomaticSync(.background)
        automaticUnlock.rearm()
        lifecycleGeneration &+= 1
        lifecycle = .background
        recoveryOperation?.cancel()
        epoch &+= 1
        fileTransfers.stop()
        transfer.cancel()
        deviceTransfer.background()
        backgroundAt = .now
        if finishing, let engine {
            finishInBackground(engine)
        } else {
            engine?.suspend()
        }
        if immediateBackgroundLock {
            let engine = engine
            clearContent(clearClipboard: false)
            if let account, !letterDrafts.isEmpty || !messageDrafts.isEmpty {
                closing = Task {
                    do { _ = try await engine?.lockWithDrafts(account: account, drafts: messageDrafts, letters: letterDrafts) }
                    catch { engine?.lockNow() }
                }
            } else { engine?.lockNow() }
        }
    }

    /// What the person just did leaves before the app stops: sends an
    /// interruption left open, then already signed versions. The screen is covered.
    private func finishInBackground(_ engine: MobileEngine) {
        endBackgroundFinish()
        let operation = OperationToken(), syncing = automaticSyncEnabled
        let task = UIApplication.shared.beginBackgroundTask(withName: "dMsg finish") { [weak self] in
            self?.endBackgroundFinish(); engine.suspend()
        }
        guard task != .invalid else { engine.suspend(); return }
        backgroundFinish = (operation, task)
        Task { [weak self] in
            for row in (try? await engine.recoverableMessages()) ?? [] {
                _ = try? await engine.sendMessage(channel: row.channel, messageId: row.message.id, operation: operation)
            }
            if syncing {
                _ = try? await engine.flushContent(operation: operation)
            }
            guard let self, self.backgroundFinish?.task == task else { return }
            self.endBackgroundFinish(); engine.suspend()
        }
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(20))
            guard let self, self.backgroundFinish?.task == task else { return }
            self.endBackgroundFinish(); engine.suspend()
        }
    }

    private func endBackgroundFinish() {
        guard let finish = backgroundFinish else { return }
        backgroundFinish = nil
        finish.operation.cancel()
        UIApplication.shared.endBackgroundTask(finish.task)
    }

    func foreground() async {
        guard UIApplication.shared.applicationState == .active else { return }
        endBackgroundFinish()
        lifecycle = .resuming
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
            await updateSyncStatus()
            await networkReturned()
            await letters.flush(self)
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
        endBackgroundFinish()
        stopAutomaticSync(.locked); contentSync = nil
        automaticUnlock.rearm()
        (try? SystemExports(.files))?.lockFiles()
        lifecycleGeneration &+= 1
        lifecycle = .protectedDataUnavailable
        fileTransfers.stop()
        transfer.cancel()
        deviceTransfer.reset()
        recoveryOperation?.cancel(); recoveryRequest = nil; checkingRecovery = false
        recoveryWarning = nil; recoveryRefreshFailed = false
        authenticationGeneration &+= 1; webLogin.cancel(); passkeyLogin.cancel(); passkeySessionLimitCredential = nil
        busyOperation = nil
        privacyShield.show()
        epoch &+= 1; viewGeneration &+= 1; drafts = [:]; dirtyDrafts = []
        vault = []; blockedAccounts = []; vaultCounts = VaultCounts(); vaultLoaded = false; vaultFailed = false; vaultFilter = "all"; vaultTag = ""; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; library = .empty; filesListings = [:]; filesSearch = ""; approvals = []; session = nil
        clearCloudBackups()
        if let engine { engine.suspend(); engine.lockNow(); closing = Task { _ = try? await engine.close() } }
        engine = nil; security?.cancelPresence()
    }

    func lock(clearClipboard: Bool = true) async {
        if clearClipboard { automaticUnlock.suppress() }
        if clearClipboard { lifecycleGeneration &+= 1 }
        let account = session?.account, engine = engine
        let drafts = dirtyDrafts.compactMap { channel in self.drafts[channel].map { MessageDraft(channel: channel, text: $0) } }
        let letterDrafts = letters.pendingInputs
        let flush = unlocked && account != nil && (!drafts.isEmpty || !letterDrafts.isEmpty)
        clearContent(clearClipboard: clearClipboard)
        deviceTransfer.reset()
        let work = beginBusyOperation(); defer { finishBusyOperation(work) }
        do {
            if flush, let account { try adopt(try await engine?.lockWithDrafts(account: account, drafts: drafts, letters: letterDrafts)) }
            else { try adopt(try await engine?.lock()) }
        } catch {
            _ = try? await engine?.lock()
            await updateSession(); report(error)
        }
    }

    private func clearContent(clearClipboard: Bool) {
        if clearClipboard { signingIntoAnotherAccount = false }
        stopAutomaticSync(.locked); contentSync = nil
        notifications.clearVisibility()
        (try? SystemExports(.files))?.lockFiles()
        fileTransfers.stop()
        transfer.cancel()
        recoveryOperation?.cancel()
        epoch &+= 1
        viewGeneration &+= 1; drafts = [:]; dirtyDrafts = []
        vault = []; blockedAccounts = []; vaultCounts = VaultCounts(); vaultLoaded = false; vaultFailed = false; vaultFilter = "all"; vaultTag = ""; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; library = .empty; filesListings = [:]; filesSearch = ""; approvals = []; search = ""
        clearCloudBackups()
        notice = nil; error = nil; passkeySessionLimitCredential = nil
        busyOperation = nil
        if clearClipboard { clipboard.clearOwned() }
        if clearClipboard { authenticationGeneration &+= 1; webLogin.cancel(); passkeyLogin.cancel(); engine?.cancelLogin() }
        security?.cancelPresence()
        // Remove the unlocked UI before awaiting the Rust barrier.
        session = nil
    }

    func login(usingInternetIdentity: Bool = false, passkeyOperation: PasskeyOperation = .login, credentialID: Data? = nil) async {
        guard let engine, !busy else { return }
        let resetting = passkeyOperation == .resetSessions
        let credential = resetting ? passkeySessionLimitCredential : credentialID
        guard !resetting || credential != nil else { return }
        automaticUnlock.suppress()
        let operation = beginBusyOperation()
        error = nil
        if !resetting { passkeySessionLimitCredential = nil }
        activity()
        authenticationGeneration &+= 1
        let current = authenticationGeneration
        var credentialCreated = false
        var selectedCredential = credential
        let usePasskey = !usingInternetIdentity && (passkeyOperation != .login || session?.passkeyAvailable == true)
        defer { finishBusyOperation(operation) }
        do {
            let state: SessionSnapshot
            if usePasskey {
                let start = try await engine.beginPasskey(operation: passkeyOperation, credentialId: credential)
                let response = try await passkeyLogin.authenticate(start)
                selectedCredential = resetting ? credential : response.credentialId
                credentialCreated = passkeyOperation == .register
                await waitUntilActive()
                guard current == authenticationGeneration else { return }
                _ = try await engine.resume(expectedGeneration: engine.controlGeneration())
                state = try await engine.completePasskey(state: start.state, response: response)
            } else {
                let start = try await engine.beginLogin()
                guard let url = URL(string: start.url) else { throw CocoaError(.coderInvalidValue) }
                let callback = try await webLogin.authenticate(url: url)
                await waitUntilActive()
                guard current == authenticationGeneration else { return }
                _ = try await engine.resume(expectedGeneration: engine.controlGeneration())
                state = try await engine.completeLogin(callback: callback.absoluteString)
            }
            guard current == authenticationGeneration else { return }
            try adopt(state)
            if resetting {
                passkeySessionLimitCredential = nil
                finishBusyOperation(operation)
                await login(passkeyOperation: .login, credentialID: credential)
                return
            }
            signingIntoAnotherAccount = false; uncoverIfActive()
            if unlocked { await refresh() }
        } catch { if current == authenticationGeneration {
            engine.cancelLogin(); await updateSession()
            reportLoginFailure(error, usingPasskey: usePasskey, operation: passkeyOperation, credentialCreated: credentialCreated, credentialID: selectedCredential)
        } }
    }

    func reportLoginFailure(_ failure: Error, usingPasskey: Bool, operation: PasskeyOperation, credentialCreated: Bool = false, credentialID: Data? = nil) {
        if failure is CancellationError || Task.isCancelled { return }
        if let failure = failure as? MobileError, case .Failure(let code) = failure, code == "CANCELLED" || code == "LOCKED" { return }
        report(failure)
        passkeySessionLimitCredential = operation == .resetSessions ? credentialID : nil
        if let failure = failure as? MobileError, case .Failure(let code) = failure {
            if usingPasskey && operation == .login && code == "SESSION_LIMIT_EXCEEDED" {
                passkeySessionLimitCredential = credentialID
                self.error = String(localized: "This passkey has too many signed-in devices. Sign out its devices to continue. Other passkeys and saved content are not affected.")
            }
            if code == "QUOTA_EXCEEDED" {
                self.error = String(localized: "The operation could not be completed. Please try again.")
            }
            if usingPasskey && operation == .login && code == "NOT_FOUND" {
                self.error = String(localized: "This passkey is not registered with dMsg. Choose another passkey. If an earlier creation failed, remove that unused passkey from your password manager before creating a new one.")
            }
        }
        if usingPasskey && operation == .register && credentialCreated {
            self.error = String(localized: "Your password manager saved the passkey, but dMsg could not confirm registration. Try signing in with it before creating another passkey.")
        }
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
        let operation = beginBusyOperation(.unlocking)
        error = nil
        activity()
        let current = epoch
        defer { finishBusyOperation(operation) }
        do {
            let state: SessionSnapshot
            if session?.fastUnlock == true { state = try await engine.unlockFast() }
            else { state = try await engine.refreshAccount() }
            guard current == epoch else { return }
            try adopt(state); await refresh()
        } catch { if current == epoch { await updateSession(); report(error) } }
    }

    func initializeAccount() async {
        guard !busy, engine != nil || uiTesting else { return }
        automaticUnlock.suppress()
        let operation = beginBusyOperation(.initializingAccount), current = epoch
        error = nil
        activity()
        defer { finishBusyOperation(operation) }
        for attempt in 0..<2 {
            do {
                let state = try await initializeAccountState()
                guard current == epoch, !Task.isCancelled else { return }
                try adopt(state)
                if unlocked { await refresh() }
                return
            } catch {
                guard current == epoch, !Task.isCancelled else { return }
                // Even a failed transition fences its old handle. Refresh it
                // before reconciling, or the next attempt only returns LOCKED.
                let refreshed = await updateSession()
                guard current == epoch, !Task.isCancelled else { return }
                if refreshed, attempt == 0, case MobileError.Failure(code: "EXECUTION_UNKNOWN") = error {
                    // The core resumes the saved account/root operation and
                    // checks its receipt; it does not create a second account.
                    continue
                }
                if case MobileError.Failure(code: "EXECUTION_UNKNOWN") = error {
                    self.error = String(localized: "Account setup is not confirmed yet. Check your connection and try again to continue the saved setup.")
                } else { report(error) }
                return
            }
        }
    }

    private func initializeAccountState() async throws -> SessionSnapshot {
        #if DEBUG
        if uiTesting { return try await initializeAccountFixture() }
        #endif
        guard let engine else { throw MobileError.Failure(code: "LOCKED") }
        return try await engine.initializeAccount()
    }

    private func unlockAutomaticallyIfNeeded() async {
        guard isForeground, !concealed, engine != nil, !signingIntoAnotherAccount,
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
            // The list is local to read, and the Messages tab's badge counts it.
            let result = try await engine.channels()
            guard current == epoch else { return }; if channels != result.items { channels = result.items }
            switch selectedTab {
            case .vault: await loadVault()
            case .messages: await loadBlockedAccounts()
            case .files:
                await loadFiles(quiet: true)
                await loadCloudBackups(refresh: false)
            case .account: break
            }
        } catch { if current == epoch { reportBackground(error) } }
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
                if ProcessInfo.processInfo.arguments.contains("--ui-vault-slow-load") {
                    try await Task.sleep(for: .seconds(2))
                }
                guard current == epoch, request == vaultRequest, key == vaultQueryKey, !Task.isCancelled else { return }
                let page = fixtureVaultPage(offset: offset, limit: count)
                vault = more ? vault + page.items : page.items
                vaultCounts = page.counts; vaultHasMore = page.more
                vaultCountsQuery = vaultCountsKey
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
            let page: VaultBrowsePage = try await decodeInBackground(result)
            guard current == epoch, request == vaultRequest, key == vaultQueryKey, !Task.isCancelled else { return }
            vault = more ? vault + page.items.map(\.item) : page.items.map(\.item)
            vaultCounts = VaultCounts(total: page.total, all: page.all, favorites: page.favorites, kinds: page.kinds, tags: page.tags)
            vaultCountsQuery = vaultCountsKey
            vaultHasMore = page.hasMore; vaultLoaded = true; vaultQuery = key
        } catch {
            if current == epoch, request == vaultRequest, !Task.isCancelled { vaultFailed = true }
        }
    }

    func synchronize() async {
        activity(); syncClock = .now
        guard engine != nil, !busy, !syncCoolingDown else { return }
        let operation = beginBusyOperation(.synchronizing), current = epoch
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
        #if DEBUG
        if uiTesting { refreshFileCloudFixture(refresh: refresh); return }
        #endif
        // Reading a cache must not replace the identity of an in-flight refresh.
        if !refresh && loadingCloudBackups { return }
        let current = epoch
        do { try await coreOperation { try await fetchCloudBackups(refresh: refresh, operation: $0) } }
        catch { if current == epoch, !Task.isCancelled { cloudBackupsError = fileFailure(error) } }
    }

    private func fetchCloudBackups(refresh: Bool, operation: OperationToken) async throws {
        guard let engine, unlocked else { return }
        let current = epoch, request = UUID()
        cloudBackupRequest = request
        defer { if cloudBackupRequest == request { cloudBackupRequest = nil } }
        do {
            let result = try await engine.cloudBackups(refresh: refresh, operation: operation)
            guard !Task.isCancelled, current == epoch, cloudBackupRequest == request else { return }
            let backups: [CloudBackup] = try await decodeInBackground(result)
            guard !Task.isCancelled, current == epoch, cloudBackupRequest == request else { return }
            cloudBackups = backups
            struct Observation: Decodable { let complete: Bool; let observedAtMs: UInt64? }
            let observation: Observation = try decode(try await engine.cloudBackupStatus())
            guard !Task.isCancelled, current == epoch, cloudBackupRequest == request else { return }
            cloudBackupsKnown = observation.complete || !cloudBackups.isEmpty
            cloudBackupsComplete = observation.complete
            cloudBackupsObservedAt = observation.observedAtMs
            if refresh { cloudBackupsFailed = false; cloudBackupsError = nil }
        } catch {
            if !Task.isCancelled, current == epoch, cloudBackupRequest == request {
                // Inventory pages already committed before a later page failed
                // remain useful offline; keep the failure visible after rereading.
                if let saved = try? await engine.cloudBackups(refresh: false, operation: operation),
                   current == epoch, cloudBackupRequest == request {
                    cloudBackups = (try? decode(saved)) ?? cloudBackups
                }
                guard !Task.isCancelled, current == epoch, cloudBackupRequest == request else { return }
                cloudBackupsFailed = true; cloudBackupsError = fileFailure(error); throw error
            }
        }
    }

    private func clearCloudBackups() {
        cloudBackups = []; cloudBackupsFailed = false; cloudBackupsKnown = false; cloudBackupRequest = nil
        cloudBackupsObservedAt = nil; cloudBackupsComplete = false; cloudBackupsError = nil
    }

    func loadApprovals(refresh: Bool = false) async {
        guard let engine else { return }; let current = epoch
        do {
            if refresh { _ = try await coreOperation { try await engine.syncConnect(operation: $0) } }
            let result = try await engine.approvals()
            let values: [ApprovalView] = try decode(result)
            guard current == epoch else { return }; approvals = values; approvalsFailed = false
        } catch { if current == epoch { approvalsFailed = true; reportBackground(error) } }
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
            notice = String(localized: "An account recovery is pending. Review it in Account recovery.")
        }
    }

    /// For the large lists: their JSON is parsed away from the main thread.
    func decodeInBackground<T: Decodable & Sendable>(_ snapshot: ObjectSnapshot) async throws -> T {
        guard !Task.isCancelled, snapshot.generation == session?.generation else { throw CancellationError() }
        let json = snapshot.json
        let value = try await Task.detached(priority: .userInitiated) {
            let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
            return try decoder.decode(T.self, from: Data(json.utf8))
        }.value
        guard !Task.isCancelled, snapshot.generation == session?.generation else { throw CancellationError() }
        return value
    }

    func decode<T: Decodable>(_ snapshot: ObjectSnapshot) throws -> T {
        guard !Task.isCancelled, snapshot.generation == session?.generation else { throw CancellationError() }
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(T.self, from: Data(snapshot.json.utf8))
    }

    /// Work the person did not ask for keeps its failure in the view it belongs
    /// to. Only losing this device's approval still has to reach them.
    func reportBackground(_ error: Error) {
        if let failure = error as? MobileError, case .Failure(let code) = failure, code == "DEVICE_NOT_APPROVED" { report(error) }
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
                    epoch &+= 1; viewGeneration &+= 1; drafts = [:]; dirtyDrafts = []; vault = []; blockedAccounts = []; vaultCounts = VaultCounts(); vaultLoaded = false; vaultFailed = false; vaultFilter = "all"; vaultTag = ""; vaultRequest = nil; loadingVault = false; activeChannel = nil; channels = []; library = .empty; filesListings = [:]; filesSearch = ""; approvals = []; session = nil
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

/// Visibility is independent of login validity and the content lock. Resuming
/// keeps the privacy cover until the core has adopted the current session.
enum SessionLifecycle { case inactive, background, resuming, active, protectedDataUnavailable }

enum SessionOperation { case other, unlocking, initializingAccount, synchronizing }

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

/// See `SessionModel.route`.
struct WorkspaceRoute: Equatable {
    var vaultItem: String?
    var messagesMode = 0
    var message: String?
    var filesPath: [FilesRoute] = []
    var file: String?
}

enum FilesRoute: Hashable {
    case folder(String), file(String), transfers
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
