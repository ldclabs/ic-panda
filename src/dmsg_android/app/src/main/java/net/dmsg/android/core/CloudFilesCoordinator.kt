package net.dmsg.android.core

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonElement

data class CloudBackup(val id: String, val file: String?, val version: String?, val name: String?, val path: List<String>,
    val size: ULong?, val mime: String?, val sha256: String?, val device: String, val generation: ULong,
    val createdAt: ULong?, val committedAt: ULong?, val bytes: ULong, val needsRoot: Boolean, val local: Boolean)
data class CloudInventory(val items: List<CloudBackup>, val complete: Boolean, val observedAt: ULong?)
data class BackupCapacity(val fileLimit: ULong?, val storageLimit: ULong?, val committed: ULong, val reserved: ULong, val requestLimit: ULong?) {
    val remaining: ULong? get() = storageLimit?.let { if (it > committed && it - committed > reserved) it - committed - reserved else 0uL }
}
data class BackupAdmission(val uploads: List<String>, val skipped: List<String>) {
    val accepted get() = uploads.isNotEmpty() || skipped.isNotEmpty() && skipped.all { it in setOf("backed_up", "in_progress") }
}
data class BackupVersionKey(val file: String, val revision: String)
data class ReviewedBackup(val file: LibraryFile, val at: ULong, val current: Boolean) {
    val key get() = BackupVersionKey(file.id, file.version)
    val path get() = (file.path + file.name).joinToString(" / ")
}
data class BackupOutcome(val code: String, val accepted: Boolean)
data class BackupBatch(
    val selected: Set<BackupVersionKey> = emptySet(),
    val review: List<ReviewedBackup> = emptyList(),
    val outcomes: Map<BackupVersionKey, BackupOutcome> = emptyMap(),
) {
    val pending get() = review.filter { outcomes[it.key]?.accepted != true }
    val bytesToReserve get() = pending.filterNot { it.file.status == "backing_up" }.sumOf { it.file.size }
    fun fits(capacity: BackupCapacity): Boolean {
        val limit = capacity.fileLimit ?: return false
        return pending.isNotEmpty() && pending.all { it.file.status == "backing_up" || it.file.size <= limit } &&
            (capacity.remaining?.let { bytesToReserve <= it } ?: true)
    }
}

fun BackupAdmission.outcome(): BackupOutcome = when {
    uploads.isNotEmpty() -> BackupOutcome("queued", true)
    accepted -> BackupOutcome(skipped.first(), true)
    else -> BackupOutcome(when (skipped.firstOrNull()) {
        "too_large" -> "FILE_TOO_LARGE"
        "not_local" -> "NOT_LOCAL"
        else -> "BACKUP_NOT_STARTED"
    }, false)
}

fun parseCloudInventory(items: JsonElement, status: JsonElement): CloudInventory {
    val observed = status.objectValue()
    val complete = observed.requiredFlag("complete")
    val at = observed.unsigned("observed_at_ms")
    check(complete == (at != null)) { "INTEGRITY_FAILED" }
    return CloudInventory(items.arrayRows().map { row ->
        val backup = CloudBackup(row.required("upload_id"), row.text("file_id"), row.text("version_id"), row.text("name"), row.strings("path"),
            row.unsigned("size"), row.text("mime"), row.text("sha256"), row.required("device"), row.requiredUnsigned("generation"),
            row.unsigned("created_at"), row.unsigned("committed_at"), row.requiredUnsigned("bytes"), row.requiredFlag("needs_root"), row.requiredFlag("local"))
        if (!backup.needsRoot) check(backup.file != null && backup.version != null && backup.name != null && backup.size != null && backup.mime != null && backup.sha256 != null) { "INTEGRITY_FAILED" }
        check(!backup.local || !backup.needsRoot) { "INTEGRITY_FAILED" }
        backup
    }, complete, at)
}
fun parseBackupCapacity(value: JsonElement): BackupCapacity = value.objectValue().let {
    BackupCapacity(it.unsigned("backup_max_file_bytes"), it.unsigned("storage_limit"), it.requiredUnsigned("committed"), it.requiredUnsigned("reserved"), it.unsigned("file_request_limit"))
}
fun parseBackupAdmission(value: JsonElement): BackupAdmission = value.objectValue().let {
    BackupAdmission(it.rows("started").map { row -> row.required("upload_id") }, it.rows("skipped").map { row -> row.required("reason") })
}

/** Cloud inventory has its own observation and never stands in for account content sync. */
class CloudFilesCoordinator(private val client: ClientController) {
    val inventory = ReadStore<CloudInventory> { it.complete && it.items.isEmpty() }
    val unbacked = ReadStore<List<FileDetails>> { it.isEmpty() }
    val capacity = ReadStore<BackupCapacity>()
    private val batchValue = MutableStateFlow(BackupBatch())
    val batch = batchValue.asStateFlow()

    fun choose(key: BackupVersionKey, checked: Boolean) {
        if (client.isWorking("cloud:backup-batch")) return
        client.activity()
        val selected = batchValue.value.selected
        batchValue.value = BackupBatch(selected = if (checked) selected + key else selected - key)
    }

    fun reviewSelection() {
        if (client.isWorking("cloud:backup-batch") || unbacked.state.value !is ReadState.Ready) return
        val selected = batchValue.value.selected
        val rows = unbacked.value().orEmpty().flatMap { group -> group.versions.mapNotNull { version ->
            if (BackupVersionKey(group.file.id, version.id) in selected) ReviewedBackup(group.file.atVersion(version), version.createdAt, version.current) else null
        } }
        if (rows.isEmpty()) return
        batchValue.value = BackupBatch(rows.map { it.key }.toSet(), rows)
        client.navigation.push(Route("file-backup", field = "batch"))
    }

    fun queueReviewed() {
        val reviewed = batchValue.value
        val limits = (capacity.state.value as? ReadState.Ready)?.value ?: return
        if (client.isWorking("cloud:backup-batch") || !reviewed.fits(limits)) return
        val stamp = client.currentStamp() ?: return
        client.perform({ core ->
            for (row in reviewed.pending) {
                currentCoroutineContext().ensureActive()
                val result = try {
                    parseBackupAdmission(core.backupFileRevision(row.file.id, row.file.version).decode()).outcome()
                } catch (e: CancellationException) { throw e }
                catch (e: Exception) { BackupOutcome(client.errorCode(e), false) }
                withContext(Dispatchers.Main.immediate) {
                    if (client.accepts(stamp) && batchValue.value.review === reviewed.review) {
                        val batch = batchValue.value
                        batchValue.value = batch.copy(outcomes = batch.outcomes + (row.key to result),
                            selected = if (result.accepted) batch.selected - row.key else batch.selected)
                        if (result.accepted) client.fileWork.queued()
                    }
                }
            }
        }, key = "cloud:backup-batch") { listUnbacked(); capacity(); client.accountWorkspace.local() }
    }

    fun browse(refresh: Boolean = true, refreshAfter: Boolean = false) {
        val request = inventory.begin()
        client.perform({ core ->
            val items = client.operation { core.cloudBackups(refresh, it) }.decode()
            parseCloudInventory(items, core.cloudBackupStatus().decode())
        }, key = "cloud:list:$refresh", replace = true, quiet = true, onError = { inventory.fail(request, it) }) {
            inventory.accept(request, it)
            if (refreshAfter) browse(true)
        }
    }
    fun listUnbacked() {
        val request = unbacked.begin()
        client.perform({ core -> core.unbackedFileVersions().decode().arrayRows().map(::parseFileDetails) },
            key = "cloud:unbacked", replace = true, quiet = true, onError = { unbacked.fail(request, it) }) {
            unbacked.accept(request, it)
            val keys = it.flatMap { group -> group.versions.map { v -> BackupVersionKey(group.file.id, v.id) } }.toSet()
            batchValue.value = batchValue.value.copy(selected = batchValue.value.selected.intersect(keys))
        }
    }
    fun capacity() {
        val request = capacity.begin()
        client.perform({ core -> parseBackupCapacity(client.operation { core.fileBackupCapacity(it) }.decode()) },
            key = "cloud:capacity", replace = true, quiet = true, onError = { capacity.fail(request, it) }) { capacity.accept(request, it) }
    }
    fun delete(backup: CloudBackup, done: () -> Unit) {
        client.perform({ core -> client.operation { core.deleteBackup(backup.id, it) } }, key = "cloud:delete:${backup.id}") {
            browse(); client.library.browse(); done()
        }
    }
    fun clear(retainBatch: Boolean = false) {
        client.cancelOperation("cloud:backup-batch")
        if (!retainBatch) batchValue.value = BackupBatch()
        inventory.reset(); capacity.reset(); unbacked.reset()
    }
}
