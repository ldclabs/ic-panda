package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.core.ReportInput

data class ReceivingPolicy(val mode: String, val allow: List<String>, val capacity: ULong?, val observedAt: ULong, val stale: Boolean)
data class ReceivingDraft(val mode: String = "closed", val allow: List<String> = emptyList()) {
    val valid get() = mode in setOf("closed", "contacts", "free") && allow.size <= 1000 && allow.distinct().size == allow.size && allow.all(::validAccount)
}
fun parseReceivingSnapshot(value: JsonElement): ReceivingPolicy? {
    val root = value.objectValue(); val state = root.required("state")
    if (state == "missing") { check(root["data"] == JsonNull && root["observed_at_ms"] == JsonNull); return null }
    check(state in setOf("absent", "present")) { "INTEGRITY_FAILED" }
    val observed = root.requiredUnsigned("observed_at_ms"); val stale = root.requiredFlag("stale")
    if (state == "absent") { check(root["data"] == JsonNull); return ReceivingPolicy("closed", emptyList(), null, observed, stale) }
    val row = root["data"]!!.objectValue()
    return ReceivingPolicy(row.required("mode"), row.strings("allow"), row.requiredUnsigned("capacity"), observed, stale)
}
data class ReportTarget(val kind: String, val account: String, val resource: String, val sequence: ULong? = null)
data class ReportDraft(val target: ReportTarget, val reason: String = "spam", val note: String = "", val disclose: Boolean = false) {
    val valid get() = reason in setOf("spam", "harassment", "illegal", "other") && note.toByteArray(Charsets.UTF_8).size <= 2000 && validAccount(target.account)
    fun input() = ReportInput(target.kind, target.account, target.resource, target.sequence, reason, note, disclose)
}
data class ContentReport(val id: String, val state: String, val resolution: String?, val createdAt: ULong?)
data class ReportReview(val report: ContentReport, val kind: String, val account: String, val resource: String, val reason: String, val note: String, val disclosedText: String?)
fun parseContentReports(value: JsonElement) = value.arrayRows().map { ContentReport(it.required("id"), it.required("state"), it.text("resolution"), it.unsigned("created_at")) }
fun parseReportReview(value: JsonElement): ReportReview {
    val row = value.objectValue(); val payload = row["payload"]!!.objectValue()
    return ReportReview(ContentReport(row.required("id"), row.required("state"), row.text("resolution"), row.unsigned("created_at")), payload.required("kind"), payload.required("target_account"), payload.required("resource"), payload.required("reason"), payload.required("note"), payload.text("disclosed_text"))
}

/** The local block list and original sealed reports remain authoritative offline. */
class SafetyCoordinator(private val client: ClientController) {
    val blocked = ReadStore<Set<String>> { it.isEmpty() }
    val policy = ReadStore<ReceivingPolicy>()
    val reports = ReadStore<List<ContentReport>> { it.isEmpty() }
    val report = ReadStore<ReportReview>()
    private val submissionErrorsValue = MutableStateFlow<Map<String, String>>(emptyMap())
    val submissionErrors = submissionErrorsValue.asStateFlow()
    private val draftValue = MutableStateFlow<ReceivingDraft?>(null)
    val receiving = draftValue.asStateFlow()
    private val pendingValue = MutableStateFlow<ReceivingDraft?>(null)
    val pending = pendingValue.asStateFlow()
    private val reportDraftValue = MutableStateFlow<ReportDraft?>(null)
    val reportDraft = reportDraftValue.asStateFlow()
    private val blockErrorValue = MutableStateFlow<String?>(null)
    val blockSyncError = blockErrorValue.asStateFlow()
    private var dirty = false
    private var preparedReport: String? = null
    fun loadBlocks(refresh: Boolean = false) {
        val request = blocked.begin()
        client.perform({ it.blockedAccounts().toSet() }, key = "safety:blocks", replace = true, quiet = true, onError = { blocked.fail(request, it) }) {
            blocked.accept(request, it); if (refresh) syncBlocks()
        }
    }
    fun syncBlocks() = client.perform({ it.syncBlockedAccounts() }, key = "safety:block-sync", quiet = true, onError = { blockErrorValue.value = it; loadBlocks() }) { blockErrorValue.value = null; loadBlocks() }
    fun block(account: String, value: Boolean) = client.perform({ it.setBlockedAccountLocal(account, value) }, key = "safety:block:$account", onError = { loadBlocks() }) { loadBlocks(); syncBlocks() }
    fun openBlock(account: String) { client.navigation.push(Route("blocked-accounts", id = account)) }

    fun editReceiving(value: ReceivingDraft) { client.activity(); dirty = true; draftValue.value = value }
    private fun apply(request: Long, next: ReceivingPolicy) {
        policy.accept(request, next)
        if (!dirty && pendingValue.value == null) draftValue.value = ReceivingDraft(next.mode, next.allow)
    }
    fun loadPolicy(force: Boolean = false) {
        val request = policy.begin()
        client.perform({ parseReceivingSnapshot(it.cachedDisplay("inbox-policy", "").decode()) }, key = "safety:policy:local", replace = true, quiet = true, onError = { policy.fail(request, it) }) { cached ->
            cached?.let { apply(request, it) }
            client.perform({ core -> parseReceivingSnapshot(client.operation { core.refreshDisplay("inbox-policy", "", force, it) }.decode()) ?: error("INTEGRITY_FAILED") }, key = "safety:policy:remote", replace = true, quiet = true, onError = { policy.fail(request, it) }) { apply(request, it) }
        }
    }
    fun savePolicy() {
        val draft = pendingValue.value ?: draftValue.value ?: return
        if (!draft.valid || policy.value() == null) return
        pendingValue.value = draft
        client.perform({ core -> client.operation { core.configureInbox(draft.mode, draft.allow, it) } }, key = "safety:policy:save", onError = {
            if (it in setOf("INVALID_INPUT", "FORBIDDEN", "QUOTA_EXCEEDED", "VERSION_CONFLICT", "IDEMPOTENCY_CONFLICT")) pendingValue.value = null
        }) { pendingValue.value = null; draftValue.value = draft; dirty = false; loadPolicy(true) }
    }
    fun openReport(target: ReportTarget) {
        if (reportDraftValue.value?.target != target) { reportDraftValue.value = ReportDraft(target); preparedReport = null; report.reset() }
        preparedReport?.let(::reviewReport)
        client.navigation.push(Route("report-compose", id = target.account, field = target.kind, channel = target.resource, sequence = target.sequence))
    }
    fun editReport(value: ReportDraft) { if (report.value() != null) return; client.activity(); reportDraftValue.value = value }
    fun prepareReport() {
        val form = reportDraftValue.value ?: return
        if (!form.valid || report.value() != null) return
        preparedReport?.let { reviewReport(it); return }
        val request = report.begin(false)
        client.perform({ parseReportReview(it.prepareReport(form.input()).decode()) }, key = "safety:report:prepare", onError = { report.fail(request, it) }) { preparedReport = it.report.id; report.accept(request, it); loadReports(false) }
    }
    fun reviewReport(id: String) {
        val request = report.begin(report.value()?.report?.id == id)
        client.perform({ parseReportReview(it.reportDetails(id).decode()) }, key = "safety:report:detail", replace = true, quiet = true, onError = { report.fail(request, it) }) { report.accept(request, it) }
    }
    fun submitReport(id: String) {
        submissionErrorsValue.value -= id
        client.perform({ core -> client.operation { core.submitReport(id, it) } }, key = "safety:report:submit:$id", quiet = true,
            onError = { submissionErrorsValue.value += id to it; reviewReport(id); loadReports(false) }) { reviewReport(id); loadReports(false) }
    }
    fun loadReports(refresh: Boolean = true) {
        val request = reports.begin()
        client.perform({ parseContentReports(it.reportHistory(false).decode()) }, key = "safety:reports:local", replace = true, quiet = true, onError = { reports.fail(request, it) }) { local ->
            reports.accept(request, local.sortedByDescending { it.createdAt })
            if (refresh) client.perform({ parseContentReports(it.reportHistory(true).decode()) }, key = "safety:reports:remote", replace = true, quiet = true, onError = { reports.fail(request, it) }) { remote ->
                val ids = remote.map { it.id }.toSet(); reports.accept(request, (remote + local.filterNot { it.id in ids }).sortedByDescending { it.createdAt })
            }
        }
    }
    fun clear(retainForms: Boolean = false) {
        blocked.reset(); policy.reset(); reports.reset(); report.reset(); blockErrorValue.value = null; submissionErrorsValue.value = emptyMap()
        if (!retainForms) { dirty = false; draftValue.value = null; pendingValue.value = null; reportDraftValue.value = null; preparedReport = null }
    }
}
