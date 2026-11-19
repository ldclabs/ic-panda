package net.dmsg.android.core

import kotlinx.serialization.json.*

data class SecretReview(val importing: Boolean, val title: String, val fields: List<PackageField>, val notes: Boolean, val files: List<PackageAttachment>,
    val source: String, val revision: String, val destination: String?, val channel: SnapshotChannel?, val expiry: ULong?, val sessions: ULong?, val password: Boolean?)

/** Only a complete, exact selection may enable approval; private values are never parsed. */
fun parseSecretReview(kind: String, value: JsonElement): SecretReview {
    val row = value.objectValue()
    fun fields(root: JsonObject) = root.rows("fields").map { PackageField(it.required("id"), it.required("type"), it.required("label"), it.text("section"), it.text("role")) }
    fun files(root: JsonObject) = root.rows("attachments").map { PackageAttachment(it.required("name"), it.required("mime"), it.requiredUnsigned("size"), it.required("sha256")) }
    if (kind == "secret_share") {
        val input = row["input"]?.objectValue() ?: error("INTEGRITY_FAILED")
        val chosen = input.strings("field_ids")
        val selected = fields(row)
        check(chosen.size == chosen.distinct().size && selected.map { it.id }.toSet() == chosen.toSet() && selected.size == chosen.size) { "INTEGRITY_FAILED" }
        val attachments = files(row)
        check(attachments.size == input.rows("attachments").size) { "INTEGRITY_FAILED" }
        val channel = row["channel"]?.takeIf { it != JsonNull }?.objectValue()?.let {
            SnapshotChannel(it.required("channel_id"), it.required("name"), it.requiredUnsigned("epoch"), it.required("control_head"), it.requiredUnsigned("acl_version"))
        }
        val expiry = if (channel == null) row.requiredUnsigned("expires_at") else null
        val sessions = if (channel == null) row.requiredUnsigned("max_download_sessions") else null
        return SecretReview(false, row.required("title"), selected, input.requiredFlag("include_notes"), attachments, input.required("item_id"), input.required("revision"),
            row.required("account_id"), channel, expiry, sessions, if (channel == null) row.requiredFlag("password") else null)
    }
    check(kind == "secret_package_import" && row.requiredFlag("files_local_only") && row.requiredFlag("files_inherit_folder_grants")) { "INTEGRITY_FAILED" }
    val preview = row["preview"]?.objectValue() ?: error("INTEGRITY_FAILED")
    val selection = row["selection"]?.objectValue() ?: error("INTEGRITY_FAILED")
    val chosen = selection.strings("fields")
    val metadata = fields(preview)
    val selected = chosen.map { id -> metadata.singleOrNull { it.id == id } ?: error("INTEGRITY_FAILED") }
    val indexes = (selection["attachments"] as? JsonArray)?.map {
        val number = it as? JsonPrimitive ?: error("INTEGRITY_FAILED")
        check(!number.isString) { "INTEGRITY_FAILED" }
        number.content.toUIntOrNull()?.takeIf { it <= Int.MAX_VALUE.toUInt() }?.toInt() ?: error("INTEGRITY_FAILED")
    } ?: error("INTEGRITY_FAILED")
    val allFiles = files(preview)
    check(chosen.size == chosen.distinct().size && indexes.size == indexes.distinct().size) { "INTEGRITY_FAILED" }
    val notes = selection.requiredFlag("include_notes")
    check(!notes || preview.requiredFlag("notes")) { "INTEGRITY_FAILED" }
    return SecretReview(true, preview.required("title"), selected, notes, indexes.map { allFiles.getOrNull(it) ?: error("INTEGRITY_FAILED") },
        selection.required("package_id"), selection.required("digest"), selection.required("folder"), null, null, null, null)
}
