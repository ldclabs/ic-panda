package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.core.*

data class ApprovalDetail(val id: String, val digest: String, val kind: String, val state: String, val source: String, val risk: String, val expiresAt: ULong, val supported: Boolean, val detail: JsonElement, val error: String?, val secret: SecretReview? = null, val record: JsonObject = JsonObject(emptyMap()))

/** The reviewed statement can be device-local or wrapped by a browser request. */
internal fun ApprovalDetail.statement(): JsonObject? {
    val body = detail as? JsonObject ?: return null
    val content = body["content"] as? JsonObject
    return when {
        kind == "attest" -> body["statement"] as? JsonObject
        kind == "connect" && body.text("method") == "signDocument" -> content?.get("statement") as? JsonObject
        kind == "connect" && body.text("method") == "signAction" -> content
        else -> null
    }
}
internal fun ApprovalDetail.browserLogin(): JsonObject? {
    val body = detail as? JsonObject ?: return null
    val content = body["content"] as? JsonObject ?: return null
    return body.takeIf {
        kind == "connect" && body.text("method") == "authenticate" && !body.text("origin").isNullOrBlank() &&
            !content.text("purpose").isNullOrBlank() && !content.text("receiver").isNullOrBlank() && content.unsigned("expires_at") != null
    }
}

internal enum class ApprovalFileCheck { Checked, ChangedOrUnreadable, DigestOnly }

internal fun ApprovalDetail.fileCheck(): ApprovalFileCheck? {
    val content = statement()?.get("content") as? JsonObject ?: return null
    if (content.text("sha256").isNullOrEmpty()) return null
    val body = detail as JsonObject
    return when {
        body.optionalFlag("file_checked") == true -> ApprovalFileCheck.Checked
        body["file"] is JsonObject -> ApprovalFileCheck.ChangedOrUnreadable
        else -> ApprovalFileCheck.DigestOnly
    }
}

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
        client.perform({ parseApprovals(it.approvals().json) }, key = "approval:list", replace = true, quiet = true,
            onError = { listing.fail(request, it) }) { listing.accept(request, it) }
        selected?.let(::select)
        settle()
    }
    fun refreshRemote() {
        if (client.state.value.session?.accountKind != "cloud") { refresh(); return }
        val cached = listing.begin()
        client.perform({ parseApprovals(it.approvals().json) }, key = "approval:list", replace = true, quiet = true,
            onError = { listing.fail(cached, it) }) { values ->
            if (listing.accept(cached, values)) {
                val request = listing.begin()
                client.perform({ core -> client.operation { core.syncConnect(it) }; parseApprovals(core.approvals().json) },
                    key = "approval:remote", replace = true, quiet = true, onError = { listing.fail(request, it) }) { listing.accept(request, it) }
            }
        }
        settle()
    }
    fun reconcile(value: ApprovalDetail) = client.perform({ core ->
        client.operation { core.reconcile(it) }
        parseApproval(core.approval(value.id).decode()).also { check(it.digest == value.digest) { "VERSION_CONFLICT" } }
    }, key = "approval:reconcile:" + value.id, onError = { if (selected == value.id) select(value.id) }) {
        if (selected == value.id) { val request = detail.begin(); detail.accept(request, it) }
        refresh()
    }

    fun select(id: String) {
        val request = detail.begin(keep = selected == id); selected = id
        client.perform({ parseApproval(it.approval(id).decode()) }, key = "approval:detail", replace = true, quiet = true,
            onError = { detail.fail(request, it) }) { detail.accept(request, it) }
    }
    fun leave(id: String) { if (selected == id) { selected = null; detail.reset() } }
    fun open(id: String) { client.navigation.push(Route("approvals", id)) }
    fun request(key: String, create: suspend (MobileEngine) -> ObjectSnapshot) {
        mutableReceipts.value[key]?.let { open(it); return }
        client.perform({ create(it).decode().objectValue().required("id") }, key = "approval:request:" + key) { id ->
            mutableReceipts.value += key to id; open(id)
        }
    }
    fun decide(value: ApprovalDetail, approved: Boolean) {
        client.perform({ core -> parseApproval(client.operation { core.decide(value.id, value.digest, approved, it) }.decode()) },
            key = "approval:decide:" + value.id, onError = { refresh() }) { result ->
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
