package net.dmsg.android.core

import kotlinx.coroutines.CancellationException
import kotlinx.serialization.json.*
import net.dmsg.core.VaultItem

data class SecretAttachment(val id: String, val available: Boolean, val details: FileDetails?, val error: String?) {
    val selectable: Boolean get() = available && details?.file?.trashedAt == null && details?.versions?.any { it.current && it.local && it.id == details.file.version } == true
}
data class SecretAttachments(val item: String, val revision: String, val files: List<SecretAttachment>)
data class SecretDependency(val kind: String, val id: String, val label: String, val revision: String, val state: String, val action: String) {
    val key: String get() = kind + ":" + id
}
data class SecretDependencyIndex(val revision: String?, val dependencies: List<SecretDependency>, val digest: String)
data class SecretFieldReference(val item: String, val field: String, val uri: String, val index: SecretDependencyIndex)
data class SecretRotation(val id: String, val account: String, val item: String, val field: String, val previous: String, val next: String?,
    val created: Boolean, val tested: Boolean, val referencesChecked: Boolean, val referencesDigest: String?, val upstreamRevoked: Boolean,
    val evidence: String, val createdAt: ULong, val updatedAt: ULong, val completedAt: ULong?)
data class SecretRotationDetail(val rotation: SecretRotation, val index: SecretDependencyIndex)
data class SecretFinding(val id: String, val rule: String, val severity: String, val subject: String, val revision: String, val field: String?,
    val observedAt: ULong, val evidence: String, val action: String, val dismissed: Boolean)
data class SecretHealth(val findings: List<SecretFinding>, val rotations: List<SecretRotation>, val observedAt: ULong, val subjects: Map<String, VaultItem> = emptyMap())
data class SecretActivityEntry(val id: String, val kind: String, val state: String, val source: String, val error: String?)
data class SecretActivity(val items: List<SecretActivityEntry>, val next: String?)

fun parseSecretAttachments(value: JsonElement): SecretAttachments {
    val row = value.objectValue()
    check(row.requiredFlag("device_local") && row.requiredFlag("attachments_sync_separately")) { "INTEGRITY_FAILED" }
    return SecretAttachments(row.required("item_id"), row.required("revision"), row.rows("files").map {
        val available = it.requiredFlag("available")
        val details = if (available) parseFileDetails(it["details"] ?: error("INTEGRITY_FAILED")) else null
        check(details == null || details.file.id == it.required("id")) { "INTEGRITY_FAILED" }
        SecretAttachment(it.required("id"), available, details, it.text("error"))
    })
}

fun parseSecretDependencies(value: JsonElement): SecretDependencyIndex {
    val row = value.objectValue()
    check(row.requiredFlag("device_local")) { "INTEGRITY_FAILED" }
    return SecretDependencyIndex(row.text("revision"), row.rows("dependencies").map {
        SecretDependency(it.required("kind"), it.required("id"), it.required("label"), it.required("revision"), it.required("state"), it.required("action"))
    }, row.required("digest"))
}

fun parseSecretRotation(value: JsonElement): SecretRotation {
    val row = value.objectValue()
    val reference = row["reference"]?.objectValue() ?: error("INTEGRITY_FAILED")
    return SecretRotation(row.required("id"), reference.required("account_id"), reference.required("item_id"), reference.required("field_id"),
        row.required("previous_revision"), row.text("new_revision"), row.requiredFlag("created"), row.requiredFlag("tested"), row.requiredFlag("references_checked"),
        row.text("references_digest"), row.requiredFlag("upstream_revoked"), row.required("evidence"), row.requiredUnsigned("created_at"), row.requiredUnsigned("updated_at"), row.unsigned("completed_at"))
}

fun parseSecretHealth(value: JsonElement): SecretHealth {
    val row = value.objectValue()
    check(row.requiredFlag("device_local")) { "INTEGRITY_FAILED" }
    return SecretHealth(row.rows("findings").map {
        SecretFinding(it.required("id"), it.required("rule"), it.required("severity"), it.required("subject"), it.required("revision"), it.text("field"),
            it.requiredUnsigned("observed_at"), it.required("evidence"), it.required("action"), it.requiredFlag("dismissed"))
    }, row.rows("rotations").map(::parseSecretRotation), row.requiredUnsigned("observed_at"))
}

fun parseSecretActivity(value: JsonElement): SecretActivity {
    val row = value.objectValue()
    check(row.requiredFlag("device_local")) { "INTEGRITY_FAILED" }
    return SecretActivity(row.rows("items").map {
        SecretActivityEntry(it.required("id"), it.required("type"), it.required("state"), it.required("source"), it.text("error"))
    }, row.text("next"))
}

/** Local observations and owner statements, with the core retaining revision/digest authority. */
class SecretToolsCoordinator(private val client: ClientController) {
    val attachments = ReadStore<SecretAttachments> { it.files.isEmpty() }
    val reference = ReadStore<SecretFieldReference>()
    val health = ReadStore<SecretHealth>()
    val rotation = ReadStore<SecretRotationDetail>()
    val activity = ReadStore<SecretActivity> { it.items.isEmpty() }

    fun attachments(item: String) {
        val request = attachments.begin(attachments.state.value.valueOrNull()?.item == item)
        client.perform({ parseSecretAttachments(it.secretItemAttachments(item).decode()) }, key = "secret:attachments", replace = true,
            onError = { attachments.fail(request, it) }) { attachments.accept(request, it) }
    }

    fun reference(item: String, field: String) {
        val old = reference.state.value.valueOrNull()
        val request = reference.begin(old?.item == item && old.field == field)
        client.perform({ core ->
            val uri = core.secretReference(item, field).decode().objectValue().required("uri")
            SecretFieldReference(item, field, uri, parseSecretDependencies(core.secretReferences(uri).decode()))
        }, key = "secret:reference", replace = true, onError = { reference.fail(request, it) }) { reference.accept(request, it) }
    }

    fun health() {
        val request = health.begin()
        client.perform({ core ->
            val report = parseSecretHealth(core.secretHealth().decode())
            val subjects = mutableMapOf<String, VaultItem>()
            for (id in report.findings.filter { it.field != null }.map { it.subject }.distinct()) {
                try { subjects[id] = core.vaultItem(id, null).item }
                catch (error: CancellationException) { throw error }
                catch (_: Exception) { /* A missing local subject remains visible by ID. */ }
            }
            report.copy(subjects = subjects)
        }, key = "secret:health", replace = true, onError = { health.fail(request, it) }) { health.accept(request, it) }
    }

    fun dismiss(id: String) = client.perform({ it.secretHealthDismiss(id) }, key = "secret:dismiss:" + id) { health() }

    fun createRotation(value: SecretFieldReference) = client.perform({ parseSecretRotation(it.secretRotationCreate(value.uri).decode()) },
        key = "secret:rotation:create:" + value.item + ":" + value.field) { client.navigation.push(Route("secret-rotation", it.id)); health() }

    fun rotation(id: String) {
        val request = rotation.begin(rotation.state.value.valueOrNull()?.rotation?.id == id)
        client.perform({ core ->
            val rotations = core.secretRotations().decode().objectValue()
            check(rotations.requiredFlag("device_local")) { "INTEGRITY_FAILED" }
            val found = rotations.rows("rotations").map(::parseSecretRotation).singleOrNull { it.id == id } ?: error("NOT_FOUND")
            SecretRotationDetail(found, parseSecretDependencies(core.secretRotationDependencies(id).decode()))
        }, key = "secret:rotation", replace = true, quiet = true, onError = { rotation.fail(request, it) }) { rotation.accept(request, it) }
    }

    fun record(value: SecretRotationDetail, step: String) = client.perform({
        it.secretRotationUpdate(value.rotation.id, step, if (step == "references_checked") value.index.digest else null)
    }, key = "secret:rotation:record:" + value.rotation.id, onError = { rotation(value.rotation.id) }) { rotation(value.rotation.id); health() }

    fun activity(more: Boolean = false) {
        val old = activity.state.value.valueOrNull()
        val before = if (more) old?.next ?: return else null
        val request = activity.begin()
        client.perform({ core ->
            val page = parseSecretActivity(core.secretActivity(before, 40u).decode())
            if (before == null) page else page.copy(items = (old!!.items + page.items).distinctBy { it.id })
        }, key = "secret:activity", replace = !more, onError = { activity.fail(request, it) }) { activity.accept(request, it) }
    }

    fun activityAction(id: String, reviewed: Boolean) = client.perform({ core ->
        if (reviewed) core.secretActivityReviewed(id) else core.secretActivityCancel(id)
    }, key = "secret:activity:" + id) { activity(); client.approvals.refresh() }

    fun clear() { attachments.reset(); reference.reset(); health.reset(); rotation.reset(); activity.reset() }
}
