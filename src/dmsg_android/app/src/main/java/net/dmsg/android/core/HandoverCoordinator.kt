package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*

data class HandoverStep(val name: String, val settled: Boolean, val evidence: JsonElement?, val skipped: List<String>) {
    val completed get() = settled && evidence != null
    val left get() = settled && evidence == null
}
data class Handover(val id: String, val side: String, val account: String, val predecessor: String, val successor: String, val steps: List<HandoverStep>, val available: List<String>, val cancelledAt: ULong?, val createdAt: ULong) {
    val complete get() = side == "successor" && steps.all { it.settled }
    fun canRun(step: String) = cancelledAt == null && step in available && steps.any { it.name == step && !it.settled }
    companion object { val STEPS = setOf("authorize", "approval_key", "cosigners", "content", "wallets", "native_addresses", "revoke") }
    val leftBehind get() = steps.filter { it.left }.flatMap { it.skipped.ifEmpty { listOf(it.name) } }
}
data class MovedCopy(val id: String, val account: String?, val device: String, val movedAt: ULong)
data class HandoverRequest(val id: String, val token: String, val device: String, val approvalKey: Boolean, val cosigner: Boolean)
data class HandoverReview(val id: String, val device: String, val role: String, val capabilities: List<String>, val approvalKey: Boolean, val cosigner: Boolean, val expires: ULong, val wallets: List<WalletSummary>)
data class HandoverAddress(val wallet: String, val from: UInt)
fun parseHandovers(value: JsonElement) = value.arrayRows().map { row ->
    Handover(row.required("id"), row.required("side"), row.required("account"), row.required("predecessor"), row.required("successor"), row.rows("steps").map {
        check(it.containsKey("evidence")) { "INTEGRITY_FAILED" }
        HandoverStep(it.required("step"), it.requiredFlag("settled"), it["evidence"]?.takeIf { it != JsonNull }, it.strings("skipped"))
    }, row.strings("available"), row.unsigned("cancelled_at"), row.requiredUnsigned("created_at")).also {
        check(it.side in setOf("predecessor", "successor") && it.steps.map { it.name }.toSet() == Handover.STEPS && it.steps.size == Handover.STEPS.size && it.available.all { step -> it.steps.any { it.name == step && !it.settled } }) { "INTEGRITY_FAILED" }
    }
}
fun parseMovedCopies(value: JsonElement) = value.arrayRows().map { MovedCopy(it.required("id"), it.text("account"), it.required("device_id"), it.requiredUnsigned("moved_at")) }
fun parseHandoverRequest(value: JsonElement) = value.objectValue().let { HandoverRequest(it.required("id"), it.required("token"), it.required("device_id"), it.requiredFlag("approval_key"), it.requiredFlag("cosigner")) }
fun parseHandoverReview(value: JsonElement) = value.objectValue().let { row ->
    HandoverReview(row.required("id"), row.required("device_id"), row.required("role"), row.strings("capabilities"), row.requiredFlag("approval_key"), row.requiredFlag("cosigner"), row.requiredUnsigned("expires_at"), row.rows("wallets").map { WalletSummary(it.required("wallet"), it.text("name").orEmpty()) })
}
class HandoverCoordinator(private val client: ClientController) {
    val listing = ReadStore<List<Handover>> { it.isEmpty() }
    val copies = ReadStore<List<MovedCopy>> { it.isEmpty() }
    val requested = ReadStore<HandoverRequest>()
    val review = ReadStore<HandoverReview>()
    private val predecessorValue = MutableStateFlow("")
    val predecessor = predecessorValue.asStateFlow()
    private val tokenValue = MutableStateFlow("")
    val token = tokenValue.asStateFlow()
    private val confirmationValue = MutableStateFlow("")
    val confirmation = confirmationValue.asStateFlow()
    private val resultsValue = MutableStateFlow<Map<String, JsonObject>>(emptyMap())
    val results = resultsValue.asStateFlow()
    private val copyApprovalValue = MutableStateFlow<String?>(null)
    val copyApproval = copyApprovalValue.asStateFlow()
    fun closeCopyApproval() { copyApprovalValue.value = null }
    private val refreshValue = MutableStateFlow(false)
    val needsFreshRequest = refreshValue.asStateFlow()
    private var reviewedToken: String? = null
    fun editPredecessor(value: String) { client.activity(); predecessorValue.value = value; requested.reset() }
    fun editToken(value: String) { client.activity(); tokenValue.value = value; reviewedToken = null; review.reset(); confirmationValue.value = "" }
    fun editConfirmation(value: String) { client.activity(); confirmationValue.value = value }
    fun load() {
        val read = listing.begin()
        client.perform({ parseHandovers(it.successions().decode()).reversed() }, key = "handover:list", replace = true, quiet = true, onError = { listing.fail(read, it) }) { listing.accept(read, it) }
        loadCopies(); client.accountWorkspace.local(); client.approvals.settle()
    }
    fun loadCopies() {
        val read = copies.begin()
        client.perform({ parseMovedCopies(it.movedCopies().decode()) }, key = "handover:copies", replace = true, quiet = true, onError = { copies.fail(read, it) }) { copies.accept(read, it) }
    }
    fun createRequest() {
        val chosen = predecessorValue.value.trim().lowercase()
        if (!transferHex(chosen) || requested.value() != null) return
        val read = requested.begin()
        client.perform({ parseHandoverRequest(it.successionRequest(chosen).decode()) }, key = "handover:request", quiet = true, onError = { requested.fail(read, it) }) {
            if (predecessorValue.value.trim().lowercase() == chosen) requested.accept(read, it)
        }
    }
    fun checkAuthorization() = client.perform({ it.refreshAccount() }, transition = true) { client.showAccess(AccessPage.WELCOME) }
    fun checkRequest() {
        val token = tokenValue.value.trim()
        if (token.isEmpty() || token.length > 16 * 1024) return
        reviewedToken = null
        val read = review.begin(false)
        client.perform({ parseHandoverReview(it.successionReview(token).decode()) }, key = "handover:review", replace = true, quiet = true, onError = { review.fail(read, it) }) {
            if (token == tokenValue.value.trim()) { reviewedToken = token; review.accept(read, it) }
        }
    }
    fun canApprove(now: ULong): Boolean {
        val value = review.value() ?: return false
        return review.state.value is ReadState.Ready && reviewedToken == tokenValue.value.trim() && value.expires > now && confirmationValue.value.trim().lowercase() == value.device
    }
    fun approve() {
        val value = review.value() ?: return
        if (!canApprove(System.currentTimeMillis().toULong())) return
        val token = reviewedToken ?: return
        client.perform({ it.successionApprove(token, value.device).decode().objectValue() }, key = "handover:approve") {
            if (it["requests"] is JsonArray) { refreshValue.value = true; review.reset(); reviewedToken = null }
            acceptOutcome(value.id, it); load()
        }
    }
    fun run(task: Handover, step: String) {
        if (listing.state.value !is ReadState.Ready || !task.canRun(step)) return
        client.perform({ it.successionRun(task.id, step).decode().objectValue() }, key = "handover:step:" + task.id, onError = { load() }) { acceptOutcome(task.id, it); load() }
    }
    fun skip(task: Handover, step: String) {
        if (listing.state.value !is ReadState.Ready || !task.canRun(step) || step !in setOf("content", "native_addresses")) return
        val items = if (step == "content") listOf("library") else addresses(task.id).map { it.wallet + " #" + it.from }
        if (step == "native_addresses" && resultsValue.value[task.id]?.get("addresses") !is JsonArray) return
        client.perform({ it.successionSkip(task.id, step, items) }, key = "handover:step:" + task.id, onError = { load() }) { load() }
    }
    fun cancel(task: Handover) = client.perform({ it.successionCancel(task.id) }, key = "handover:step:" + task.id, onError = { load() }) { load() }
    fun import(copy: MovedCopy, task: Handover?) {
        val account = client.state.value.session?.account
        if (copy.account != account || task?.let { it.account != account || it.predecessor != copy.device || it.side != "successor" || !it.canRun("content") } == true) return
        client.perform({ it.movedImport(copy.id, task?.id) }, key = "handover:import:" + copy.id, onError = { load() }) {
            client.notice("FILE_SAVED_LOCALLY"); load(); client.library.browse()
        }
    }
    fun approveFromCopy(copy: MovedCopy) = client.perform({ it.movedApproveSuccessor(copy.id).decode().objectValue() }, key = "handover:copy-approve:" + copy.id) {
        acceptOutcome(copy.id, it)
    }
    fun addresses(id: String): List<HandoverAddress> = resultsValue.value[id]?.takeIf { it["addresses"] is JsonArray }?.rows("addresses")?.map { HandoverAddress(it.required("wallet"), it.requiredUInt("from")) }.orEmpty()
    private fun acceptOutcome(id: String, value: JsonObject) {
        resultsValue.value += id to value
        val approval = if (value.text("kind") != null && value.text("digest") != null) value.required("id")
            else (value["requests"] as? JsonArray)?.firstOrNull()?.objectValue()?.required("id")
        if (approval != null) {
            if (client.state.value.unlocked) client.approvals.open(approval)
            else copyApprovalValue.value = approval
        }
    }
    fun clear(retainForms: Boolean = false) {
        listing.reset(); copies.reset(); review.reset(); reviewedToken = null; resultsValue.value = emptyMap()
        if (!retainForms) { requested.reset(); predecessorValue.value = ""; tokenValue.value = ""; confirmationValue.value = ""; copyApprovalValue.value = null; refreshValue.value = false }
    }
}
