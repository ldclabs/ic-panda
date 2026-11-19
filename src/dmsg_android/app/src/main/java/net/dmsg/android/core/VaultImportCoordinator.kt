package net.dmsg.android.core

import android.net.Uri
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.android.platform.PrivateFiles

data class VaultImportItem(val index: UInt, val kind: String, val title: String, val username: String, val duplicate: Boolean)
data class VaultImportPreview(val format: String, val digest: String, val skipped: ULong, val incomplete: ULong, val items: List<VaultImportItem>)
data class VaultImportForm(val handle: String, val selected: Set<UInt> = emptySet(), val pending: List<UInt>? = null, val query: String = "",
    val completed: List<Pair<String, String>>? = null)

fun parseVaultImport(value: JsonElement): VaultImportPreview {
    val row = value.objectValue()
    return VaultImportPreview(row.required("format"), row.required("digest"), row.requiredUnsigned("skipped"), row.requiredUnsigned("incomplete"), row.rows("items").map {
        val index = it.requiredUnsigned("index")
        check(index <= UInt.MAX_VALUE.toULong()) { "INTEGRITY_FAILED" }
        VaultImportItem(index.toUInt(), it.required("type"), it.required("title"), it.required("username"), it.requiredFlag("duplicate"))
    }.also { check(it.map { row -> row.index }.distinct().size == it.size) { "INTEGRITY_FAILED" } })
}

/** The original staged export and selection stay available until a confirmed
 * receipt; retrying an uncertain commit cannot create another import identity. */
class VaultImportCoordinator(private val client: ClientController, private val scope: CoroutineScope, private val paths: () -> PrivateFiles?) {
    val preview = ReadStore<VaultImportPreview>()
    private val mutable = MutableStateFlow<VaultImportForm?>(null)
    val form = mutable.asStateFlow()
    private var picker: SessionStamp? = null

    fun begin(): Boolean {
        if (mutable.value?.pending != null || client.isWorking("vault:import-preview") || client.isWorking("vault:import-commit")) return false
        picker = client.currentStamp()?.takeIf { it.account != null } ?: return false
        client.beginExternalInteraction()
        return true
    }

    fun result(uri: Uri?) {
        val stamp = picker ?: return
        picker = null
        if (uri != null) client.deliverExternalResult("vault-import", stamp) { stage(uri, stamp) }
    }

    private fun stage(uri: Uri, stamp: SessionStamp) {
        val files = paths() ?: return
        val owner = stamp.account ?: return
        clear()
        val request = preview.begin(false)
        client.perform({ core ->
            val job = currentCoroutineContext()[Job]
            val handle = files.stage(uri, owner, 64L * 1024 * 1024) { client.sessionCurrent(stamp) && job?.isActive == true }
            var retained = false
            try {
                withContext(Dispatchers.Main.immediate) {
                    check(client.accepts(stamp)) { "LOCKED" }
                    mutable.value = VaultImportForm(handle)
                    retained = true
                }
                parseVaultImport(client.operation { core.vaultImportPreview(handle, it) }.decode())
            } finally { if (!retained) files.finishImport(handle) }
        }, key = "vault:import-preview", replace = true, onError = { preview.fail(request, it) }) { preview.accept(request, it) }
    }

    fun retry() {
        val current = mutable.value ?: return
        if (current.pending != null) { commit(); return }
        val request = preview.begin()
        client.perform({ core -> parseVaultImport(client.operation { core.vaultImportPreview(current.handle, it) }.decode()) }, key = "vault:import-preview", replace = true,
            onError = { preview.fail(request, it) }) { preview.accept(request, it) }
    }

    fun edit(change: (VaultImportForm) -> VaultImportForm) {
        client.activity()
        val current = mutable.value ?: return
        if (current.pending == null && current.completed == null && !client.isWorking("vault:import-commit")) mutable.value = change(current)
    }

    fun commit() {
        val current = mutable.value ?: return
        val value = preview.value() ?: return
        val picks = current.pending ?: current.selected.sorted()
        if (picks.isEmpty() || picks.size > 1000 || client.isWorking("vault:import-commit")) return
        mutable.value = current.copy(pending = picks)
        client.perform({ core ->
            val result = client.operation { core.vaultImportSelected(current.handle, value.digest, picks, it) }.decode()
            val ids = (result as? JsonArray)?.map { (it as? JsonPrimitive)?.takeIf { it.isString }?.content ?: error("INTEGRITY_FAILED") } ?: error("INTEGRITY_FAILED")
            check(ids.size == picks.size) { "INTEGRITY_FAILED" }
            ids.zip(picks.map { index -> value.items.single { it.index == index }.title })
        }, key = "vault:import-commit", onError = { code ->
            if (code in setOf("VERSION_CONFLICT", "INVALID_INPUT", "QUOTA_EXCEEDED", "FORBIDDEN", "NOT_FOUND")) mutable.value = current.copy(pending = null)
        }) { result ->
            mutable.value = current.copy(selected = emptySet(), pending = null, completed = result)
            preview.reset(); cleanup(current.handle); client.vault.changed()
        }
    }

    fun cancelPreview() { client.cancelOperation("vault:import-preview"); preview.reset() }
    fun clear(retain: Boolean = false) {
        picker = null
        if (!retain) { mutable.value?.let { cleanup(it.handle) }; mutable.value = null; preview.reset() }
    }
    private fun cleanup(handle: String) {
        val files = paths() ?: return
        scope.launch(Dispatchers.IO) { runCatching { files.finishImport(handle) } }
    }
}
