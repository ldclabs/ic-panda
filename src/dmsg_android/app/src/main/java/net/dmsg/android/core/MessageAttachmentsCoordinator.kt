package net.dmsg.android.core

import android.graphics.Bitmap
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.serialization.json.JsonNull
import net.dmsg.core.MessageItem
import net.dmsg.android.platform.BoundedImages

data class MessageAttachmentState(val file: LibraryFile? = null, val thumbnail: Bitmap? = null, val checked: Boolean = false,
    val reading: Boolean = false, val saving: Boolean = false, val completed: ULong = 0u, val total: ULong = 0u, val error: String? = null,
    val name: String = "", val waiting: Boolean = false)

/** A local lookup never downloads an attachment; saving always follows an explicit action. */
class MessageAttachmentsCoordinator(private val client: ClientController) {
    private val mutable = MutableStateFlow<Map<String, MessageAttachmentState>>(emptyMap())
    val states = mutable.asStateFlow()
    private val revisionValue = MutableStateFlow(0L)
    val revision = revisionValue.asStateFlow()
    private val owners = mutableMapOf<String, Long>()
    private val observed = mutableSetOf<String>()
    private val contexts = mutableMapOf<String, Pair<String, MessageItem>>()
    private val downloads = Semaphore(2)
    private var sequence = 0L
    fun key(channel: String, number: ULong) = "$channel:$number"
    fun observe(channel: String, message: MessageItem) { val key = key(channel, message.sequence); observed += key; contexts[key] = channel to message; inspect(channel, message) }
    fun release(channel: String, number: ULong) {
        val key = key(channel, number); observed -= key
        if (mutable.value[key]?.saving != true) { client.cancelOperation("message:file-info:$key"); mutable.value -= key; owners.remove(key); contexts.remove(key) }
    }
    fun inspect(channel: String, message: MessageItem, force: Boolean = false) {
        if (message.unreadable || message.retracted || message.attachmentDeleted || message.attachmentMime == "application/vnd.dmsg.secret-package") return
        val key = key(channel, message.sequence)
        val previous = mutable.value[key]
        if (previous?.saving == true || previous?.reading == true || previous?.checked == true && !force) return
        val owner = ++sequence; owners[key] = owner
        mutable.value += key to (previous ?: MessageAttachmentState()).copy(reading = true, error = null)
        client.perform({ core ->
            val value = core.savedAttachment(channel, message.sequence).decode()
            val file = if (value == JsonNull) null else parseFile(value.objectValue())
            val thumbnail = if (file?.mime?.startsWith("image/") == true) try {
                val preview = core.filePreview(file.id, file.version).decode().objectValue()
                preview.text("content_base64url")?.takeIf { preview.text("kind") == "image" }?.let { BoundedImages.thumbnail(it) }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Throwable) { null } else null
            MessageAttachmentState(file, thumbnail, checked = true, name = message.attachmentName.orEmpty())
        }, key = "message:file-info:$key", replace = true, quiet = true, onError = {
            if (owners[key] == owner) mutable.value += key to MessageAttachmentState(error = it)
        }) { if (owners[key] == owner) mutable.value += key to it }
    }
    fun save(channel: String, message: MessageItem) {
        if (message.unreadable || message.retracted || message.attachmentDeleted || message.attachmentMime == "application/vnd.dmsg.secret-package") return
        val key = key(channel, message.sequence)
        if (mutable.value[key]?.saving == true) return
        val stamp = client.currentStamp() ?: return
        val owner = ++sequence; owners[key] = owner
        contexts[key] = channel to message
        client.cancelOperation("message:file-info:$key")
        mutable.value += key to MessageAttachmentState(saving = true, name = message.attachmentName.orEmpty(), waiting = true)
        client.perform({ core -> downloads.withPermit { client.operation { token -> coroutineScope {
            val watching = launch {
                while (isActive) {
                    val progress = token.progress()
                    withContext(Dispatchers.Main.immediate) {
                        if (client.accepts(stamp) && owners[key] == owner) mutable.value += key to MessageAttachmentState(saving = true, completed = progress.completed, total = progress.total, name = message.attachmentName.orEmpty())
                    }
                    delay(250)
                }
            }
            try { parseFile(core.saveAttachment(channel, message.sequence, token).decode().objectValue()["file"]!!.objectValue()) }
            finally { watching.cancel() }
        } } } }, key = "message:file-save:$key", onError = {
            if (owners[key] == owner) mutable.value += key to MessageAttachmentState(checked = true, error = it, name = message.attachmentName.orEmpty())
        }) { file ->
            if (owners[key] == owner) mutable.value += key to MessageAttachmentState(file = file, checked = true, name = file.name)
            client.library.browse(); client.notice("FILE_SAVED_LOCALLY")
            if (key in observed) inspect(channel, message, force = true) else release(channel, message.sequence)
        }
    }
    fun cancel(channel: String, message: MessageItem) {
        val key = key(channel, message.sequence)
        owners[key] = ++sequence
        client.cancelOperation("message:file-save:$key")
        mutable.value += key to MessageAttachmentState(error = "CANCELLED")
        inspect(channel, message, force = true)
    }
    fun invalidate() {
        mutable.value.filterValues { !it.saving }.keys.forEach { client.cancelOperation("message:file-info:$it"); owners.remove(it) }
        mutable.value = mutable.value.filterValues { it.saving }
        revisionValue.value += 1
    }
    fun stop(key: String) { contexts[key]?.let { (channel, message) -> cancel(channel, message) } }
    fun show(key: String) { contexts[key]?.let { (channel, _) -> client.navigation.open("messages"); client.navigation.select(channel) } }
    fun clear() {
        mutable.value.keys.forEach { client.cancelOperation("message:file-info:$it"); client.cancelOperation("message:file-save:$it") }
        owners.clear(); contexts.clear(); observed.clear(); mutable.value = emptyMap(); revisionValue.value += 1
    }
}
