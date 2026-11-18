package net.dmsg.android.core

import android.app.Application
import android.os.SystemClock
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import net.dmsg.android.BuildConfig
import net.dmsg.android.platform.*
import net.dmsg.core.*
import org.json.JSONObject
import java.io.File

/** One process owns one core. UI snapshots are discarded after every lock/account transition. */
class ClientController internal constructor(private val application: Application,
    uptime: () -> Long = SystemClock::elapsedRealtime,
    private val openEngine: suspend (PrivateFiles, AndroidSecurity) -> MobileEngine = { paths, platform -> Deployment.open(paths.workspace.path, paths.staging.path, platform) },
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val mutable = MutableStateFlow(UiState())
    val state = mutable.asStateFlow()
    private val mutableAccess = MutableStateFlow(AccessPage.WELCOME)
    val access = mutableAccess.asStateFlow()
    fun showAccess(page: AccessPage) { activity(); mutableAccess.value = page }
    val prompts = PresencePrompts()
    val passkeys = NativePasskeys(application)
    val authentication = AuthenticationCoordinator(passkeys)
    val vault = VaultCoordinator(this)
    val vaultHistory = VaultHistoryCoordinator(this)
    val secretTools = SecretToolsCoordinator(this)
    val secretSharing = SecretSharingCoordinator(this)
    val messages = MessageCoordinator(this, scope)
    val channels = ChannelCoordinator(this)
    val participants = ParticipantCoordinator(this, scope)
    private var incomingChannel: String? = null
    private var incomingChannelPresented: String? = null
    val messageAttachments = MessageAttachmentsCoordinator(this)
    val library = FileCoordinator(this)
    val cloudFiles = CloudFilesCoordinator(this)
    val fileLinks = FileLinksCoordinator(this)
    val fileRequests = FileRequestsCoordinator(this)
    val fileWork = FileWorkCoordinator(this, scope)
    val backgroundFiles = BackgroundFileCoordinator(this, application)
    val systemExports = SystemExportsCoordinator(this, application, scope) { files }
    val incoming = IncomingShareCoordinator(this, application, scope) { files }
    private var pendingWorkspace: WorkspaceLink? = null
    private var pendingNotice: NoticeRoute? = null
    internal var navigationIntent = 0L
        private set
    internal fun beginNavigationIntent(): Long {
        navigationIntent += 1
        incomingConnect = null; incomingConnectPresented = null
        incomingChannel = null; incomingChannelPresented = null; pendingWorkspace = null; pendingNotice = null
        noticeNavigation.clear(discard = true)
        preferences()
        return navigationIntent
    }
    fun requestNotice(route: NoticeRoute) { beginNavigationIntent(); automaticUnlock.suppress(); pendingNotice = route; preferences(); routePendingNotice() }
    fun routePendingNotice() { if (!state.value.starting && !state.value.concealed) pendingNotice?.let(noticeNavigation::resolve) ?: noticeNavigation.continueIfReady() }
    internal fun finishNoticeRoute() { pendingNotice = null; preferences() }
    internal fun persistNavigation() = preferences()
    fun requestWorkspace(link: WorkspaceLink) { beginNavigationIntent(); pendingWorkspace = link; preferences(); routePendingWorkspace() }
    fun routePendingWorkspace() {
        val target = pendingWorkspace ?: return
        if (!state.value.unlocked || state.value.concealed) return
        pendingWorkspace = null
        navigation.open(target.page); preferences()
    }
    val approvals = ApprovalCoordinator(this)
    val account = AccountCoordinator(this)
    val accountWorkspace = AccountWorkspaceCoordinator(this)
    val browserConnections = BrowserConnectionsCoordinator(this)
    private var incomingConnect: ConnectLink? = null
    private var incomingConnectPresented: String? = null
    val accountDeletion = AccountDeletionCoordinator(this)
    val deviceTransfers = DeviceTransferCoordinator(this, scope, DeviceDiscovery(application)) { engine?.transferCancel() }
    val handovers = HandoverCoordinator(this)
    val agents = AgentCoordinator(this)
    val wallets = WalletCoordinator(this)
    val membership = MembershipCoordinator(this)
    val notifications = NotificationCoordinator(this)
    val push = PushCoordinator(this, application, scope)
    val purchases = PlayPurchaseCoordinator(this, application, scope)
    val noticeNavigation = NoticeNavigation(this)
    val notices = NoticePresenter(this, application, scope)
    val profile = ProfileCoordinator(this) { files }
    private val accountNamesValue = MutableStateFlow<Map<String, String>>(emptyMap())
    val accountNames = accountNamesValue.asStateFlow()
    val letters = LetterCoordinator(this)
    val safety = SafetyCoordinator(this)
    val feedback = FeedbackCoordinator(this)
    val transfers = FileTransferCoordinator(this) { files }
    val filePreview = FilePreviewCoordinator(this) { files }
    val vaultImport = VaultImportCoordinator(this, scope) { files }
    val qr = QrCoordinator(this)
    val vaultDrafts = VaultDrafts(this)
    val navigation = WorkspaceNavigation()
    val synchronization = SyncCoordinator(this, scope)
    val clipboard = PrivateClipboard(application)
    private val automaticUnlock = AutomaticUnlockPolicy()
    private val policy = SessionPolicy(uptime)
    private val sessions = NativeSession()
    private val engine: MobileEngine? get() = sessions.root
    private var files: PrivateFiles? = null
    private var security: AndroidSecurity? = null
    private val epoch: Long get() = sessions.fence.epoch
    private val operations = OperationOwners()
    private var operationSequence = 0L
    private var externalLoginReturn = false
    private var externalUntil = 0L
    private var appForeground = true
    private var starting = false
    private var pendingLogin = false
    private var bindingLogin = false
    private data class BindingReturn(val callback: String, val generation: ULong, val deadline: Long)
    private var bindingReturn: BindingReturn? = null
    private var backgroundLock: Job? = null
    private var afterLock: (() -> Unit)? = null
    private var controlBusy = false
    private var backgroundFinish: Job? = null
    private val syncPreferences = mutableMapOf<String, Boolean>()
    private val preferencesTurn = Mutex()

    init {
        scope.launch {
            while (isActive) {
                delay(15_000)
                checkInactivity()
            }
        }
    }

    suspend fun shutdown() {
        deviceTransfers.clear(); backgroundFiles.clear(); incoming.stop(); systemExports.clear(); notices.stop(); push.stop(); purchases.stop()
        operations.cancelAll(); scope.cancel(); prompts.cancel(); authentication.cancel()
        engine?.lockNow()
        engine?.shutdown()
    }

    fun tryAutomaticUnlock() {
        if (engine == null || !appForeground || state.value.concealed || mutableAccess.value != AccessPage.WELCOME || !prompts.canPrompt || prompts.isAuthenticating || passkeys.isAuthenticating) return
        if (application.getSystemService(android.app.KeyguardManager::class.java).isDeviceLocked) return
        if (automaticUnlock.consume(state.value.session?.state == "locked", state.value.session?.fastUnlock == true, state.value.busy)) unlock()
    }

    fun activity() { policy.activity() }
    internal fun checkInactivity() { if (state.value.unlocked && !state.value.concealed && policy.shouldLock()) lock(false) }
    fun beginExternalInteraction() { activity(); externalUntil = SystemClock.elapsedRealtime() + 180_000 }
    internal fun currentStamp(): SessionStamp? = sessions.fence.stamp()
    internal fun scopedHandle(): MobileEngine? = runCatching { sessions.handle() }.getOrNull()
    internal fun sessionCurrent(stamp: SessionStamp) = sessions.fence.current(stamp)
    internal fun accepts(stamp: SessionStamp): Boolean = sessions.fence.current(stamp) && appForeground && state.value.unlocked && !state.value.concealed && !policy.shouldLock()
    internal fun contentChanged(changed: Boolean) {
        perform({ }, key = "session:read", replace = true) {
            if (changed) {
                vault.browse()
                vault.selected?.let(vault::select)
                if (navigation.state.value.workspace == Workspace.FILES) refresh("files")
                messages.reload()
            }
        }
    }
    internal fun receivedHint(kind: String, resource: String?) {
        when (kind) {
            "channel", "message" -> messages.hint(resource)
            "approval", "connect" -> refresh("approvals")
            "inbox", "letter" -> refresh("inbox")
            "file-request" -> fileRequests.browse()
        }
    }
    fun cancelOperation(key: String) { operations.cancel(key); updateOperations() }
    fun cancelForHandoff(vararg keys: String): List<Job> = keys.mapNotNull { operations.cancel(it) }.also { updateOperations() }
    internal fun isWorking(key: String) = operations.running(key)
    fun notice(code: String) { mutable.update { it.copy(notice = code) } }
    fun clearNotice() { mutable.update { it.copy(error = null, notice = null) } }
    fun showError(code: String) { mutable.update { it.copy(error = code) } }

    fun start() {
        if (starting) return
        if (engine != null) { foreground(); return }
        starting = true
        scope.launch {
            try {
                val opened = withContext(Dispatchers.IO) {
                    val privateFiles = PrivateFiles(application)
                    check(NativeBootstrap.initialize(application)) { "TLS_INITIALIZATION" }
                    val platform = AndroidSecurity(application, privateFiles, prompts)
                    check(buildIdentity() == BuildIdentity.HASH) { "NATIVE_BUILD_MISMATCH" }
                    Triple(privateFiles, platform, openEngine(privateFiles, platform))
                }
                files = opened.first; security = opened.second; sessions.install(opened.third)
                val preferences = File(opened.first.root, "preferences.json")
                val saved = withContext(Dispatchers.IO) { if (preferences.isFile) runCatching { JSONObject(preferences.readText()) }.getOrNull() else null }
                policy.immediateBackgroundLock = saved?.optBoolean("immediate_lock") ?: false
                saved?.optJSONObject("automatic_sync")?.let { values -> values.keys().forEach { key ->
                    if (key.matches(Regex("[0-9a-v]{20}"))) (values.opt(key) as? Boolean)?.let { syncPreferences[key] = it }
                } }
                saved?.optJSONObject("account_names")?.let { names ->
                    accountNamesValue.value = names.keys().asSequence().filter(::validAccount).take(100).mapNotNull { id ->
                        (names.opt(id) as? String)?.takeIf { it.codePointCount(0, it.length) <= 80 && it.isNotBlank() }?.let { id to it }
                    }.toMap()
                }
                if (navigationIntent == 0L) {
                    incomingConnect = (saved?.opt("pending_connect") as? String)?.let(ConnectLink::parse)
                    pendingWorkspace = (saved?.opt("pending_workspace") as? String)?.let(WorkspaceLink::parse)
                    pendingNotice = (saved?.opt("pending_notice") as? String)?.let(NoticeRoute::parse)
                    noticeNavigation.restore(saved?.opt("pending_reminder") as? String)
                    if (incomingConnect != null || pendingWorkspace != null || pendingNotice != null || noticeNavigation.save() != null) navigationIntent = 1L
                }
                val restored = withContext(Dispatchers.IO) { readSession(opened.third) }
                val session = restored.session
                if (!sessions.adopt(session)) return@launch
                restoreEditors(restored.pending)
                navigation.activate(session.account)
                mutable.value = UiState(starting = false, concealed = !appForeground, session = session, epoch = epoch,
                    dark = when (saved?.optString("appearance")) { "dark" -> true; "light" -> false; else -> null },
                    immediateLock = policy.immediateBackgroundLock, error = restored.draftError)
                if (navigationIntent != 0L) this@ClientController.preferences()
                incoming.checkPending()
                if (appForeground) startForegroundWork()
                tryAutomaticUnlock()
            } catch (error: Throwable) {
                mutable.update { it.copy(starting = false, concealed = !appForeground, error = errorCode(error)) }
            } finally { starting = false }
        }
    }

    private fun clearPrivate(concealed: Boolean = true) {
        sessions.fence.invalidate()
        mutableAccess.value = AccessPage.WELCOME
        incoming.clear(); systemExports.clear(); noticeNavigation.clear(); notices.stop(); push.stop(); purchases.stop()
        vault.clear()
        vaultHistory.clear()
        secretTools.clear()
        secretSharing.clear()
        vaultImport.clear()
        qr.clear()
        messages.clear(sessions.snapshot?.account)
        channels.clear(); participants.clear()
        library.clear(); cloudFiles.clear(); fileLinks.clear(); fileRequests.clear(); filePreview.clear(); messageAttachments.clear(); fileWork.stop(clear = true); backgroundFiles.clear()
        approvals.clear()
        account.clear(); accountWorkspace.clear(); browserConnections.clear(); accountDeletion.clear(); wallets.clear(); agents.clear(); deviceTransfers.clear(); handovers.clear(); membership.clear(); purchases.clear(); notifications.clear(); profile.clear()
        letters.clear(); safety.clear(); feedback.clear(); transfers.clear()
        vaultDrafts.clear()
        synchronization.stop(clear = true)
        mutable.update { UiState(starting = false, concealed = concealed, epoch = epoch, dark = it.dark, immediateLock = it.immediateLock) }
    }

    fun lock(suppressAutomaticUnlock: Boolean = true, after: (() -> Unit)? = null) {
        val root = engine ?: return
        if (suppressAutomaticUnlock) automaticUnlock.suppress()
        clipboard.clearOwned()
        if (after != null) afterLock = after
        if (controlBusy) return
        controlBusy = true
        val cleanupFiles = files?.fenceIO()
        val session = sessions.snapshot
        val account = session?.account
        val pending = account?.let(messages::pending).orEmpty()
        val letterDrafts = letters.pending()
        val feedbackDraft = feedback.pending()
        val vaultInputs = vaultDrafts.pending()
        val needsFlush = pending.isNotEmpty() || letterDrafts.isNotEmpty() || feedbackDraft != null || vaultInputs.isNotEmpty()
        val device = session?.deviceId
        val checkpoint = if (needsFlush && account != null && device != null) {
            root.beginDraftLock(account, device, pending.map { it.native() }, letterDrafts, feedbackDraft, vaultInputs)
        } else { root.lockNow(); null }
        prompts.cancel()
        authentication.cancel()
        bindingReturn = null
        operations.cancelAll()
        messages.suspend()
        clearPrivate(false)
        pendingLogin = false; bindingLogin = false
        mutable.update { it.copy(busy = true) }
        scope.launch {
            val saved = try {
                withContext(Dispatchers.IO) { checkpoint?.finish() }
                !needsFlush || checkpoint != null
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Exception) { false }
            finally { checkpoint?.destroy() }
            try {
                if (appForeground) {
                    val generation = root.controlGeneration()
                    val result = withContext(Dispatchers.IO) { root.resume(generation) }
                    if (appForeground && root.controlGeneration() == generation) {
                        adopt(result)
                        mutable.update { it.copy(concealed = false) }
                        startForegroundWork()
                    }
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) { if (appForeground) showError(errorCode(error)) }
            finally { controlBusy = false; updateOperations() }
            if (saved) continueAfterLock()
            else {
                afterLock = null
                showError("DRAFT_SAVE_FAILED")
            }
        }
        scope.launch(Dispatchers.IO) { cleanupFiles?.invoke() }
    }

    private fun continueAfterLock() {
        if (controlBusy || !appForeground || state.value.concealed || state.value.session == null) return
        val continuation = afterLock ?: return
        afterLock = null
        continuation()
    }

    fun background() {
        vaultDrafts.suspend()
        deviceTransfers.background(); incoming.clear(); systemExports.clear(); noticeNavigation.clear(); notices.stop(); push.stop(); purchases.stop()
        appForeground = false
        participants.clear(); browserConnections.clear(); accountDeletion.clear(retainDraft = true); wallets.clear(retainForms = true); agents.clear(retainForms = true); handovers.clear(retainForms = true); membership.clear(); purchases.clear(); notifications.clear(retainDraft = true); profile.clear(retainForm = true); synchronization.stop(); fileWork.stop(); backgroundFiles.clear(); cloudFiles.clear(retainBatch = true); fileLinks.clear(retainForm = true); fileRequests.clear(); filePreview.clear(); messageAttachments.clear()
        externalLoginReturn = pendingLogin || prompts.isAuthenticating || passkeys.isAuthenticating || externalUntil > SystemClock.elapsedRealtime()
        if (!externalLoginReturn) {
            automaticUnlock.rearm()
            policy.background()
            if (policy.immediateBackgroundLock || controlBusy) {
                backgroundLock?.cancel()
                lock(false)
                mutable.update { it.copy(concealed = true, epoch = epoch) }
                return
            }
            sessions.fence.invalidate()
            val owner = sessions.snapshot?.account
            val pending = owner?.let(messages::pending).orEmpty()
            val letterDrafts = letters.pending()
            val feedbackDraft = feedback.pending()
            val vaultInputs = vaultDrafts.pending()
            operations.cancelAll()
            prompts.cancel()
            val handle = runCatching { sessions.handle() }.getOrNull()
            messages.suspend(); transfers.suspend()
            val backgroundEpoch = epoch
            backgroundFinish = scope.launch {
                try {
                    if (handle != null) {
                        val savedFeedback = withContext(Dispatchers.IO) {
                            withTimeout(2000) {
                                for (draft in vaultInputs) handle.checkpointVaultDraft(draft)
                                for (draft in pending) { handle.saveAuthoredDraft(draft.native()); messages.saved(draft) }
                                for (draft in letterDrafts) handle.saveLetterDraft(draft.id, draft.recipient, draft.text, draft.invitation)
                                feedbackDraft?.let { value ->
                                    val current = parseFeedback(handle.feedback().decode())
                                    if (current == null || current.id == value.id && current.state == "draft") {
                                        handle.saveFeedback(value.copy(revision = current?.revision ?: 0u))
                                        value
                                    } else null
                                }
                            }
                        }
                        if (epoch == backgroundEpoch) {
                            letters.acknowledge(letterDrafts)
                            feedback.acknowledge(savedFeedback)
                        }
                    }
                } catch (_: Throwable) { /* Explicit lock/foreground retry retains authored drafts. */ }
                finally {
                    handle?.destroy()
                    if (!appForeground && epoch == backgroundEpoch) engine?.`suspend`()
                }
            }
        }
        mutable.update { it.copy(concealed = true, epoch = epoch) }
        backgroundLock?.cancel()
        backgroundLock = scope.launch {
            delay(180_000)
            lock(false)
        }
    }

    fun foreground() {
        appForeground = true
        externalUntil = 0
        backgroundLock?.cancel()
        if (policy.foreground()) { lock(false); return }
        val current = engine
        if (current == null) { start(); return }
        val at = epoch
        scope.launch {
            try {
                backgroundFinish?.join()
                val expected = current.controlGeneration()
                val session = withContext(Dispatchers.IO) {
                    readSession(current, if (externalLoginReturn) null else current.resume(expected))
                }
                if (at == epoch && appForeground) {
                    adopt(session)
                    if (!prompts.isAuthenticating && !passkeys.isAuthenticating) {
                        externalLoginReturn = false
                        mutable.update { it.copy(concealed = false) }
                        startForegroundWork()
                        deviceTransfers.foreground()
                        continueAfterLock()
                        tryAutomaticUnlock()
                    }
                }
            } catch (error: Throwable) { if (at == epoch && appForeground) mutable.update { it.copy(concealed = false, error = errorCode(error)) } }
        }
    }

    private data class EditorSession(val session: SessionSnapshot, val pending: EditorDrafts? = null, val draftError: String? = null)

    private suspend fun readSession(root: MobileEngine, known: SessionSnapshot? = null): EditorSession {
        val next = known ?: root.sessionState()
        val previous = state.value.session
        if (next.state != "unlocked" || previous?.state == "unlocked" && previous.generation == next.generation) {
            return EditorSession(next)
        }
        val handle = root.scoped(next.generation)
        return try { EditorSession(next, handle.restoreEditorDrafts()) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { EditorSession(next, draftError = errorCode(error)) }
        finally { handle.destroy() }
    }

    private fun restoreEditors(pending: EditorDrafts?) {
        if (pending == null) return
        pending.letters.forEach(letters::restore)
        pending.feedback?.let(feedback::restore)
        pending.vault.forEach(vaultDrafts::restore)
        pending.drafts.forEach { messages.restore(requireNotNull(sessions.snapshot?.account), it) }
    }

    private fun adopt(restored: EditorSession) = adopt(restored.session, restored.pending, restored.draftError)

    private fun adopt(next: SessionSnapshot, pending: EditorDrafts? = null, draftError: String? = null) {
        val before = state.value.session
        if (!sessions.adopt(next)) return
        navigation.activate(next.account)
        if (next.state == "unlocked") mutableAccess.value = AccessPage.WELCOME
        val changed = before != null && (before.account != next.account || before.generation != next.generation)
        if (changed) {
            bindingReturn = null
            systemExports.clear(); noticeNavigation.clear(); notices.stop(); push.stop(); purchases.stop()
            val retain = before?.account == next.account && next.state == "unlocked"
            vault.clear(retainQuery = retain)
            vaultHistory.clear()
            secretTools.clear()
            secretSharing.clear(retainForms = retain)
            vaultImport.clear(retain)
            qr.clear()
            messages.clear(before?.account, retainDrafts = retain)
            channels.clear(retainForms = retain); participants.clear()
            library.clear(retainForms = retain); cloudFiles.clear(retainBatch = retain); fileLinks.clear(retain); fileRequests.clear(); filePreview.clear(); messageAttachments.clear(); fileWork.stop(clear = !retain); backgroundFiles.clear(); approvals.clear(retainReceipts = retain); account.clear(retainForms = retain); accountWorkspace.clear(); browserConnections.clear(); accountDeletion.clear(retainDraft = retain); wallets.clear(retainForms = retain); agents.clear(retainForms = retain); incoming.clear(); deviceTransfers.clear(retainPlan = retain); handovers.clear(retainForms = retain); membership.clear(); purchases.clear(); notifications.clear(retainDraft = retain); profile.clear(retainForm = retain)
            letters.clear(retain); safety.clear(retain); feedback.clear(retain)
            if (retain) transfers.suspend() else transfers.clear()
        }
        if (before?.account != next.account || next.state != "unlocked") vaultDrafts.clear()
        restoreEditors(pending)
        mutable.update {
            if (changed) UiState(starting = false, concealed = it.concealed, session = next, epoch = epoch,
                dark = it.dark, immediateLock = it.immediateLock, error = draftError)
            else it.copy(starting = false, session = next, epoch = epoch, error = draftError ?: it.error)
        }
        if (appForeground && !state.value.concealed) startForegroundWork()
        if (next.state != "unlocked") { synchronization.stop(clear = true); fileWork.stop(clear = true); backgroundFiles.clear() }
    }

    private fun startForegroundWork() {
        notices.start(); push.start(); purchases.start()
        if (!state.value.unlocked) return
        synchronization.setAutomatic(syncPreferences[state.value.session?.account] ?: true)
        synchronization.start(); fileWork.start(); systemExports.start()
        incoming.checkPending()
        routePendingWorkspace()
    }

    private fun updateOperations() {
        val keys = operations.keys()
        mutable.update { it.copy(operations = keys, busy = controlBusy || "session" in keys) }
    }

    /** Activity results can arrive before onResume. Wait for the same session's
     * visible foreground instead of dropping an early result or adopting it in
     * whichever account happens to be active when a picker returns. */
    fun deliverExternalResult(key: String, stamp: SessionStamp, discard: () -> Unit = {}, apply: () -> Unit) {
        var delivered = false
        lateinit var job: Job
        val resource = "external-result:" + key
        job = scope.launch(start = CoroutineStart.LAZY) {
            try {
                withTimeout(180_000) { state.first { !it.starting && !it.concealed } }
                if (operations.owns(resource, job) && appForeground && sessionCurrent(stamp)) { delivered = true; apply() }
            } finally { operations.release(resource, job); updateOperations() }
        }
        job.invokeOnCompletion { if (!delivered) discard() }
        if (operations.claim(resource, job, replace = true)) { updateOperations(); job.start() } else job.cancel()
    }

    fun <T> perform(work: suspend (MobileEngine) -> T, transition: Boolean = false,
        key: String = if (transition) "session" else "operation:" + ++operationSequence,
        replace: Boolean = false, serial: Mutex? = null, quiet: Boolean = false, userInitiated: Boolean = transition, onError: (String) -> Unit = {}, apply: (T) -> Unit = {}) {
        if (!appForeground && !transition) return
        val root = engine ?: return
        if (operations.running(key) && !replace) return
        val handle = try { sessions.handle() } catch (error: Throwable) { showError(errorCode(error)); return }
        if (userInitiated) activity()
        val generation = sessions.snapshot?.generation
        if (transition) {
            operations.cancelAll()
            sessions.fence.invalidate()
        }
        val at = epoch
        lateinit var job: Job
        fun canDeliver() = operations.owns(key, job) && at == epoch && appForeground && (!state.value.concealed || transition)
        fun report(error: Throwable) {
            val code = errorCode(error)
            onError(code)
            if (!quiet) showError(code)
        }
        job = scope.launch(start = CoroutineStart.LAZY) {
            updateOperations()
            if (!quiet) mutable.update { it.copy(error = null) }
            try {
                // Claim local ordering on Main before dispatching native work. A later
                // keystroke cannot overtake the queue operation started by Send.
                val result = runCatching {
                    if (serial == null) withContext(Dispatchers.IO) { work(handle) }
                    else serial.withLock { withContext(Dispatchers.IO) { work(handle) } }
                }
                ensureActive()
                if (!canDeliver()) return@launch
                val session = runCatching { withContext(Dispatchers.IO) { readSession(root) } }
                ensureActive()
                if (!canDeliver()) return@launch
                val snapshot = session.getOrNull()
                val currentGeneration = root.controlGeneration()
                if (snapshot != null && snapshot.session.generation != currentGeneration) return@launch
                snapshot?.let(::adopt)
                if (!transition && generation != currentGeneration) return@launch

                // A completion may start the next step with the same operation key.
                operations.release(key, job)
                val failure = result.exceptionOrNull() ?: session.exceptionOrNull()
                if (failure != null) report(failure)
                else {
                    if (transition) mutable.update { it.copy(concealed = false) }
                    try { apply(result.getOrThrow()) }
                    catch (error: Exception) { report(error) }
                }
            } finally {
                handle.destroy()
                operations.release(key, job)
                updateOperations()
            }
        }
        if (!operations.claim(key, job, replace)) { handle.destroy(); job.cancel(); return }
        job.start()
    }

    @OptIn(InternalCoroutinesApi::class)
    suspend fun <T> operation(work: suspend (OperationToken) -> T): T {
        val token = OperationToken()
        val cancellation = currentCoroutineContext()[Job]?.invokeOnCompletion(onCancelling = true, invokeImmediately = true) {
            if (it != null) token.cancel()
        }
        try { return work(token) }
        finally { cancellation?.dispose(); token.cancel(); token.destroy() }
    }

    fun passkey(operation: PasskeyOperation = PasskeyOperation.LOGIN, credential: ByteArray? = null, resume: Boolean = false) {
        if (authentication.state.value.working) return
        automaticUnlock.suppress()
        perform({ core -> authentication.authenticate(core, operation, credential, resume) }, transition = true) { completed ->
            mutable.update { it.copy(concealed = false) }
            if (completed.operation == PasskeyOperation.RESET_SESSIONS) {
                authentication.capacityRecovery()?.let { (id, purpose) -> passkey(purpose, id) }
            } else if (completed.approval != null) approvals.open(completed.approval) else refresh("vault")
        }
    }

    fun continuePasskey() { authentication.retryOperation()?.let { passkey(it, resume = true) } }
    fun resetPasskeySessions() { authentication.capacityRecovery()?.let { (id, _) -> passkey(PasskeyOperation.RESET_SESSIONS, id) } }
    fun cancelAuthentication() {
        authentication.cancel(); engine?.cancelLogin(); pendingLogin = false; bindingLogin = false; bindingReturn = null
        mutable.update { it.copy(bindingRetry = false) }
    }

    fun login(binding: Boolean = false, open: (String) -> Unit) {
        automaticUnlock.suppress()
        cancelAuthentication()
        perform({ if (binding) it.beginBinding() else it.beginLogin() }, key = "session") {
            pendingLogin = true; bindingLogin = binding; open(it.url)
        }
    }

    fun acceptCallback(raw: String) {
        ConnectLink.parse(raw)?.let { link ->
            beginNavigationIntent(); incomingConnect = link; incomingConnectPresented = null; preferences(); routePendingConnect(); return
        }
        WorkspaceLink.parse(raw)?.let { requestWorkspace(it); return }
        if (raw.startsWith("dmsg://channel/")) {
            if (ChannelInvitation.parse(raw) == null) { showError("INVALID_INPUT"); return }
            beginNavigationIntent(); incomingChannel = raw; incomingChannelPresented = null; routePendingChannel(); return
        }
        if (!LoginCallback.valid(raw)) { showError("INVALID_LINK"); return }
        if (!pendingLogin) { showError("LOGIN_EXPIRED"); return }
        pendingLogin = false
        val binding = bindingLogin; bindingLogin = false
        mutable.update { it.copy(concealed = false) }
        if (binding) {
            val generation = sessions.snapshot?.generation ?: return
            bindingReturn = BindingReturn(raw, generation, SystemClock.elapsedRealtime() + 180_000)
            continueBrowserBinding()
        } else perform({ it.completeLogin(raw) }, transition = true) { refresh("vault") }
    }

    fun routePendingConnect() {
        val link = incomingConnect ?: return
        if (!state.value.unlocked || state.value.concealed) return
        val account = state.value.session?.account ?: return
        if (incomingConnectPresented == account) return
        incomingConnectPresented = account
        navigation.push(link.route())
    }
    fun dismissConnectLink() { incomingConnect = null; incomingConnectPresented = null; preferences() }

    fun routePendingChannel() {
        val link = incomingChannel ?: return
        if (!state.value.unlocked || state.value.concealed) return
        val account = state.value.session?.account ?: return
        if (incomingChannelPresented == account) return
        incomingChannelPresented = account
        navigation.open("messages")
        channels.openSetup(true, link)
    }
    fun consumeChannelLink(link: String) { if (incomingChannel == link) incomingChannel = null }

    fun continueBrowserBinding() {
        val returned = bindingReturn ?: return
        if (sessions.snapshot?.generation != returned.generation || SystemClock.elapsedRealtime() >= returned.deadline) {
            cancelAuthentication(); showError("LOGIN_EXPIRED"); return
        }
        mutable.update { it.copy(bindingRetry = false) }
        perform({ it.completeBinding(returned.callback).decode().objectValue().required("id") }, key = "session", onError = { code ->
            if (code in setOf("UNAVAILABLE", "EXECUTION_UNKNOWN") && bindingReturn == returned) mutable.update { it.copy(bindingRetry = true) }
            else bindingReturn = null
        }) { request -> bindingReturn = null; approvals.open(request) }
    }

    fun unlock() { automaticUnlock.suppress(); perform({ it.unlockFast() }, transition = true) { refresh("vault") } }
    fun initialize() { automaticUnlock.suppress(); perform({ it.initializeAccount() }, transition = true) { refresh("vault") } }
    fun refreshAccount() = perform({ it.refreshAccount() }, transition = true) { refresh("vault") }
    fun logout() { val account = state.value.session?.account; account?.let(push::forget); lock { perform({ systemExports.invalidate(); account?.let { owner -> backgroundFiles.forget(owner); incoming.forget(owner) }; it.logout() }, transition = true) } }
    fun switchAccount(account: String) { lock { perform({ systemExports.invalidate(); it.switchAccount(account) }, transition = true) } }
    fun removeLocalAccount(account: String) {
        if (!state.value.unlocked || state.value.session?.account != account) return
        push.forget(account)
        // Removal verifies the unlocked account itself. Conceal/cancel the UI now,
        // but let that native transition retire its keys and workspace together.
        automaticUnlock.suppress()
        operations.cancelAll(); prompts.cancel(); authentication.cancel(); clipboard.clearOwned()
        val cleanupFiles = files?.fenceIO()
        clearPrivate(false)
        perform({ systemExports.invalidate(); backgroundFiles.forget(account); incoming.forget(account); it.removeLocalAccount(account) }, transition = true) {
            accountNamesValue.value -= account
            syncPreferences.remove(account); navigation.forget(account); preferences()
        }
        scope.launch(Dispatchers.IO) { cleanupFiles?.invoke() }
    }
    internal fun rememberAccountName(account: String, name: String?) = rememberAccountNames(mapOf(account to name))
    internal fun rememberAccountNames(values: Map<String, String?>) {
        if (!state.value.unlocked) return
        var next = accountNamesValue.value
        values.filterKeys(::validAccount).forEach { (account, name) ->
            val value = name?.trim()?.takeIf { it.isNotEmpty() && it.codePointCount(0, it.length) <= 80 }
            next = if (value == null) next - account else next + (account to value)
        }
        if (next == accountNamesValue.value) return
        accountNamesValue.value = next.entries.toList().takeLast(100).associate { it.toPair() }
        preferences()
    }
    fun fastUnlock(enabled: Boolean) = perform({ it.setFastUnlock(enabled) }, transition = true)

    fun refresh(tab: String) {
        if (!state.value.unlocked) return
        when (tab) {
            "vault" -> vault.browse()
            "messages" -> messages.browse()
            "inbox" -> letters.refresh()
            "files" -> library.browse(navigation.state.value.current.folder)
            "approvals" -> approvals.refresh()
        }
    }

    fun sync() = synchronization.manual()

    fun dark(value: Boolean) { mutable.update { it.copy(dark = value) }; preferences() }
    fun systemAppearance() { mutable.update { it.copy(dark = null) }; preferences() }
    fun automaticSync(value: Boolean) {
        val account = state.value.session?.account ?: return
        syncPreferences[account] = value
        synchronization.setAutomatic(value)
        preferences()
    }
    fun immediateLock(value: Boolean) { policy.immediateBackgroundLock = value; mutable.update { it.copy(immediateLock = value) }; preferences() }
    private fun preferences() {
        if (sessions.snapshot == null) return
        val root = files?.root ?: return
        scope.launch { preferencesTurn.withLock {
            val bytes = JSONObject().put("appearance", when (state.value.dark) { true -> "dark"; false -> "light"; null -> "system" })
                .put("immediate_lock", state.value.immediateLock).put("automatic_sync", JSONObject(syncPreferences.toMap()))
                .put("account_names", JSONObject(accountNamesValue.value)).put("pending_connect", incomingConnect?.url()).put("pending_workspace", pendingWorkspace?.url()).put("pending_notice", pendingNotice?.value).put("pending_reminder", noticeNavigation.save()).toString().toByteArray()
            withContext(Dispatchers.IO) { PrivateFiles.atomicWrite(File(root, "preferences.json"), bytes) }
        } }
    }

    internal fun errorCode(error: Throwable): String = when (error) {
        is TimeoutCancellationException -> "TIMEOUT"
        is CancellationException -> "CANCELLED"
        is MobileException.Failure -> error.code
        is UnsatisfiedLinkError -> "NATIVE_LIBRARY_UNAVAILABLE"
        is IllegalStateException, is IllegalArgumentException -> error.message?.takeIf { it in setOf("GOOGLE_UNAVAILABLE", "PLAY_PLAN_CHANGED", "PLAY_PENDING", "PLAY_RECOVERED", "PLAY_CURRENT_PLAN", "PLAY_RESTORE_REQUIRED", "PLAY_SERVICE_UNAVAILABLE", "PLAY_ALREADY_OWNED", "PLAY_PRODUCT_UNAVAILABLE", "PLAY_UNSUPPORTED_PURCHASE", "PAYMENT_STORAGE_UNAVAILABLE", "EXTERNAL_PLAN", "SUBSCRIPTION_REQUIRED", "SUBSCRIPTION_SUSPENDED", "ACCOUNT_MISMATCH", "EXPORT_NOT_CONFIGURED", "EXPORT_EXPIRED", "EXPORT_LOCKED", "EXPORT_KEY_UNAVAILABLE", "AUTOFILL_UNVERIFIED", "DEVICE_CREDENTIAL_NOT_SET", "HARDWARE_UNAVAILABLE", "INTEGRITY_FAILED", "LOCKED", "EXPIRED", "FORBIDDEN", "PASSKEY_ORIGIN_MISMATCH", "FILE_LIMIT", "FILE_UNAVAILABLE", "CANCELLED", "STORAGE_UNAVAILABLE", "INVALID_INPUT", "INVALID_AGENT_LABEL", "WRONG_ACCOUNT", "SOURCE_CHANGED", "DEVICE_LOCKED", "INCOMING_KEY_UNAVAILABLE", "PENDING", "TRANSFER_PENDING", "TRANSFER_EXPIRED", "SYSTEM_TRANSFER_UNAVAILABLE", "MISSING_CIPHERTEXT", "DISCOVERY_UNAVAILABLE", "DISCOVERY_NAME_CONFLICT", "BACKUP_NOT_STARTED", "DISK_FULL") } ?: "OPERATION_FAILED"
        else -> "OPERATION_FAILED"
    }
}
