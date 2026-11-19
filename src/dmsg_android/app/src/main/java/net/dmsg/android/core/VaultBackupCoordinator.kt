package net.dmsg.android.core

import android.net.Uri
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.android.platform.PrivateFiles
import net.dmsg.core.MobileEngine

data class BackupForm(
    val step: String = "home", val id: String = "", val input: String? = null,
    val view: JsonObject = JsonObject(emptyMap()), val approval: String? = null,
    val output: String? = null, val folders: List<Pair<String,String>> = emptyList(), val failure: String? = null
)

class VaultBackupCoordinator(
    private val client: ClientController, private val scope: CoroutineScope,
    private val paths: () -> PrivateFiles?, private val native: () -> MobileEngine?
) {
    private val mutable = MutableStateFlow(BackupForm())
    val form = mutable.asStateFlow()
    private var serial = 0
    private var picker: SessionStamp? = null
    private val key = "vault:backup"
    private fun <T> run(work: suspend (MobileEngine) -> T, failed: (String) -> Unit = {}, done: (T) -> Unit) {
        val turn = serial
        mutable.value = mutable.value.copy(failure=null)
        client.perform(work, key = key, userInitiated = true, onError = { if (serial == turn) failed(it) }) { if (serial == turn) done(it) }
    }

    fun begin(importing: Boolean) {
        clear()
        run({ it.vaultBackupBegin(importing) }) { token ->
            mutable.value = BackupForm(if (importing) "pick" else "loading", token)
            if (!importing) inventory()
        }
    }

    fun inventory() {
        val id = mutable.value.id
        run({ it.vaultBackupInventory(id).decode().objectValue() }) {
            mutable.value = mutable.value.copy(step = "export", view = it)
        }
    }

    fun generate(done: (String) -> Unit) = run({ it.vaultBackupPassword() }, done = done)

    suspend fun processedBytes(id: String): ULong = withContext(Dispatchers.IO) {
        try {
            native()?.vaultBackupStatus(id)?.decode()?.objectValue()?.unsigned("bytes") ?: 0uL
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) { 0uL }
    }

    fun request(items: List<String>, attachments: Boolean, password: String, repeat: String) {
        val id = mutable.value.id
        run({ it.vaultBackupRequest(id,items,attachments,password,repeat).decode().objectValue().required("id") }) {
            mutable.value = mutable.value.copy(step = "waiting", approval = it)
            client.approvals.open(it)
        }
    }

    fun approved() {
        val current = mutable.value
        val approval = current.approval ?: return
        run({ it.approval(approval).decode().objectValue().required("state") }) { state ->
            when (state) {
                "done" -> export(current,approval)
                "awaiting_user","executing" -> Unit
                else -> inventory()
            }
        }
    }

    private fun export(current: BackupForm, approval: String) {
        run({ it.vaultBackupExport(current.id,approval).decode().objectValue() }, failed = { mutable.value=current.copy(step="loading") }) {
            mutable.value = current.copy(step="ready",view=it)
        }
    }

    fun external(): Boolean {
        picker = client.currentStamp() ?: return false
        client.beginExternalInteraction()
        return true
    }

    fun picked(uri: Uri?) {
        val stamp = picker ?: return
        picker = null
        if (uri == null) return
        val files = paths() ?: return
        val account = stamp.account ?: return
        client.deliverExternalResult(key+":pick",stamp) {
            run({
                val job = currentCoroutineContext()[Job]
                files.stage(uri,account,320L*1024*1024) { client.sessionCurrent(stamp) && job?.isActive == true }
            }) { handle ->
                mutable.value.input?.let(files::finishImport)
                mutable.value = mutable.value.copy(input=handle)
            }
        }
    }

    fun inspect(password: String) {
        val current = mutable.value
        val handle = current.input ?: return
        run({ core ->
            val view = core.vaultBackupInspect(current.id,handle,password).decode().objectValue()
            val folders = core.fileFolders().decode().jsonArray.map { it.jsonObject.required("id") to it.jsonObject.required("name") }
            view to folders
        }, failed = { if (it=="INTEGRITY_FAILED") mutable.value=current.copy(failure="INTEGRITY_FAILED") }) { (view,folders) ->
            paths()?.finishImport(handle)
            mutable.value = mutable.value.copy(step = "preview", input = null, view = view, folders = folders)
        }
    }

    fun commit(items: List<UInt>, folder: String) {
        val current = mutable.value
        val prefs = paths()?.backupReceipts ?: return
        val owner = client.currentStamp()?.account ?: return
        prefs.edit().putString(owner,current.id).apply()
        run({ it.vaultBackupCommit(current.id,current.view.required("digest"),items,folder).decode().objectValue() }) {
            prefs.edit().remove(owner).apply()
            mutable.value = current.copy(step="done",view=it)
            client.vault.changed()
            if (it.requiredUnsigned("files") > 0uL) client.library.browse()
        }
    }

    fun previous(): String? = client.currentStamp()?.account?.let { paths()?.backupReceipts?.getString(it,null) }

    fun reconcile() {
        val id=previous() ?: return
        run({ it.vaultBackupStatus(id).decode().objectValue() }) {
            client.currentStamp()?.account?.let { owner -> paths()?.backupReceipts?.edit()?.remove(owner)?.apply() }
            val receipt = it["receipt"] as? JsonObject
            if (receipt == null) { mutable.value=mutable.value.copy(failure="uncommitted"); return@run }
            mutable.value = BackupForm("done",view=receipt)
        }
    }

    fun prepareSave(launchPicker: () -> Unit) {
        val current = mutable.value
        run({ current.output ?: it.vaultBackupOutput(current.id) }) { path ->
            mutable.value = current.copy(output = path)
            if (external()) launchPicker()
        }
    }

    fun destination(uri: Uri?) {
        val stamp = picker
        picker = null
        if (uri == null) return
        val files = paths() ?: return
        val current = mutable.value
        val path = current.output
        val discard = { files.removeIncompleteExport(uri) }
        if (stamp == null || path == null) {
            discard()
            return
        }
        client.deliverExternalResult("$key:save", stamp, discard = discard) {
            run({ core ->
                var written = false
                try {
                    files.exportBackup(current.id,path,uri) { client.sessionCurrent(stamp) }
                    written = true
                } finally {
                    if (!written) discard()
                }
                withContext(NonCancellable) { runCatching { core.vaultBackupCancel(current.id) } }
            }) { mutable.value = current.copy(step="done",output=null) }
        }
    }

    fun clear() {
        serial++
        picker=null
        val current=mutable.value
        mutable.value=BackupForm()
        client.cancelOperation(key)
        client.cancelOperation("external-result:$key:save")
        val core=native()
        val files=paths()
        scope.launch(Dispatchers.IO) {
            current.input?.let { runCatching { files?.finishImport(it) } }
            if(current.id.isNotEmpty()) runCatching { core?.vaultBackupCancel(current.id) }
        }
    }
}
