package net.dmsg.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*

data class FileLink(val id: String, val source: String?, val file: String?, val version: String?, val name: String?, val size: ULong?,
    val url: String?, val expiresAt: ULong, val maxDownloads: ULong, val password: Boolean, val showProfile: Boolean,
    val state: String, val downloads: ULong, val observedAt: ULong?) {
    val active get() = state == "active" && expiresAt > System.currentTimeMillis().coerceAtLeast(0).toULong()
}
data class FileLinkIntent(val id: String, val file: String, val version: String, val name: String, val mime: String, val size: ULong,
    val backup: String?, val duration: ULong, val maxDownloads: ULong, val password: Boolean, val showProfile: Boolean,
    val stage: String, val createdAt: ULong, val updatedAt: ULong, val error: String?, val canDiscard: Boolean, val link: FileLink?)
data class FileLinkTarget(val id: String, val version: String?, val backup: Boolean, val name: String, val mime: String, val size: ULong, val remote: Boolean)
data class FileLinkDraft(val duration: ULong = 604_800_000u, val maxDownloads: ULong = 0u, val password: String = "", val showProfile: Boolean = false)
data class FileLinkProgress(val owner: Long, val completed: ULong = 0u, val total: ULong = 0u)

fun parseFileLink(value: JsonElement): FileLink = value.objectValue().let { row ->
    FileLink(row.required("share_id"), row["source"]?.takeIf { it != JsonNull }?.objectValue()?.required("type"),
        row.text("file"), row.text("version"), row.text("name"), row.unsigned("size"), row.text("url"),
        row.requiredUnsigned("expires_at"), row.requiredUnsigned("max_downloads"), row.requiredFlag("password"), row.requiredFlag("show_profile"),
        row.required("state"), row.requiredUnsigned("downloads"), row.unsigned("last_checked_at"))
}
fun parseFileLinkIntent(value: JsonElement): FileLinkIntent = value.objectValue().let { row ->
    val link = row["link"]?.takeIf { it != JsonNull }?.let(::parseFileLink)
    val stage = row.required("stage")
    check(stage != "complete" || link != null) { "INTEGRITY_FAILED" }
    FileLinkIntent(row.required("id"), row.required("file"), row.required("version"), row.required("name"), row.required("mime"), row.requiredUnsigned("size"),
        row.text("backup"), row.requiredUnsigned("expires_in_ms"), row.requiredUnsigned("max_downloads"), row.requiredFlag("password"), row.requiredFlag("show_profile"),
        stage, row.requiredUnsigned("created_at"), row.requiredUnsigned("updated_at"), row.text("error"), row.requiredFlag("can_discard"), link)
}

/** One durable intent owns publication; retries never call a fresh create API. */
class FileLinksCoordinator(private val client: ClientController) {
    val links = ReadStore<List<FileLink>> { it.isEmpty() }
    val intents = ReadStore<List<FileLinkIntent>> { it.isEmpty() }
    val target = ReadStore<FileLinkTarget>()
    private val draftValue = MutableStateFlow(FileLinkDraft())
    val draft = draftValue.asStateFlow()
    private val progressValue = MutableStateFlow<Map<String, FileLinkProgress>>(emptyMap())
    val progress = progressValue.asStateFlow()
    private var form = 0L
    private var sequence = 0L
    private var formKey: String? = null
    private var retained = false

    fun edit(value: FileLinkDraft) { client.activity(); draftValue.value = value }
    fun open(id: String, version: String?, backup: Boolean) {
        form += 1
        val key = "$backup:$id:$version"
        if (!retained || formKey != key) draftValue.value = FileLinkDraft()
        retained = false; formKey = key
        val request = target.begin(false)
        client.perform({ core ->
            if (backup) {
                val inventory = parseCloudInventory(client.operation { core.cloudBackups(false, it) }.decode(), core.cloudBackupStatus().decode())
                val found = inventory.items.singleOrNull { it.id == id } ?: error("NOT_FOUND")
                check(!found.needsRoot && found.name != null && found.size != null && found.mime != null) { "RECOVERY_INCOMPLETE" }
                FileLinkTarget(id, found.version, true, found.name, found.mime, found.size, !found.local)
            } else {
                val details = parseFileDetails(core.fileDetails(id).decode())
                val selected = details.versions.singleOrNull { it.id == version } ?: error("NOT_FOUND")
                FileLinkTarget(id, selected.id, false, details.file.name, selected.mime ?: (if (selected.current) details.file.mime else "application/octet-stream"), selected.size, !selected.local)
            }
        }, key = "links:target", replace = true, quiet = true, onError = { target.fail(request, it) }) { target.accept(request, it) }
    }
    fun leaveComposer() {
        form += 1; client.cancelOperation("links:prepare"); client.cancelOperation("links:target")
        draftValue.value = draftValue.value.copy(password = ""); target.reset()
    }
    fun browse(refresh: Boolean = true, refreshAfter: Boolean = false) {
        val request = links.begin()
        client.perform({ core -> client.operation { core.fileLinks(refresh, it) }.decode().arrayRows().map(::parseFileLink).filter { it.source != "vault_snapshot" } },
            key = "links:list:$refresh", replace = true, quiet = true, onError = { links.fail(request, it) }) {
            links.accept(request, it); if (refreshAfter) browse(true)
        }
        loadIntents()
    }
    fun loadIntents() {
        val request = intents.begin()
        client.perform({ it.fileLinkIntents().decode().arrayRows().map(::parseFileLinkIntent) }, key = "links:intents", replace = true, quiet = true,
            onError = { intents.fail(request, it) }) { intents.accept(request, it) }
    }
    private fun keep(value: FileLinkIntent) {
        val current = intents.value().orEmpty().filterNot { it.id == value.id }
        intents.accept(intents.begin(), (current + value).sortedByDescending { it.createdAt })
    }
    fun create() {
        if (client.isWorking("links:prepare")) return
        val source = target.value() ?: return
        val value = draftValue.value
        if (source.size > 100uL * 1024u * 1024u) { client.showError("FILE_TOO_LARGE"); return }
        if (value.password.toByteArray(Charsets.UTF_8).size > 4096) { client.showError("PASSWORD_TOO_LONG"); return }
        val owner = form
        client.perform({ core -> parseFileLinkIntent((if (source.backup) core.prepareBackupLink(source.id, value.duration, value.maxDownloads, value.password.takeIf { it.isNotEmpty() }, value.showProfile)
            else core.prepareFileLink(source.id, checkNotNull(source.version), value.duration, value.maxDownloads, value.password.takeIf { it.isNotEmpty() }, value.showProfile)).decode()) }, key = "links:prepare") { intent ->
            keep(intent)
            if (owner == form) {
                draftValue.value = draftValue.value.copy(password = "")
                client.navigation.push(Route("file-link-job", id = intent.id))
                resume(intent.id)
            }
        }
    }
    fun resume(id: String) {
        val key = "links:run:$id"
        if (client.isWorking(key)) return
        val stamp = client.currentStamp() ?: return
        val owner = ++sequence
        progressValue.value += id to FileLinkProgress(owner)
        client.perform({ core -> client.operation { token -> coroutineScope {
            val watching = launch {
                while (isActive) {
                    val next = token.progress()
                    withContext(Dispatchers.Main.immediate) {
                        if (client.accepts(stamp) && progressValue.value[id]?.owner == owner) progressValue.value += id to FileLinkProgress(owner, next.completed, next.total)
                    }
                    delay(250)
                }
            }
            try { parseFileLinkIntent(core.runFileLinkIntent(id, token).decode()) } finally { watching.cancel() }
        } } }, key = key, onError = { progressValue.value -= id; loadIntents() }) {
            progressValue.value -= id; keep(it); browse(false)
        }
    }
    fun pause(id: String) {
        client.cancelOperation("links:run:$id"); progressValue.value -= id
        if (client.currentStamp()?.let(client::accepts) == true) loadIntents()
    }
    fun discard(id: String, done: () -> Unit) {
        client.perform({ core -> client.operation { core.discardFileLinkIntent(id, it) } }, key = "links:discard:$id", onError = { loadIntents() }) { loadIntents(); done() }
    }
    fun revoke(id: String) {
        client.perform({ core -> parseFileLink(client.operation { core.revokeFileLink(id, it) }.decode()) }, key = "links:revoke:$id") { value ->
            links.accept(links.begin(), links.value().orEmpty().filterNot { it.id == id } + value)
            loadIntents()
        }
    }
    fun clear(retainForm: Boolean = false) {
        progressValue.value.keys.forEach { client.cancelOperation("links:run:$it") }
        listOf("links:prepare", "links:target", "links:list:true", "links:list:false", "links:intents").forEach(client::cancelOperation)
        form += 1; links.reset(); intents.reset(); target.reset(); progressValue.value = emptyMap()
        retained = retainForm
        if (!retainForm) formKey = null
        draftValue.value = if (retainForm) draftValue.value.copy(password = "") else FileLinkDraft()
    }
}
