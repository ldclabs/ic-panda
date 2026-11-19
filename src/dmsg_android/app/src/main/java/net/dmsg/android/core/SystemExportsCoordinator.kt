package net.dmsg.android.core

import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.android.platform.*
import net.dmsg.core.*

data class SystemExportStatus(val credentials: ExportHeader?, val files: ExportHeader?, val requiresBiometry: Boolean, val biometryAvailable: Boolean)

class SystemExportsCoordinator(private val client: ClientController, private val context: Context, private val scope: CoroutineScope, private val paths: () -> PrivateFiles?) {
    val status = ReadStore<SystemExportStatus>()
    val fileChoices = ReadStore<FileCoordinator.Listing>()
    private val filesValue = MutableStateFlow<List<LibraryFile>>(emptyList())
    val chosenFiles = filesValue.asStateFlow()
    private val onlyValue = MutableStateFlow(false)
    val only = onlyValue.asStateFlow()
    private val failureValue = MutableStateFlow<String?>(null)
    val failure = failureValue.asStateFlow()
    private val autoFillFailureValue = MutableStateFlow<String?>(null)
    val autoFillFailure = autoFillFailureValue.asStateFlow()
    @Volatile private var generation = 0L
    private var timer: Job? = null
    fun start() {
        if (timer?.isActive == true) return
        timer = scope.launch { while (isActive) { refreshCredentials(); delay(60_000) } }
    }
    fun load() {
        val request = status.begin()
        val stamp = client.currentStamp() ?: return
        client.perform({ core ->
            val hardware = core.systemExportPolicy().decode().objectValue()
            val credentials = systemExports(context, ExportPurpose.credentials).header(false)?.takeIf { it.account == SystemExportStore.digest(checkNotNull(stamp.account).toByteArray()) }
            val files = systemExports(context, ExportPurpose.files).header(false)?.takeIf { it.account == SystemExportStore.digest(checkNotNull(stamp.account).toByteArray()) }
            Pair(SystemExportStatus(credentials, files, hardware.requiredFlag("biometry_only"), hardware.requiredFlag("biometry_available")), hardware.requiredFlag("autofill_biometry_only") || hardware.requiredFlag("biometry_only"))
        }, key = "exports:status", replace = true, quiet = true, onError = { status.fail(request, it) }) {
            status.accept(request, it.first); onlyValue.value = it.second
        }
    }
    fun browseFiles(folder: String = "root") {
        val request = fileChoices.begin(false)
        client.perform({ parseFiles(it.files(folder).json).let { FileCoordinator.Listing(folder, "", false, it.first, it.second) } }, key = "exports:file-list", replace = true, quiet = true, onError = { fileChoices.fail(request, it) }) { fileChoices.accept(request, it) }
    }
    fun toggle(file: LibraryFile) {
        client.activity()
        filesValue.value = if (filesValue.value.any { it.id == file.id }) filesValue.value.filterNot { it.id == file.id }
            else if (filesValue.value.size < 10 && file.size <= SystemExportStore.MAX_FILE.toULong() && filesValue.value.sumOf { it.size } + file.size <= 200uL * SystemExportStore.PART.toULong()) filesValue.value + file else filesValue.value
    }
    fun setFileBiometry(value: Boolean) {
        client.activity()
        onlyValue.value = value || status.value()?.requiresBiometry == true
    }

    fun setCredentialBiometry(value: Boolean) {
        client.perform({ it.setAutofillBiometry(value) }, key = "exports:policy", userInitiated = true, quiet = true, onError = { autoFillFailureValue.value = it }) {
            onlyValue.value = value; refreshCredentials()
        }
    }

    fun refreshCredentials() {
        val stamp = client.currentStamp() ?: return
        if (!client.accepts(stamp) || client.isWorking("exports:policy")) return
        val at = generation
        client.perform({ core ->
            val owner = SystemExportStore.digest(checkNotNull(stamp.account).toByteArray())
            // A new login can select a workspace without the explicit switch UI.
            // Reconcile both independent purposes before publishing for that owner.
            for (purpose in ExportPurpose.entries) {
                check(at == generation && client.accepts(stamp)) { "CANCELLED" }
                val copy = systemExports(context, purpose)
                copy.header(false)?.takeIf { it.account != owner }?.let { copy.invalidate() }
            }
            val store = systemExports(context, ExportPurpose.credentials)
            val header = store.header(false)
            val required = core.systemExportPolicy().decode().objectValue().requiredFlag("biometry_only")
            val row = core.autofillCatalog(header?.digest.orEmpty(), required).decode().objectValue()
            if (!row.requiredFlag("changed")) return@perform header
            val job = currentCoroutineContext()[Job]
            fun valid() = at == generation && client.accepts(stamp) && job?.isActive != false
            check(valid()) { "CANCELLED" }
            val inputs = row.rows("entries").map {
                ExportCredentialInput(it.required("title"), it.required("username"), it.required("password"),
                    AutofillTrust.host(it.required("url")).orEmpty(), it.required("id"))
            }
            try {
                val prepared = store.prepareCredentials(inputs, checkNotNull(stamp.account), row.requiredFlag("biometry_only"), row.required("digest"), ::valid)
                try { store.publish(prepared, ::valid); prepared.opened.header } finally { store.discard(prepared) }
            } catch (failure: Exception) {
                if (valid() && header != null) store.withdrawCredentials(header)
                throw failure
            }
        }, key = "exports:refresh", replace = true, quiet = true, onError = { autoFillFailureValue.value = it }) { header ->
            autoFillFailureValue.value = null
            status.value()?.let { status.accept(status.begin(), it.copy(credentials = header)) }
        }
    }

    fun updateFiles() {
        val selection = filesValue.value.toList(); if (selection.isEmpty()) return
        val stamp = client.currentStamp() ?: return; val privateFiles = paths() ?: return
        val at = generation; val only = onlyValue.value; failureValue.value = null
        client.perform({ core ->
            val job = currentCoroutineContext()[Job]
            fun valid() = generation == at && client.accepts(stamp) && job?.isActive != false
            val nonces = mutableListOf<String>()
            try {
                val inputs = selection.map { file ->
                    check(valid()) { "CANCELLED" }
                    val detail = parseFileDetails(core.fileDetails(file.id).decode())
                    check(detail.file.trashedAt == null && detail.versions.any { it.id == file.version && it.local }) { "NOT_LOCAL" }
                    val opened = client.operation { core.openFile(file.id, file.version, it) }.decode().objectValue()
                    nonces += opened.required("nonce")
                    check(opened.required("file") == file.id && opened.required("version") == file.version) { "INTEGRITY_FAILED" }
                    ExportFileInput(privateFiles.workspaceFile(opened.required("path")), file.name, file.mime, file.size.toLong(), file.sha256)
                }
                val store = systemExports(context, ExportPurpose.files)
                val required = core.systemExportPolicy().decode().objectValue().requiredFlag("biometry_only")
                val prepared = store.prepareFiles(inputs, checkNotNull(stamp.account), only || required, ::valid)
                try { store.publish(prepared, ::valid); ExportFileAccess.lock(); ExportFileAccess.changed(context) } finally { store.discard(prepared) }
            } finally { withContext(NonCancellable) { nonces.forEach { runCatching { core.closeFile(it) } } } }
        }, key = "exports:files", userInitiated = true, quiet = true, onError = { failureValue.value = it }) { load() }
    }
    fun removeFiles() {
        client.cancelOperation("exports:files")
        ExportFileAccess.lock()
        failureValue.value = null
        client.perform({
            systemExports(context, ExportPurpose.files).invalidate()
            ExportFileAccess.changed(context)
        }, key = "exports:remove", userInitiated = true, quiet = true, onError = { failureValue.value = it }) {
            filesValue.value = emptyList()
            load()
        }
    }
    suspend fun invalidate(account: String? = null) {
        withContext(Dispatchers.Main.immediate) { clear(); AutofillRequests.clear() }
        withContext(Dispatchers.IO) {
            ExportPurpose.entries.forEach { systemExports(context, it).invalidate(account) }
            ExportFileAccess.changed(context)
        }
    }
    fun clear() {
        generation++; timer?.cancel(); timer = null; ExportFileAccess.lock()
        status.reset(); fileChoices.reset(); filesValue.value = emptyList(); failureValue.value = null; autoFillFailureValue.value = null; onlyValue.value = false
    }
}
