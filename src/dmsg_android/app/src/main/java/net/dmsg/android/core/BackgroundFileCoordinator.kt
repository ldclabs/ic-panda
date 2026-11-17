package net.dmsg.android.core

import android.content.Context
import kotlinx.coroutines.*
import net.dmsg.android.platform.*
import net.dmsg.core.MobileEngine

class BackgroundFileCoordinator(private val client: ClientController, private val context: Context, private val scope: CoroutineScope) {
    val current = ReadStore<CipherJob?> { it == null }
    private val scheduler = CipherScheduler(context)
    private var watch: Job? = null
    private fun store() = cipherStore(context) ?: error("INSTALLATION_MISMATCH")
    /** Called on IO before advancing a native foreground queue. */
    fun holds(kind: String, account: String?): Boolean {
        val job = store().snapshot() ?: return false
        return job.plan.account == account && (kind == "upload") == (job.plan.method == "PUT")
    }
    fun start() {
        if (watch?.isActive == true) return
        watch = scope.launch { while (isActive) { load(); delay(1000) } }
    }
    fun load() {
        if (client.isWorking("cipher:status")) return
        val read = current.begin()
        client.perform({
            val store = store()
            val value = store.snapshot()
            if (value?.phase == "transferring" && !scheduler.exists(value) && !client.isWorking("cipher:prepare")) store.pause("TRANSFER_STOPPED")
            store.snapshot()
        }, key = "cipher:status", quiet = true, onError = { current.fail(read, it) }) { current.accept(read, it) }
    }
    fun upload(file: LibraryFile) = prepare { core ->
        val admission = parseBackupAdmission(core.backupFile(file.id, file.version).decode())
        check(admission.accepted && "backed_up" !in admission.skipped) { "BACKUP_NOT_STARTED" }
        parseCipherPlan(client.operation { core.backgroundBackup(file.id, file.version, it) }.decode())
    }
    fun download(upload: String, folder: String) = prepare { core -> parseCipherPlan(client.operation { core.backgroundDownload(upload, folder, it) }.decode()) }
    private fun prepare(create: suspend (MobileEngine) -> CipherPlan) {
        if (client.isWorking("cipher:prepare")) return
        val stamp = client.currentStamp() ?: return
        val stopped = client.cancelForHandoff("files:run:upload", "files:run:download")
        client.fileWork.pause()
        client.perform({ core ->
            stopped.joinAll()
            val store = store()
            check(store.snapshot() == null) { "TRANSFER_PENDING" }
            val plan = create(core)
            check(plan.account == stamp.account && plan.device == client.state.value.session?.deviceId) { "FORBIDDEN" }
            store.start(plan, System.currentTimeMillis().coerceAtLeast(0).toULong())
            store.pause("PREPARING_CIPHERTEXT")
            stageUpload(core, plan)
            store.retry(System.currentTimeMillis().coerceAtLeast(0).toULong())
        }, key = "cipher:prepare", onError = { load() }) { value ->
            schedule(value); client.navigation.push(Route("file-transfers"))
        }
    }
    private suspend fun stageUpload(core: MobileEngine, plan: CipherPlan) {
        if (plan.method != "PUT") return
        for (part in plan.parts) {
            currentCoroutineContext().ensureActive()
            val store = store()
            if (runCatching { store.verifiedPart(plan, part.slot).also { it.fill(0) } }.isSuccess) continue
            val bytes = core.backgroundUploadPart(plan.id, part.slot)
            try { currentCoroutineContext().ensureActive(); store.stage(plan, part.slot, bytes) }
            finally { bytes.fill(0) }
        }
    }
    private fun schedule(value: CipherJob) {
        try { if (!value.ready) scheduler.schedule(value) }
        catch (_: Throwable) { store().pause("SYSTEM_TRANSFER_UNAVAILABLE"); client.showError("SYSTEM_TRANSFER_UNAVAILABLE") }
        load()
    }
    fun pause() = client.perform({ store().pause(); scheduler.cancel() }, key = "cipher:pause") { load() }
    fun retry() {
        val value = current.value() ?: return
        if (value.plan.account != client.state.value.session?.account) return
        client.perform({ core ->
            val store = store()
            val actual = store.snapshot() ?: error("NOT_FOUND")
            check(actual.plan.id == value.plan.id && actual.plan.expires > System.currentTimeMillis().coerceAtLeast(0).toULong()) { "TRANSFER_EXPIRED" }
            scheduler.cancel(); store.pause()
            stageUpload(core, actual.plan)
            store.retry(System.currentTimeMillis().coerceAtLeast(0).toULong())
        }, key = "cipher:prepare", onError = { load() }) { schedule(it) }
    }
    fun finish() {
        val value = current.value() ?: return
        if (!value.ready || value.plan.account != client.state.value.session?.account) return
        client.perform({ core ->
            val store = store()
            check(store.snapshot()?.let { it.plan.id == value.plan.id && it.ready } == true) { "TRANSFER_PENDING" }
            if (value.plan.method == "GET") for (part in value.plan.parts) {
                currentCoroutineContext().ensureActive()
                val bytes = store.verifiedPart(value.plan, part.slot)
                try { core.backgroundAcceptPart(value.plan.id, part.slot, bytes) } finally { bytes.fill(0) }
            }
            client.operation { core.backgroundFinish(value.plan.id, it) }
            store.discard(value.plan)
        }, key = "cipher:finish", onError = { load() }) {
            load(); client.fileWork.resume(false); client.library.browse(); client.cloudFiles.browse(true)
        }
    }
    fun cancel() {
        val value = current.value() ?: return
        if (value.plan.account != client.state.value.session?.account) return
        client.perform({ core ->
            val store = store()
            check(store.snapshot()?.plan?.id == value.plan.id) { "NOT_FOUND" }
            store.pause(); scheduler.cancel()
            core.backgroundRevoke(value.plan.id); store.discard(value.plan)
        }, key = "cipher:cancel", onError = { load() }) { load(); client.fileWork.load() }
    }
    /** Explicit local removal/logout stops this installation's executor, even offline. */
    fun forget(account: String) {
        val store = cipherStore(context) ?: return
        val value = store.snapshot()?.takeIf { it.plan.account == account } ?: return
        store.pause(); scheduler.cancel(); store.discard(value.plan); current.reset()
    }
    fun clear() { watch?.cancel(); watch = null; client.cancelOperation("cipher:status"); current.reset() }
}
