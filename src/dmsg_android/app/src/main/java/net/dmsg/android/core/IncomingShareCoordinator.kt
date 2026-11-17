package net.dmsg.android.core

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.CancellationSignal
import android.provider.OpenableColumns
import androidx.core.content.IntentCompat
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.android.platform.*
import java.io.ByteArrayInputStream
import java.io.File
import java.io.InputStream
import java.util.UUID
import java.util.concurrent.atomic.AtomicReference

data class IncomingPayload(val uris: List<Uri>, val text: String?, val mime: String)
fun incomingPayload(intent: Intent): IncomingPayload? {
    if (intent.action !in setOf(Intent.ACTION_SEND, Intent.ACTION_SEND_MULTIPLE)) return null
    val uris = mutableListOf<Uri>()
    if (intent.action == Intent.ACTION_SEND_MULTIPLE) uris += IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
    else IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let(uris::add)
    val clip = intent.clipData
    if (clip != null) {
        check(clip.itemCount <= 10) { "FILE_LIMIT" }
        for (i in 0 until clip.itemCount) clip.getItemAt(i).uri?.let { if (it !in uris) uris += it }
    }
    check(uris.size <= 10 && uris.all { it.scheme == "content" && it.authority != null && it.toString().length <= 4096 }) { "INVALID_INPUT" }
    val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()?.takeIf { it.isNotBlank() }
    check(text == null || text.length <= 1024 * 1024 && text.toByteArray().size <= 1024 * 1024) { "FILE_LIMIT" }
    check(uris.isNotEmpty() || text != null) { "INVALID_INPUT" }
    check(uris.size + (if (text != null) 1 else 0) <= 10) { "FILE_LIMIT" }
    return IncomingPayload(uris, text, intent.type?.take(128) ?: "application/octet-stream")
}
data class IncomingIntake(val working: Boolean = false, val count: Int = 0, val error: String? = null, val duplicate: Boolean = false)

/** The external sender only gets a bounded encrypted queue. Import is a separate
 * action through the one foreground Client after the user chooses an account. */
class IncomingShareCoordinator(private val client: ClientController, private val context: Context, private val scope: CoroutineScope, private val paths: () -> PrivateFiles?) {
    private val root = File(context.noBackupFilesDir, "system/incoming")
    private val storage by lazy { IncomingShareStore(root, IncomingKeys(context, root)) }
    val entries = ReadStore<List<IncomingShare>> { it.isEmpty() }
    private val intakeValue = MutableStateFlow(IncomingIntake())
    val intake = intakeValue.asStateFlow()
    private val previewValue = MutableStateFlow<Map<String, String>>(emptyMap())
    val previews = previewValue.asStateFlow()
    private val imageValue = MutableStateFlow<Map<String, android.graphics.Bitmap>>(emptyMap())
    val images = imageValue.asStateFlow()
    private var receiving: Job? = null
    private val cleanup = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val input = AtomicReference<InputStream?>()
    private val signal = AtomicReference<CancellationSignal?>()
    private var reviewNavigation: Long? = null
    private var checked: SessionStamp? = null
    private var checking: Job? = null
    @OptIn(InternalCoroutinesApi::class)
    fun receive(intent: Intent, onFinished: () -> Unit = {}): Boolean {
        val payload = try {
            incomingPayload(intent)?.also { payload ->
                // Receiving our own private provider URI would use this app's UID
                // rather than the external sender's access grant.
                check(payload.uris.none { context.packageManager.resolveContentProvider(it.authority!!, 0)?.applicationInfo?.uid == android.os.Process.myUid() }) { "INVALID_INPUT" }
            }
        } catch (_: Throwable) { intakeValue.value = IncomingIntake(error = "INVALID_INPUT"); onFinished(); return true }
            ?: run { onFinished(); return false }
        if (receiving?.isActive == true) { intakeValue.value = intakeValue.value.copy(error = "PENDING"); onFinished(); return true }
        reviewNavigation = client.beginNavigationIntent()
        intakeValue.value = IncomingIntake(working = true)
        receiving = scope.launch {
            try {
                val result = withContext(Dispatchers.IO) {
                    var duplicated = false
                    val task = currentCoroutineContext()[Job]
                    fun current() = task?.isActive != false && !context.getSystemService(android.app.KeyguardManager::class.java).isDeviceLocked
                    for (uri in payload.uris) {
                        ensureActive()
                        val cancellation = CancellationSignal().also(signal::set)
                        try {
                            var name = "Shared file"; var size: Long? = null
                            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null, cancellation)?.use { cursor ->
                                if (cursor.moveToFirst()) {
                                    val nameColumn = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                                    val sizeColumn = cursor.getColumnIndex(OpenableColumns.SIZE)
                                    if (nameColumn >= 0 && !cursor.isNull(nameColumn)) name = cursor.getString(nameColumn)
                                    if (sizeColumn >= 0 && !cursor.isNull(sizeColumn)) size = cursor.getLong(sizeColumn).takeIf { it >= 0 }
                                }
                            }
                            check(size == null || size!! <= IncomingShareStore.MAX_FILE) { "FILE_LIMIT" }
                            val mime = context.contentResolver.getType(uri)?.take(128) ?: payload.mime
                            context.contentResolver.openAssetFileDescriptor(uri, "r", cancellation)?.use { descriptor ->
                                descriptor.createInputStream().use { stream ->
                                    input.set(stream)
                                    val item = storage.stage(stream, name, mime, size, CipherTransferStore.digest(uri.toString().toByteArray()), ::current)
                                    duplicated = duplicated || item == null
                                }
                            } ?: error("FILE_UNAVAILABLE")
                        } finally { input.set(null); signal.set(null) }
                    }
                    payload.text?.let { text ->
                        val bytes = text.toByteArray()
                        try {
                            val item = storage.stage(ByteArrayInputStream(bytes), "Shared text.txt", "text/plain", bytes.size.toLong(), CipherTransferStore.digest(bytes), ::current)
                            duplicated = duplicated || item == null
                        } finally { bytes.fill(0) }
                    }
                    storage.list().size to duplicated
                }
                intakeValue.value = IncomingIntake(count = result.first, duplicate = result.second)
                routeIfReady(); load()
            } catch (error: CancellationException) { intakeValue.value = intakeValue.value.copy(working = false, error = "CANCELLED"); throw error }
            catch (error: Throwable) { intakeValue.value = intakeValue.value.copy(working = false, error = client.errorCode(error)) }
            finally { input.getAndSet(null)?.let { stream -> cleanup.launch { runCatching { stream.close() } } }; signal.getAndSet(null)?.cancel() }
        }
        receiving?.invokeOnCompletion(onCancelling = true, invokeImmediately = true) { cause ->
            if (cause != null) closeInput()
        }
        receiving?.invokeOnCompletion { onFinished() }
        return true
    }
    private fun closeInput() {
        val stream = input.getAndSet(null); val cancellation = signal.getAndSet(null)
        cleanup.launch { runCatching { cancellation?.cancel() }; runCatching { stream?.close() } }
    }
    fun checkPending() {
        val stamp = client.currentStamp() ?: return
        if (checked == stamp || checking?.isActive == true) return
        checked = stamp
        checking = scope.launch {
            try {
                val pending = withContext(Dispatchers.IO) { storage.list().size to storage.pendingReview() }
                intakeValue.value = intakeValue.value.copy(count = pending.first)
                if (client.sessionCurrent(stamp)) {
                    if (pending.second && client.navigationIntent == 0L) reviewNavigation = 0L
                    routeIfReady()
                }
            } catch (_: Throwable) { /* The queue remains available for explicit retry after device unlock. */ }
        }
    }
    fun acknowledge() = client.perform({ storage.acknowledgeReview() }, key = "incoming:reviewed", quiet = true) { load() }
    fun routeIfReady() {
        val request = reviewNavigation ?: return
        if (request != client.navigationIntent) { reviewNavigation = null; return }
        if (client.state.value.unlocked && !client.state.value.concealed) { reviewNavigation = null; client.navigation.push(Route("incoming-shares")) }
    }
    fun load() {
        if (!client.state.value.unlocked) return
        val read = entries.begin()
        client.perform({ storage.list() }, key = "incoming:list", replace = true, quiet = true, onError = { entries.fail(read, it) }) { values -> entries.accept(read, values); intakeValue.value = intakeValue.value.copy(count = values.size) }
    }
    fun preview(item: IncomingShare) = client.perform({ storage.preview(item) }, key = "incoming:preview:" + item.id) { text ->
        if (text != null) previewValue.value += item.id to text
    }
    fun previewImage(item: IncomingShare) = client.perform({
        val bytes = storage.image(item)
        try { IncomingImages.decode(bytes) } finally { bytes.fill(0) }
    }, key = "incoming:image:" + item.id) { bitmap -> imageValue.value += item.id to bitmap }
    fun import(item: IncomingShare, folder: String) {
        val stamp = client.currentStamp()?.takeIf(client::accepts) ?: return
        val account = stamp.account ?: return
        val paths = paths() ?: return
        client.transfers.runLocal("incoming:import:" + item.id, item.name, stamp, { core, operation ->
            val importing = currentCoroutineContext()[Job]
            val claimed = storage.claim(item.id, account, folder)
            val directory = File(paths.staging, account + "/dmsg-import-" + UUID.randomUUID()).apply { check(mkdirs()) }
            val target = File(directory, claimed.name)
            try {
                storage.materialize(claimed, target) { importing?.isActive != false && client.sessionCurrent(stamp) && !client.state.value.concealed }
                val handle = account + "/" + directory.name + "/" + target.name
                val imported = parseFile(core.importFile(handle, claimed.folder ?: folder, claimed.id, operation).decode().objectValue()["file"]!!.objectValue())
                storage.remove(claimed.id)
                imported
            } finally { directory.deleteRecursively() }
        }, onError = { load() }) {
            previewValue.value -= item.id; imageValue.value -= item.id; load(); client.library.browse(); client.notice("FILE_SAVED_LOCALLY")
        }
    }
    fun discard(item: IncomingShare) = client.perform({ storage.remove(item.id) }, key = "incoming:discard:" + item.id) { previewValue.value -= item.id; imageValue.value -= item.id; load() }
    fun forget(account: String) { storage.removeAccount(account) }
    fun clear() { checked = null; checking?.cancel(); entries.reset(); previewValue.value = emptyMap(); imageValue.value = emptyMap() }
    fun stop() {
        receiving?.cancel(); receiving = null
        closeInput()
        clear()
    }
}
