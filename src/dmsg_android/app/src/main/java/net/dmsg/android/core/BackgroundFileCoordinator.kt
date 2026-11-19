package net.dmsg.android.core

import android.content.Context
import kotlinx.coroutines.*
import net.dmsg.android.platform.*
import net.dmsg.core.MobileEngine

class BackgroundFileCoordinator(private val client: ClientController, private val context: Context) {
    val current = ReadStore<CipherJob?> { it == null }
    private val scheduler = CipherScheduler(context)
    private fun store() = cipherStore(context) ?: error("INSTALLATION_MISMATCH")
    /** Called on IO before advancing a native foreground queue. */
    fun holds(kind: String, account: String?): Boolean {
        val job = store().snapshot() ?: return false
        return job.plan.account == account && (kind == "upload") == (job.plan.method == "PUT")
    }
    fun load() {
        if (client.isWorking("cipher:status")) return
        val read = current.begin()
        client.perform({
            val store = store()
            val value = store.snapshot()
            if (value?.phase == CipherPhase.TRANSFERRING && !scheduler.exists(value) && !client.isWorking("cipher:prepare")) {
                store.pause(value.plan.id, value.attempt, "TRANSFER_STOPPED")
                store.snapshot()
            } else value
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
        client.perform({ core ->
            stopped.joinAll()
            val store = store()
            check(store.snapshot() == null) { "TRANSFER_PENDING" }
            val plan = create(core)
            check(plan.account == stamp.account && plan.device == client.state.value.session?.deviceId) { "FORBIDDEN" }
            val initial = store.start(plan, System.currentTimeMillis().coerceAtLeast(0).toULong())
            stageUpload(core, plan)
            store.retry(initial, System.currentTimeMillis().coerceAtLeast(0).toULong())
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
        catch (_: Exception) {
            if (store().pause(value.plan.id, value.attempt, "SYSTEM_TRANSFER_UNAVAILABLE")) client.showError("SYSTEM_TRANSFER_UNAVAILABLE")
        }
        load()
    }
    /** Stop OS-owned HTTP work before entering an offline workspace; keep its resumable plan. */
    fun pauseForLocalAccount() {
        val store = cipherStore(context) ?: return
        store.snapshot()?.let { job -> store.pause(job.plan.id, job.attempt, stop = scheduler::cancel) }
    }

    fun pause() {
        val value = current.value() ?: return
        if (value.plan.account != client.state.value.session?.account) return
        client.perform({ store().pause(value.plan.id, value.attempt, stop = scheduler::cancel) }, key = "cipher:pause") { load() }
    }
    fun retry() {
        val value = current.value() ?: return
        if (value.plan.account != client.state.value.session?.account) return
        client.perform({ core ->
            val store = store()
            val actual = store.snapshot() ?: error("NOT_FOUND")
            check(actual.plan.id == value.plan.id && actual.attempt == value.attempt &&
                actual.phase in setOf(CipherPhase.PAUSED, CipherPhase.FAILED)) { "TRANSFER_STOPPED" }
            check(actual.plan.expires > System.currentTimeMillis().coerceAtLeast(0).toULong()) { "TRANSFER_EXPIRED" }
            stageUpload(core, actual.plan)
            store.retry(actual, System.currentTimeMillis().coerceAtLeast(0).toULong())
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
            load(); client.fileWork.load(true); client.library.browse(); client.cloudFiles.browse(true)
        }
    }
    fun cancel() {
        val value = current.value() ?: return
        if (value.plan.account != client.state.value.session?.account) return
        client.perform({ core ->
            val store = store()
            val actual = store.snapshot()?.takeIf { it.plan.id == value.plan.id } ?: error("NOT_FOUND")
            store.pause(actual.plan.id, actual.attempt, stop = scheduler::cancel)
            core.backgroundRevoke(value.plan.id); store.discard(value.plan)
        }, key = "cipher:cancel", onError = { load() }) { load(); client.fileWork.load() }
    }
    /** Explicit local removal/logout stops this installation's executor, even offline. */
    fun forget(account: String) {
        val store = cipherStore(context) ?: return
        val value = store.snapshot()?.takeIf { it.plan.account == account } ?: return
        store.pause(value.plan.id, value.attempt, stop = scheduler::cancel); store.discard(value.plan); current.reset()
    }
    fun clear() { client.cancelOperation("cipher:status"); current.reset() }
}
