package net.dmsg.android.core

import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.JsonNull
import net.dmsg.android.platform.*
import net.dmsg.core.*

data class SystemExportStatus(val credentials: ExportHeader?, val files: ExportHeader?, val requiresBiometry: Boolean, val biometryAvailable: Boolean)
data class SelectedLogin(val id: String, val revision: String, val title: String)

class SystemExportsCoordinator(private val client: ClientController, private val context: Context, private val scope: CoroutineScope, private val paths: () -> PrivateFiles?) {
    val status = ReadStore<SystemExportStatus>()
    val logins = ReadStore<List<VaultItem>> { it.isEmpty() }
    val fileChoices = ReadStore<FileCoordinator.Listing>()
    private val chosenValue = MutableStateFlow<List<SelectedLogin>>(emptyList())
    val chosen = chosenValue.asStateFlow()
    private val filesValue = MutableStateFlow<List<LibraryFile>>(emptyList())
    val chosenFiles = filesValue.asStateFlow()
    private val onlyValue = MutableStateFlow(false)
    val only = onlyValue.asStateFlow()
    private val failureValue = MutableStateFlow<String?>(null)
    val failure = failureValue.asStateFlow()
    @Volatile private var generation = 0L
    private var timer: Job? = null
    var more = false; private set
    private var query = ""
    fun start() {
        if (timer?.isActive == true) return
        timer = scope.launch { while (isActive) { refreshCredentials(); delay(60_000) } }
    }
    fun load() {
        val request = status.begin()
        val stamp = client.currentStamp() ?: return
        client.perform({ core ->
            val hardware = core.systemExportPolicy().decode().objectValue()
            val selected = core.autofillSelection()
            val credentials = systemExports(context, ExportPurpose.credentials).header(false)?.takeIf { it.account == SystemExportStore.digest(checkNotNull(stamp.account).toByteArray()) }
            val files = systemExports(context, ExportPurpose.files).header(false)?.takeIf { it.account == SystemExportStore.digest(checkNotNull(stamp.account).toByteArray()) }
            Triple(SystemExportStatus(credentials, files, hardware.requiredFlag("biometry_only"), hardware.requiredFlag("biometry_available")), selected.items.map { SelectedLogin(it.id, it.revision, it.title) }, selected.biometryOnly || hardware.requiredFlag("biometry_only"))
        }, key = "exports:status", replace = true, quiet = true, onError = { status.fail(request, it) }) {
            status.accept(request, it.first); chosenValue.value = it.second; onlyValue.value = it.third
        }
    }
    fun browseLogins(search: String = query, next: Boolean = false) {
        val previous = if (next && search == query) logins.value().orEmpty() else emptyList()
        query = search; val request = logins.begin(next)
        client.perform({ it.vaultPage(search, "login", false, previous.size.toUInt(), 60u) }, key = "exports:logins", replace = true, quiet = true, onError = { logins.fail(request, it) }) {
            more = it.hasMore; logins.accept(request, (previous + it.items.filter { item -> !item.reserved }).distinctBy { item -> item.id })
        }
    }
    fun toggle(item: VaultItem) {
        client.activity()
        chosenValue.value = if (chosenValue.value.any { it.id == item.id }) chosenValue.value.filterNot { it.id == item.id }
            else if (chosenValue.value.size < 50 && !item.reserved) chosenValue.value + SelectedLogin(item.id, item.revision, item.title) else chosenValue.value
    }
    fun removeLogin(id: String) { client.activity(); chosenValue.value = chosenValue.value.filterNot { it.id == id } }
    fun only(value: Boolean) { client.activity(); onlyValue.value = value || status.value()?.requiresBiometry == true }
    fun browseFiles(folder: String = "root") {
        val request = fileChoices.begin(false)
        client.perform({ parseFiles(it.files(folder).json).let { FileCoordinator.Listing(folder, "", false, it.first, it.second) } }, key = "exports:file-list", replace = true, quiet = true, onError = { fileChoices.fail(request, it) }) { fileChoices.accept(request, it) }
    }
    fun toggle(file: LibraryFile) {
        client.activity()
        filesValue.value = if (filesValue.value.any { it.id == file.id }) filesValue.value.filterNot { it.id == file.id }
            else if (filesValue.value.size < 10 && file.size <= SystemExportStore.MAX_FILE.toULong() && filesValue.value.sumOf { it.size } + file.size <= 200uL * SystemExportStore.PART.toULong()) filesValue.value + file else filesValue.value
    }
    fun updateCredentials() {
        if (chosenValue.value.isEmpty()) { remove(ExportPurpose.credentials); return }
        val selection = chosenValue.value.toList(); val only = onlyValue.value
        val stamp = client.currentStamp() ?: return
        val at = ++generation; client.cancelOperation("exports:refresh"); failureValue.value = null
        client.perform({ core ->
            val refresh = core.exportAutofill(selection.map { CredentialSelection(it.id, it.revision) }, only).decode()
            publishCredentials(core, stamp, at, refresh)
        }, key = "exports:credentials", userInitiated = true, quiet = true, onError = { failureValue.value = it }) { load() }
    }
    private suspend fun publishCredentials(core: MobileEngine, stamp: SessionStamp, at: Long, value: kotlinx.serialization.json.JsonElement) {
        if (value == JsonNull) return
        val row = value.objectValue()
        if (!row.requiredFlag("changed")) return
        val job = currentCoroutineContext()[Job]
        fun valid() = generation == at && client.accepts(stamp) && job?.isActive != false
        check(valid()) { "CANCELLED" }
        val store = systemExports(context, ExportPurpose.credentials)
        val inputs = row.rows("entries").map { ExportCredentialInput(it.required("title"), it.required("username"), it.required("password"), it.required("url")) }
        if (inputs.isEmpty()) { check(valid()) { "CANCELLED" }; store.invalidate(checkNotNull(stamp.account)) }
        else {
            val only = row.requiredFlag("biometry_only") || core.systemExportPolicy().decode().objectValue().requiredFlag("biometry_only")
            val prepared = store.prepareCredentials(inputs, checkNotNull(stamp.account), only, ::valid)
            try { store.publish(prepared, ::valid) } finally { store.discard(prepared) }
        }
        check(valid()) { "CANCELLED" }; core.autofillPublished(row.required("digest"))
    }
    private fun refreshCredentials() {
        val stamp = client.currentStamp() ?: return
        if (!client.accepts(stamp) || client.isWorking("exports:credentials") || client.isWorking("exports:remove")) return
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
            val force = header == null || header.expires <= System.currentTimeMillis() + SystemExportStore.DAY / 2
            publishCredentials(core, stamp, at, core.refreshAutofill(force).decode())
        }, key = "exports:refresh", quiet = true, onError = { failureValue.value = it }) {}
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
    fun remove(purpose: ExportPurpose) {
        generation++; client.cancelOperation("exports:refresh"); client.cancelOperation("exports:credentials"); client.cancelOperation("exports:files")
        if (purpose == ExportPurpose.files) ExportFileAccess.lock()
        failureValue.value = null
        client.perform({ core ->
            if (purpose == ExportPurpose.credentials) core.forgetAutofill()
            systemExports(context, purpose).invalidate()
            if (purpose == ExportPurpose.files) ExportFileAccess.changed(context)
        }, key = "exports:remove", userInitiated = true, quiet = true, onError = { failureValue.value = it }) {
            if (purpose == ExportPurpose.credentials) chosenValue.value = emptyList() else filesValue.value = emptyList()
            load()
        }
    }
    suspend fun invalidate() {
        withContext(Dispatchers.Main.immediate) { clear(); AutofillRequests.clear() }
        ExportPurpose.entries.forEach { systemExports(context, it).invalidate() }
        ExportFileAccess.changed(context)
    }
    fun clear() {
        generation++; timer?.cancel(); timer = null; ExportFileAccess.lock()
        status.reset(); logins.reset(); fileChoices.reset(); chosenValue.value = emptyList(); filesValue.value = emptyList(); failureValue.value = null; onlyValue.value = false; more = false; query = ""
    }
}
