package net.dmsg.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*

data class FileRequest(val id: String, val url: String?, val title: String?, val instructions: String?, val label: String?,
    val expiresAt: ULong, val maxTotal: ULong, val createdAt: ULong, val status: String, val files: ULong, val bytes: ULong,
    val unread: Boolean, val observedAt: ULong?) {
    val shareable get() = status in setOf("open", "full", "paused") && expiresAt > System.currentTimeMillis().coerceAtLeast(0).toULong()
}
data class FileRequestIntent(val id: String, val batch: String, val title: String, val instructions: String, val label: String,
    val duration: ULong, val maxTotal: ULong?, val stage: String, val canDiscard: Boolean, val createdAt: ULong, val error: String?, val link: FileRequest?)
data class ReceivedFile(val id: String, val request: String, val state: String, val name: String?, val mime: String?, val size: ULong?,
    val senderName: String?, val senderNote: String?, val replaces: String?, val storedAt: ULong, val bytes: ULong, val savedFile: String?) {
    val canSave get() = state == "received"
    val canDelete get() = state in setOf("received", "saved", "unreadable")
}
data class RequestDraft(val title: String = "", val instructions: String = "", val names: String = "", val days: Int = 14, val maxTotal: ULong? = null) {
    val labels get() = names.lineSequence().map(String::trim).filter(String::isNotEmpty).toList()
    val valid get() = title.trim().isNotEmpty() && title.trim().toByteArray().size <= 512 && instructions.trim().toByteArray().size <= 4096 && labels.size <= 50 && labels.all { it.toByteArray().size <= 256 }
}
data class RequestPublicationProgress(val batch: String, val done: Int, val total: Int)

fun parseFileRequest(value: JsonElement): FileRequest = value.objectValue().let {
    FileRequest(it.required("request_id"), it.text("url"), it.text("title"), it.text("instructions"), it.text("label"), it.requiredUnsigned("expires_at"),
        it.requiredUnsigned("max_total_bytes"), it.requiredUnsigned("created_at"), it.required("status"), it.requiredUnsigned("files"), it.requiredUnsigned("bytes"), it.requiredFlag("unread"), it.unsigned("last_checked_at"))
}
fun parseFileRequestIntent(value: JsonElement): FileRequestIntent = value.objectValue().let {
    val link = it["link"]?.takeIf { value -> value != JsonNull }?.let(::parseFileRequest)
    val stage = it.required("stage")
    check(stage != "complete" || link?.id == it.required("id")) { "INTEGRITY_FAILED" }
    FileRequestIntent(it.required("id"), it.required("batch"), it.required("title"), it.required("instructions"), it.required("label"), it.requiredUnsigned("expires_in_ms"),
        it.unsigned("max_total_bytes"), stage, it.requiredFlag("can_discard"), it.requiredUnsigned("created_at"), it.text("error"), link)
}
fun parseReceivedFile(value: JsonElement): ReceivedFile = value.objectValue().let {
    val sender = it["sender"]?.takeIf { value -> value != JsonNull }?.objectValue()
    val file = ReceivedFile(it.required("upload_id"), it.required("request_id"), it.required("state"), it.text("name"), it.text("mime"), it.unsigned("size"),
        sender?.required("name"), sender?.required("note"), it.text("replaces"), it.requiredUnsigned("stored_at"), it.requiredUnsigned("bytes"), it.text("saved_file"))
    if (file.state in setOf("received", "saved")) check(file.name != null && file.mime != null && file.size != null) { "INTEGRITY_FAILED" }
    if (file.state == "saved") check(file.savedFile != null) { "INTEGRITY_FAILED" }
    file
}

/** Creation is a durable batch. Listing reads metadata; saving is always explicit. */
class FileRequestsCoordinator(private val client: ClientController) {
    data class Received(val request: String, val files: List<ReceivedFile>)
    val links = ReadStore<List<FileRequest>> { it.isEmpty() }
    val intents = ReadStore<List<FileRequestIntent>> { it.isEmpty() }
    val received = ReadStore<Received> { it.files.isEmpty() }
    private val draftValue = MutableStateFlow(RequestDraft())
    val draft = draftValue.asStateFlow()
    private val publicationValue = MutableStateFlow<RequestPublicationProgress?>(null)
    val publication = publicationValue.asStateFlow()
    private val savingValue = MutableStateFlow<Map<String, FileLinkProgress>>(emptyMap())
    val saving = savingValue.asStateFlow()
    private var selected: String? = null
    private var form = 0L
    private var sequence = 0L

    fun edit(value: RequestDraft) { client.activity(); draftValue.value = value }
    fun leaveComposer() { form += 1; client.cancelOperation("requests:prepare") }
    fun browse(refresh: Boolean = true, refreshAfter: Boolean = false) {
        val request = links.begin()
        client.perform({ core -> client.operation { core.fileRequests(refresh, it) }.decode().arrayRows().map(::parseFileRequest) },
            key = "requests:list:$refresh", replace = true, quiet = true, onError = { links.fail(request, it) }) {
            links.accept(request, it); if (refreshAfter) browse(true)
        }
        loadIntents()
    }
    fun loadIntents() {
        val request = intents.begin()
        client.perform({ it.fileRequestIntents().decode().arrayRows().map(::parseFileRequestIntent) }, key = "requests:intents", replace = true, quiet = true,
            onError = { intents.fail(request, it) }) { intents.accept(request, it) }
    }
    fun create() {
        val draft = draftValue.value
        if (!draft.valid || client.isWorking("requests:prepare")) return
        val owner = form
        client.perform({ it.prepareFileRequests(draft.title.trim(), draft.instructions.trim(), draft.labels, draft.days.toULong() * 86_400_000u, draft.maxTotal).decode().arrayRows().map(::parseFileRequestIntent) },
            key = "requests:prepare", onError = { loadIntents() }) { prepared ->
            intents.accept(intents.begin(), intents.value().orEmpty() + prepared)
            if (owner == form) {
                draftValue.value = RequestDraft()
                val batch = prepared.first().batch
                client.navigation.push(Route("file-request-batch", id = batch))
                resumeBatch(batch)
            }
        }
    }
    fun resumeBatch(batch: String) {
        if (client.isWorking("requests:publish")) return
        val pending = intents.value().orEmpty().filter { it.batch == batch && it.stage in setOf("prepared", "checking") }
        if (pending.isEmpty()) return
        val stamp = client.currentStamp() ?: return
        publicationValue.value = RequestPublicationProgress(batch, 0, pending.size)
        client.perform({ core ->
            pending.forEachIndexed { index, intent ->
                client.operation { core.runFileRequestIntent(intent.id, it) }
                withContext(Dispatchers.Main.immediate) { if (client.accepts(stamp)) publicationValue.value = RequestPublicationProgress(batch, index + 1, pending.size) }
            }
        }, key = "requests:publish", onError = { publicationValue.value = null; browse(false) }) { publicationValue.value = null; browse(false) }
    }
    fun pausePublication() { client.cancelOperation("requests:publish"); publicationValue.value = null; loadIntents() }
    fun discard(id: String) = client.perform({ core -> client.operation { core.discardFileRequestIntent(id, it) } }, key = "requests:discard:$id", onError = { loadIntents() }) { loadIntents() }
    fun select(id: String) {
        val request = received.begin(selected == id); selected = id
        client.perform({ core -> Received(id, client.operation { core.fileRequestFiles(id, it) }.decode().arrayRows().map(::parseReceivedFile).also { files -> check(files.all { it.request == id }) { "INTEGRITY_FAILED" } }) },
            key = "requests:files", replace = true, quiet = true, onError = { received.fail(request, it) }) {
            received.accept(request, it)
            links.value()?.let { values -> links.accept(links.begin(), values.map { if (it.id == id) it.copy(unread = false) else it }) }
        }
    }
    fun leaveDetail() { client.cancelOperation("requests:files"); savingValue.value.keys.forEach { client.cancelOperation("requests:save:$it") }; received.reset(); selected = null; savingValue.value = emptyMap() }
    fun revoke(id: String) = client.perform({ core -> parseFileRequest(client.operation { core.revokeFileRequest(id, it) }.decode()) }, key = "requests:revoke:$id") { link ->
        links.accept(links.begin(), links.value().orEmpty().filterNot { it.id == id } + link); loadIntents()
    }
    fun save(file: ReceivedFile, folder: String?, root: String) {
        if (!file.canSave || client.isWorking("requests:save:${file.id}")) return
        val stamp = client.currentStamp() ?: return
        val owner = ++sequence
        savingValue.value += file.id to FileLinkProgress(owner)
        client.perform({ core -> client.operation { token -> coroutineScope {
            val watch = launch {
                while (isActive) {
                    val progress = token.progress()
                    withContext(Dispatchers.Main.immediate) {
                        if (client.accepts(stamp) && savingValue.value[file.id]?.owner == owner) savingValue.value += file.id to FileLinkProgress(owner, progress.completed, progress.total)
                    }
                    delay(250)
                }
            }
            try { core.saveRequestFile(file.id, folder, root, token) } finally { watch.cancel() }
        } } }, key = "requests:save:${file.id}", onError = { savingValue.value -= file.id; if (selected == file.request) select(file.request) }) {
            savingValue.value -= file.id
            if (selected == file.request) select(file.request)
            client.library.browse(); client.notice("FILE_SAVED_LOCALLY")
        }
    }
    fun cancelSave(id: String) { client.cancelOperation("requests:save:$id"); savingValue.value -= id; selected?.let(::select) }
    fun delete(file: ReceivedFile) {
        if (!file.canDelete) return
        client.perform({ core -> client.operation { core.deleteRequestFile(file.id, it) } }, key = "requests:delete:${file.id}") {
            if (selected == file.request) select(file.request)
            browse(); client.cloudFiles.capacity()
        }
    }
    fun clear() {
        leaveDetail()
        listOf("requests:prepare", "requests:publish", "requests:intents", "requests:list:true", "requests:list:false").forEach(client::cancelOperation)
        form += 1; links.reset(); intents.reset(); draftValue.value = RequestDraft(); publicationValue.value = null
    }
}
