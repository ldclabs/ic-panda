package net.dmsg.android.core

import android.net.Uri
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import net.dmsg.android.platform.PrivateFiles
import net.dmsg.core.MobileEngine
import net.dmsg.core.OperationToken
import java.io.File

/** Activity-independent transfer ownership; native journals retain durable network jobs. */
class FileTransferCoordinator(private val client: ClientController, private val paths: () -> PrivateFiles?) {
    private data class ImportRequest(val stamp: SessionStamp, val folder: String, val channel: String? = null)
    private var importRequest: ImportRequest? = null
    internal fun pendingImportStamp(): SessionStamp? = importRequest?.stamp
    private var capture: PrivateFiles.Capture? = null
    private data class ExportRequest(val stamp: SessionStamp, val file: LibraryFile)
    private var exportRequest: ExportRequest? = null
    private val cleanupTasks = mutableMapOf<String, () -> Unit>()
    data class Transfer(val id: String, val kind: String, val name: String, val phase: String = "working", val completed: ULong = 0u, val total: ULong = 0u, val error: String? = null)
    private val mutable = MutableStateFlow<Map<String, Transfer>>(emptyMap())
    val tasks = mutable.asStateFlow()
    private fun begin(id: String, kind: String, name: String) { mutable.update { it + (id to Transfer(id, kind, name)) } }
    private fun finish(id: String, error: String? = null) { mutable.update { values -> values[id]?.let { values + (id to it.copy(phase = if (error == null) "done" else "failed", error = error)) } ?: values } }
    internal fun <T> runLocal(id: String, name: String, stamp: SessionStamp, work: suspend (MobileEngine, OperationToken) -> T,
        onError: (String) -> Unit = {}, apply: (T) -> Unit) {
        if (!client.accepts(stamp) || client.isWorking(id)) return
        begin(id, "import", name)
        client.perform({ core -> progress(id, stamp) { work(core, it) } }, key = id,
            onError = { finish(id, it); onError(it) }) { finish(id); apply(it) }
    }
    private suspend fun <T> progress(id: String, stamp: SessionStamp, work: suspend (OperationToken) -> T): T = client.operation { token ->
        coroutineScope {
            val observing = launch {
                while (isActive) {
                    val value = token.progress()
                    withContext(Dispatchers.Main.immediate) {
                        if (client.accepts(stamp)) mutable.update { values -> values[id]?.let { values + (id to it.copy(completed = value.completed, total = value.total)) } ?: values }
                    }
                    delay(200)
                }
            }
            try { work(token) } finally { observing.cancel() }
        }
    }
    fun beginImport(folder: String, channel: String? = null): Boolean {
        if (importRequest != null) return false
        val stamp = client.currentStamp()?.takeIf { it.account != null } ?: return false
        if (!client.accepts(stamp)) return false
        if (channel != null && (!client.messages.draftsReady.value || client.messages.composer(channel).attachments.size >= 10)) { client.showError("ATTACHMENT_LIMIT"); return false }
        importRequest = ImportRequest(stamp, folder, channel)
        client.beginExternalInteraction()
        return true
    }
    fun importResult(uri: Uri?) = importResults(listOfNotNull(uri))
    fun importResults(uris: List<Uri>) {
        val request = importRequest ?: return
        importRequest = null
        val captured = capture.also { capture = null }
        val cleanup = { captured?.let { paths()?.finishCapture(it) }; Unit }
        if (uris.isEmpty()) { cleanup(); return }
        val available = request.channel?.let { (10 - client.messages.composer(it).attachments.size).coerceAtLeast(0) } ?: 10
        if (uris.size > available) { cleanup(); client.showError("ATTACHMENT_LIMIT"); return }
        client.deliverExternalResult("file-import", request.stamp, discard = cleanup) { importFiles(uris, request, cleanup) }
    }
    fun beginCapture(): Uri? {
        val request = importRequest ?: return null
        val owner = request.stamp.account ?: return null
        if (!client.sessionCurrent(request.stamp)) { importRequest = null; return null }
        return try { paths()?.capture(owner)?.also { capture = it }?.uri }
        catch (error: Throwable) { importRequest = null; client.showError(client.errorCode(error)); null }
    }
    fun captureResult(saved: Boolean) = importResults(if (saved) listOfNotNull(capture?.uri) else emptyList())
    private fun importFiles(uris: List<Uri>, request: ImportRequest, cleanup: () -> Unit) {
        val (stamp, folder, channel) = request
        val files = paths() ?: run { cleanup(); return }
        val owner = stamp.account ?: run { cleanup(); return }
        val id = "file:import:" + requestId()
        cleanupTasks[id] = cleanup
        begin(id, "import", "")
        client.perform({ core ->
            try {
                for (uri in uris) {
                    val handle = files.stage(uri, owner, if (channel == null) PrivateFiles.MAX_FILE_BYTES else 100L * 1024 * 1024) { client.sessionCurrent(stamp) }
                    val imported = try { parseFile(progress(id, stamp) { core.importFile(handle, folder, null, it) }.decode().objectValue()["file"]!!.objectValue()) }
                    finally { files.finishImport(handle) }
                    withContext(Dispatchers.Main.immediate) {
                        if (client.accepts(stamp)) {
                            if (channel != null) client.messages.attach(channel, imported)
                            mutable.update { current -> current[id]?.let { current + (id to it.copy(name = imported.name)) } ?: current }
                        }
                    }
                }
            } finally { cleanup(); withContext(NonCancellable + Dispatchers.Main.immediate) { cleanupTasks.remove(id) } }
        }, key = id, onError = { finish(id, it) }) { finish(id); client.library.browse(); client.notice(if (channel == null) "FILE_SAVED_LOCALLY" else "ATTACHMENTS_READY") }
    }
    fun share(file: LibraryFile, open: (File, String) -> Unit) {
        val files = paths() ?: return
        val stamp = client.currentStamp() ?: return
        val owner = stamp.account ?: return
        val id = "file:open:" + file.id
        if (client.isWorking(id)) return
        begin(id, "open", file.name)
        client.perform({ core ->
            val opened = progress(id, stamp) { core.openFile(file.id, file.version, it) }.decode().objectValue()
            val nonce = opened.required("nonce")
            try { files.shareCopy(opened.required("path"), owner) { client.sessionCurrent(stamp) } }
            finally { withContext(NonCancellable) { runCatching { core.closeFile(nonce) } } }
        }, key = id, onError = { finish(id, it) }) { result -> finish(id); open(result, file.mime) }
    }
    fun beginExport(file: LibraryFile): Boolean {
        if (exportRequest != null) return false
        val stamp = client.currentStamp()?.takeIf(client::accepts) ?: return false
        exportRequest = ExportRequest(stamp, file)
        client.beginExternalInteraction()
        return true
    }
    fun exportResult(uri: Uri?) {
        val request = exportRequest ?: return
        exportRequest = null
        if (uri == null) return
        val files = paths() ?: return
        client.deliverExternalResult("file-export", request.stamp, discard = { files.removeIncompleteExport(uri) }) {
            val id = "file:export:" + request.file.id
            begin(id, "export", request.file.name)
            client.perform({ core ->
                var written = false
                try {
                    val opened = progress(id, request.stamp) { core.openFile(request.file.id, request.file.version, it) }.decode().objectValue()
                    val nonce = opened.required("nonce")
                    try {
                        check(opened.required("file") == request.file.id && opened.required("version") == request.file.version) { "INTEGRITY_FAILED" }
                        files.exportCopy(opened.required("path"), uri) { client.sessionCurrent(request.stamp) }
                        written = true
                    } finally { withContext(NonCancellable) { runCatching { core.closeFile(nonce) } } }
                } finally { if (!written) files.removeIncompleteExport(uri) }
            }, key = id, onError = { finish(id, it) }) { finish(id); client.notice("FILE_EXPORTED") }
        }
    }
    fun cancel(id: String) { client.cancelOperation(id); cleanupTasks.remove(id)?.let { runCatching(it) }; mutable.update { values -> values[id]?.let { values + (id to it.copy(phase = "interrupted")) } ?: values } }
    fun suspend() { mutable.update { values -> values.mapValues { (_, value) -> if (value.phase == "working") value.copy(phase = "interrupted") else value } } }
    fun clear() { mutable.value = emptyMap(); importRequest = null; exportRequest = null; cleanupTasks.values.forEach { runCatching(it) }; cleanupTasks.clear(); capture?.let { runCatching { paths()?.finishCapture(it) } }; capture = null }
}
