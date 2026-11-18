package net.dmsg.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.JsonElement

data class DurableFileJob(val id: String, val kind: String, val name: String?, val file: String?, val version: String?,
    val stage: String, val chunksDone: ULong, val chunksTotal: ULong, val bytes: ULong, val totalBytes: ULong,
    val error: String?, val retryAt: ULong?) {
    fun due(now: ULong) = stage != "done" && (error == null || retryAt?.let { it <= now } == true)
}
fun parseFileJobs(value: JsonElement, kind: String): List<DurableFileJob> = value.arrayRows().map {
    check(kind in setOf("upload", "download")) { "INTEGRITY_FAILED" }
    DurableFileJob(it.required("upload_id"), kind, it.text("name"), it.text("file"), it.text("version"), it.required("stage"),
        it.requiredUnsigned(if (kind == "upload") "sent" else "done"), it.requiredUnsigned(if (kind == "upload") "chunks" else "total"),
        it.requiredUnsigned("bytes"), it.requiredUnsigned("total_bytes"), it.text("error"), it.unsigned("retry_at"))
}

/** Drives only already-requested Rust journals while this account is visibly unlocked. */
class FileWorkCoordinator(private val client: ClientController, private val scope: CoroutineScope) {
    val jobs = ReadStore<List<DurableFileJob>> { it.isEmpty() }
    data class PassFailure(val code: String, val retryAfter: Long)
    private val failures = MutableStateFlow<Map<String, PassFailure>>(emptyMap())
    val passFailures = failures.asStateFlow()
    private val attempts = mutableMapOf<String, Int>()
    private var watch: Job? = null
    private var owner: SessionStamp? = null
    private var paused = false
    fun start() {
        val stamp = client.currentStamp()?.takeIf(client::accepts) ?: return
        if (watch?.isActive == true && owner == stamp) return
        stop(); owner = stamp
        watch = scope.launch {
            while (isActive && client.accepts(stamp)) {
                load(advance = !paused && !client.synchronization.constrained.value)
                delay(if (client.synchronization.constrained.value) 15_000 else 1_000)
            }
        }
    }
    fun load(advance: Boolean = false) {
        if (client.isWorking("files:jobs")) return
        val request = jobs.begin()
        client.perform({ core -> coroutineScope {
            val upload = async { parseFileJobs(core.backupStatus().decode(), "upload") }
            val download = async { parseFileJobs(core.downloadStatus().decode(), "download") }
            upload.await() + download.await()
        } }, key = "files:jobs", quiet = true, onError = { jobs.fail(request, it) }) { values ->
            jobs.accept(request, values)
            if (advance) {
                val now = System.currentTimeMillis().coerceAtLeast(0).toULong()
                if (values.any { it.kind == "upload" && it.due(now) }) run("upload")
                if (values.any { it.kind == "download" && it.due(now) }) run("download")
            }
        }
    }
    private fun run(kind: String) {
        val key = "files:run:$kind"
        if (client.isWorking(key) || (failures.value[kind]?.retryAfter ?: 0) > android.os.SystemClock.elapsedRealtime()) return
        val account = owner?.account
        client.perform({ core ->
            if (client.backgroundFiles.holds(kind, account)) false
            else { client.operation { if (kind == "upload") core.runBackups(it) else core.runDownloads(it) }; true }
        },
            key = key, quiet = true, onError = { code ->
                val count = ((attempts[kind] ?: 0) + 1).coerceAtMost(6)
                attempts[kind] = count
                val delay = (15_000L * (1L shl (count - 1))).coerceAtMost(300_000L)
                failures.value += kind to PassFailure(code, android.os.SystemClock.elapsedRealtime() + delay)
                load()
            }) { advanced ->
            if (advanced) {
                failures.value -= kind; attempts.remove(kind)
                load(); client.library.browse(); client.cloudFiles.browse(false)
            }
        }
    }
    fun pause() {
        paused = true
        client.cancelOperation("files:run:upload"); client.cancelOperation("files:run:download")
        load()
    }
    fun resume(retryFailures: Boolean = true) {
        paused = false
        failures.value = emptyMap(); attempts.clear()
        if (retryFailures) client.perform({ it.retryFileTransfers() }, key = "files:retry") { load(true) }
        else load(true)
    }
    fun networkChanged() {
        val stamp = owner ?: return
        if (paused || !client.accepts(stamp)) return
        failures.value = emptyMap()
        load(true)
    }
    fun queued() { paused = false; load(true) }

    fun backup(file: LibraryFile, immutableRevision: Boolean = false) {
        client.perform({ core ->
            val result = if (immutableRevision) core.backupFileRevision(file.id, file.version) else core.backupFile(file.id, file.version)
            parseBackupAdmission(result.decode()).outcome()
        }, key = "file:backup:${file.id}") { result ->
            if (result.accepted) { queued(); client.navigation.push(Route("file-transfers")) }
            else client.showError(result.code)
        }
    }

    fun restore(upload: String, folder: String) {
        client.perform({ core -> client.operation { core.restoreBackup(upload, folder, it) } }, key = "file:restore:$upload") {
            paused = false; load(true); client.navigation.push(Route("file-transfers"))
        }
    }
    fun cancelDownload(id: String) {
        client.cancelOperation("files:run:download")
        client.perform({ it.cancelDownload(id) }, key = "files:cancel:$id") { load(true) }
    }
    fun stop(clear: Boolean = false) {
        watch?.cancel(); watch = null; owner = null
        client.cancelOperation("files:jobs"); client.cancelOperation("files:run:upload"); client.cancelOperation("files:run:download")
        jobs.reset(); failures.value = emptyMap()
        if (clear) { attempts.clear(); paused = false }
    }
}
