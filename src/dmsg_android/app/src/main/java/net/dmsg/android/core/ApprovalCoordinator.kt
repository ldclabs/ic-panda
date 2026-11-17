package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.core.*

data class ApprovalDetail(val id: String, val digest: String, val kind: String, val state: String, val source: String, val risk: String, val expiresAt: ULong, val supported: Boolean, val detail: JsonElement, val error: String?, val secret: SecretReview? = null, val record: JsonObject = JsonObject(emptyMap()))
fun parseApproval(value: JsonElement): ApprovalDetail {
    val row = value.objectValue()
    val kind = row.required("type")
    val detail = row["detail"] ?: JsonNull
    val secret = if (kind in setOf("secret_share", "secret_package_import") && detail != JsonNull) parseSecretReview(kind, detail) else null
    check(row.required("state") != "awaiting_user" || detail != JsonNull) { "INTEGRITY_FAILED" }
    return ApprovalDetail(row.required("id"), row.required("digest"), kind, row.required("state"), row.required("source"), row.required("risk"), row.requiredUnsigned("expires_at"), row.requiredFlag("mobile_supported"), detail, row.text("error"), secret, row)
}
class ApprovalCoordinator(private val client: ClientController) {
    val listing = ReadStore<List<Approval>> { it.isEmpty() }
    val detail = ReadStore<ApprovalDetail>()
    private val mutableReceipts = MutableStateFlow<Map<String, String>>(emptyMap())
    val receipts = mutableReceipts.asStateFlow()
    private var selected: String? = null
    fun refresh() {
        val request = listing.begin()
        client.perform({ parseApprovals(it.approvals().json) }, key = "approval:list", replace = true,
            onError = { listing.fail(request, it) }) { listing.accept(request, it) }
        settle()
    }
    fun refreshRemote() {
        refresh()
        client.perform({ core -> client.operation { core.syncConnect(it) }; parseApprovals(core.approvals().json) }, key = "approval:remote", replace = true, quiet = true, onError = {
            val request = listing.begin(); listing.fail(request, it)
        }) { values -> val request = listing.begin(); listing.accept(request, values) }
    }
    fun reconcile(value: ApprovalDetail) = client.perform({ core ->
        client.operation { core.reconcile(it) }
        parseApproval(core.approval(value.id).decode()).also { check(it.digest == value.digest) { "VERSION_CONFLICT" } }
    }, key = "approval:reconcile:" + value.id, onError = { select(value.id) }) {
        if (selected == value.id) { val request = detail.begin(); detail.accept(request, it) }
        refresh()
    }

    fun select(id: String) {
        val request = detail.begin(keep = selected == id); selected = id
        client.perform({ parseApproval(it.approval(id).decode()) }, key = "approval:detail", replace = true,
            onError = { detail.fail(request, it) }) { detail.accept(request, it) }
    }
    fun open(id: String) { client.navigation.push(Route("approvals", id)) }
    fun request(key: String, create: suspend (MobileEngine) -> ObjectSnapshot) {
        mutableReceipts.value[key]?.let { open(it); return }
        client.perform({ create(it).decode().objectValue().required("id") }, key = "approval:request:" + key) { id ->
            mutableReceipts.value += key to id; open(id)
        }
    }
    fun decide(value: ApprovalDetail, approved: Boolean) {
        client.perform({ core -> parseApproval(client.operation { core.decide(value.id, value.digest, approved, it) }.decode()) },
            key = "approval:decide:" + value.id, onError = { select(value.id); refresh() }) { result ->
            if (selected == value.id) { val request = detail.begin(); detail.accept(request, result) }
            refresh()
        }
    }
    fun settle() {
        val expected = mutableReceipts.value
        if (expected.isEmpty()) return
        client.perform({ core -> expected.filterValues { id ->
            runCatching { core.approval(id).decode().objectValue().required("state") }.getOrNull() in finalStates
        } }, key = "approval:receipts", replace = true) { finished ->
            mutableReceipts.value = mutableReceipts.value.filterNot { (key, id) -> finished[key] == id }
        }
    }
    fun clear(retainReceipts: Boolean = false) { listing.reset(); detail.reset(); selected = null; if (!retainReceipts) mutableReceipts.value = emptyMap() }
    private companion object { val finalStates = setOf("done", "rejected", "cancelled", "expired", "failed", "elsewhere") }
}
