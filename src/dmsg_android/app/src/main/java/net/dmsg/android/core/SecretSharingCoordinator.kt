package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.core.*
import java.util.UUID

data class SnapshotChannel(val id: String, val name: String, val epoch: ULong, val control: String, val aclVersion: ULong)
data class SecretShareJob(val id: String, val state: String, val title: String, val shareId: String, val expiresAt: ULong?, val maxSessions: ULong?,
    val bytes: ULong, val channel: SnapshotChannel?, val password: Boolean?, val request: String?, val error: String?,
    val uploaded: ULong, val chunks: ULong, val cancelRequested: Boolean)
data class SnapshotSource(val item: VaultItem, val attachments: SecretAttachments)
data class SnapshotDraft(val id: String = UUID.randomUUID().toString(), val source: SnapshotSource, val fields: Set<String> = emptySet(), val notes: Boolean = false,
    val attachments: Set<String> = emptySet(), val channel: String? = null, val lifetime: ULong = 86_400_000u, val sessions: String = "1", val password: String = "",
    val prepared: String? = null, val uncertain: Boolean = false) {
    val selected: Boolean get() = fields.isNotEmpty() || notes || attachments.isNotEmpty()
    fun input(): SecretSnapshotInput {
        check(selected && !source.item.reserved && !source.item.deleted && !source.item.conflict) { "INVALID_SELECTION" }
        val chosen = attachments.map { id ->
            val entry = source.attachments.files.singleOrNull { it.id == id } ?: error("NOT_FOUND")
            check(entry.selectable) { "RECOVERY_INCOMPLETE" }
            SecretAttachmentSelection(id, entry.details!!.file.version)
        }
        val count = if (channel == null) sessions.toULongOrNull()?.takeIf { it in 1uL..100uL } ?: error("INVALID_SESSIONS") else null
        return SecretSnapshotInput(source.item.id, source.item.revision, source.item.fields.map { it.id }.filter(fields::contains), notes, chosen, if (channel == null) lifetime else null, count)
    }
}
data class PackageField(val id: String, val kind: String, val label: String, val section: String?, val role: String? = null)
data class PackageAttachment(val name: String, val mime: String, val size: ULong, val sha256: String)
data class PackageChannel(val id: String, val sequence: ULong, val sender: String, val digest: String)
data class SecretPackage(val id: String, val digest: String, val snapshot: String, val createdAt: ULong, val kind: String, val title: String,
    val fields: List<PackageField>, val notes: Boolean, val attachments: List<PackageAttachment>, val imported: List<String>, val channel: PackageChannel?)
data class PackageInput(val channel: String? = null, val sequence: ULong? = null)
data class PackageDraft(val source: PackageInput = PackageInput(), val link: String = "", val password: String = "", val fields: Set<String> = emptySet(), val notes: Boolean = false,
    val attachments: Set<UInt> = emptySet(), val folder: String = "root", val folderName: String = "", val request: String? = null, val completed: List<String>? = null,
    val pending: SecretPackageSelection? = null) {
    val selected: Boolean get() = fields.isNotEmpty() || notes || attachments.isNotEmpty()
}

fun parseSecretShare(value: JsonElement): SecretShareJob {
    val row = value.objectValue()
    val plan = row["plan"]?.objectValue() ?: error("INTEGRITY_FAILED")
    val channel = plan["channel"]?.takeIf { it != JsonNull }?.objectValue()?.let {
        SnapshotChannel(it.required("channel_id"), it.required("name"), it.requiredUnsigned("epoch"), it.required("control_head"), it.requiredUnsigned("acl_version"))
    }
    return SecretShareJob(row.required("id"), row.required("state"), plan.required("title"), plan.required("share_id"), plan.unsigned("expires_at"), plan.unsigned("max_download_sessions"),
        plan.requiredUnsigned("plain_bytes"), channel, plan.optionalFlag("password"), row.text("request_id"), row.text("error"), row.requiredUnsigned("uploaded_chunks"), row.requiredUnsigned("total_chunks"), row.requiredFlag("cancel_requested"))
}

fun parseSecretPackage(value: JsonElement): SecretPackage {
    val row = value.objectValue()
    val preview = row["preview"]?.objectValue() ?: error("INTEGRITY_FAILED")
    check(row.requiredFlag("files_local_only") && row.requiredFlag("files_inherit_folder_grants")) { "INTEGRITY_FAILED" }
    val channel = row["channel_source"]?.takeIf { it != JsonNull }?.objectValue()?.let {
        PackageChannel(it.required("channel_id"), it.requiredUnsigned("seq"), it.required("sender_account"), it.required("message_digest"))
    }
    return SecretPackage(row.required("package_id"), row.required("digest"), preview.required("snapshot_id"), preview.requiredUnsigned("created_at"), preview.required("type"), preview.required("title"),
        preview.rows("fields").map { PackageField(it.required("id"), it.required("type"), it.required("label"), it.text("section"), it.text("role")) }, preview.requiredFlag("notes"),
        preview.rows("attachments").map { PackageAttachment(it.required("name"), it.required("mime"), it.requiredUnsigned("size"), it.required("sha256")) },
        if (row["already_imported"] == null || row["already_imported"] == JsonNull) emptyList() else row.strings("already_imported"), channel)
}

/** Preparation stays local; publication and import always open their original approval. */
class SecretSharingCoordinator(private val client: ClientController) {
    val picker = VaultBrowser(client, "secret:picker")
    val source = ReadStore<SnapshotSource>()
    val jobs = ReadStore<List<SecretShareJob>> { it.isEmpty() }
    val job = ReadStore<SecretShareJob>()
    val preview = ReadStore<SecretPackage>()
    private val mutableDraft = MutableStateFlow<SnapshotDraft?>(null)
    val draft = mutableDraft.asStateFlow()
    private val mutableIncoming = MutableStateFlow(PackageDraft())
    val incoming = mutableIncoming.asStateFlow()
    private val mutableLink = MutableStateFlow<Pair<String, String>?>(null)
    val link = mutableLink.asStateFlow()
    private var selectedJob: String? = null

    fun composer(item: String, channel: String? = null) {
        if (mutableDraft.value?.source?.item?.id == item) return
        val request = source.begin(false)
        mutableDraft.value = null
        client.perform({ core ->
            val value = core.vaultItem(item, null).item
            val attachments = parseSecretAttachments(core.secretItemAttachments(item).decode())
            check(value.revision == attachments.revision) { "VERSION_CONFLICT" }
            check(!value.reserved && !value.deleted && !value.conflict) { "FORBIDDEN" }
            SnapshotSource(value, attachments)
        }, key = "secret:compose", replace = true, onError = { source.fail(request, it) }) {
            source.accept(request, it); mutableDraft.value = SnapshotDraft(source = it, channel = channel)
        }
        client.messages.browse()
    }

    fun edit(change: (SnapshotDraft) -> SnapshotDraft) {
        client.activity()
        val current = mutableDraft.value ?: return
        if (client.state.value.working("secret:prepare:" + current.id) || current.prepared != null || current.uncertain) return
        val next = change(current)
        mutableDraft.value = if (next.channel != null) next.copy(password = "") else next
    }

    fun prepare() {
        val current = mutableDraft.value ?: return
        current.prepared?.let { client.navigation.push(Route("secret-share", it)); return }
        if (current.uncertain) { client.navigation.open("secret-shares"); return }
        if (client.state.value.working("secret:prepare:" + current.id)) return
        // Cancellation can happen after the native job was sealed but before its
        // ID returned. Never silently create another job for that same form.
        mutableDraft.value = current.copy(uncertain = true)
        client.perform({ core ->
            val input = current.input()
            parseSecretShare(client.operation { operation ->
                if (current.channel == null) core.secretSharePrepare(input, current.password.takeIf(String::isNotEmpty), operation)
                else core.secretChannelSharePrepare(current.channel, input, operation)
            }.decode())
        }, key = "secret:prepare:" + current.id, onError = { code ->
            if (mutableDraft.value?.id == current.id) mutableDraft.value = current.copy(uncertain = code in setOf("EXECUTION_UNKNOWN", "CANCELLED"))
        }) { value ->
            if (mutableDraft.value?.id == current.id) mutableDraft.value = current.copy(password = "", prepared = value.id)
            if (client.navigation.state.value.current.let { it.page == "secret-compose" && it.id == current.source.item.id }) {
                client.navigation.push(Route("secret-share", value.id))
                request(value.id)
            }
            list()
        }
    }

    fun discardDraft() { mutableDraft.value?.let { client.cancelOperation("secret:prepare:" + it.id) }; mutableDraft.value = null; source.reset() }

    fun list() {
        val request = jobs.begin()
        client.perform({ core ->
            val value = core.secretShares().decode().objectValue()
            check(value.requiredFlag("device_local")) { "INTEGRITY_FAILED" }
            value.rows("shares").map(::parseSecretShare)
        }, key = "secret:shares", replace = true, onError = { jobs.fail(request, it) }) { jobs.accept(request, it) }
    }

    fun select(id: String, reveal: Boolean = false) {
        val request = job.begin(job.value()?.id == id)
        selectedJob = id
        mutableLink.value = null
        client.perform({ core ->
            val row = core.secretShare(id, reveal).decode().objectValue()
            val url = if (reveal) row["link"]?.takeIf { it != JsonNull }?.objectValue()?.required("url") ?: error("NOT_FOUND") else null
            parseSecretShare(row) to url
        }, key = "secret:share", replace = true, onError = { job.fail(request, it) }) { (value, url) ->
            job.accept(request, value)
            mutableLink.value = url?.let { id to it }
        }
    }

    fun hideLink(id: String) { if (selectedJob == id) client.cancelOperation("secret:share"); if (mutableLink.value?.first == id) mutableLink.value = null }
    fun request(id: String) = client.approvals.request("secret-share:" + id) { it.secretShareRequest(id) }
    fun reconcile(id: String) = client.perform({ core -> client.operation { core.secretShareReconcile(id, it) } }, key = "secret:reconcile:" + id,
        onError = { select(id) }) { select(id); list() }
    fun revoke(id: String) {
        hideLink(id)
        client.perform({ core -> client.operation { core.secretShareRevoke(id, it) } }, key = "secret:revoke:" + id,
            onError = { select(id) }) { select(id); list() }
    }

    fun receiver(source: PackageInput) {
        if (mutableIncoming.value.source != source) { client.cancelOperation("secret:preview"); preview.reset(); mutableIncoming.value = PackageDraft(source) }
    }
    fun editIncoming(change: (PackageDraft) -> PackageDraft) {
        val current = mutableIncoming.value
        if (client.state.value.working("secret:import") || current.pending != null) return
        val next = change(current)
        if (next.link != current.link || next.password != current.password) {
            client.cancelOperation("secret:preview"); preview.reset()
            mutableIncoming.value = next.copy(fields = emptySet(), notes = false, attachments = emptySet(), request = null, completed = null)
        } else if (current.request == null && current.completed == null) mutableIncoming.value = next
    }
    fun inspect() {
        val current = mutableIncoming.value
        if (current.pending != null) return
        val request = preview.begin(false)
        mutableIncoming.value = current.copy(fields = emptySet(), notes = false, attachments = emptySet(), request = null, completed = null)
        client.perform({ core ->
            parseSecretPackage(client.operation { operation ->
                if (current.source.channel != null) core.secretChannelPackagePreview(current.source.channel, current.source.sequence ?: error("INVALID_SEQUENCE"), operation)
                else core.secretPackagePreview(current.link.trim(), current.password.takeIf(String::isNotEmpty), operation)
            }.decode())
        }, key = "secret:preview", replace = true, onError = { preview.fail(request, it) }) {
            preview.accept(request, it); mutableIncoming.value = mutableIncoming.value.copy(password = "")
        }
    }
    fun importRequest() {
        val current = mutableIncoming.value
        current.request?.let { client.approvals.open(it); return }
        val selection = current.pending ?: run {
            val packageValue = preview.value() ?: return
            if (!current.selected) return
            SecretPackageSelection(packageValue.id, packageValue.digest, current.fields.sorted(), current.notes, current.attachments.sorted(), current.folder)
        }
        mutableIncoming.value = current.copy(pending = selection)
        client.perform({ it.secretPackageImportRequest(selection).decode().objectValue() }, key = "secret:import", onError = { code ->
            if (code !in setOf("EXECUTION_UNKNOWN", "CANCELLED", "PLATFORM_UNAVAILABLE", "UNAVAILABLE")) mutableIncoming.value = current.copy(pending = null)
        }) { value ->
            if (value.required("state") == "done") {
                mutableIncoming.value = current.copy(password = "", completed = value.strings("ids")); client.vault.changed(); client.library.browse()
            } else {
                val id = value.required("id")
                mutableIncoming.value = current.copy(password = "", request = id, pending = selection); client.approvals.open(id)
            }
        }
    }

    fun refreshIncoming() {
        val current = mutableIncoming.value
        val id = current.request ?: return
        val selection = current.pending ?: return
        client.perform({ core ->
            val state = core.approval(id).decode().objectValue().required("state")
            state to if (state == "done") core.secretPackageImportRequest(selection).decode().objectValue().strings("ids") else null
        }, key = "secret:import:receipt", replace = true) { (state, ids) ->
            if (mutableIncoming.value.request == id) {
                if (ids != null) { mutableIncoming.value = current.copy(completed = ids, pending = null); client.vault.changed(); client.library.browse() }
                else if (state in setOf("rejected", "cancelled", "expired", "failed")) mutableIncoming.value = current.copy(pending = null, request = null)
            }
        }
    }
    fun clear(retainForms: Boolean = false) {
        picker.reset()
        source.reset(); jobs.reset(); job.reset(); preview.reset(); mutableLink.value = null; selectedJob = null
        if (!retainForms) { mutableDraft.value = null; mutableIncoming.value = PackageDraft() }
        else { mutableDraft.value = mutableDraft.value?.copy(password = ""); mutableIncoming.value = mutableIncoming.value.copy(password = "", fields = emptySet(), notes = false, attachments = emptySet()) }
    }
}
