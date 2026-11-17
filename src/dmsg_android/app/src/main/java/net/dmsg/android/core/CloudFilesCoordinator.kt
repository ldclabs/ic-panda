package net.dmsg.android.core

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
    val capacity = ReadStore<BackupCapacity>()
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
    fun clear() { inventory.reset(); capacity.reset() }
}
