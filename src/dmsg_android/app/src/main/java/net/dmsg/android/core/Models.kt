package net.dmsg.android.core

import net.dmsg.core.*
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

data class LibraryFolder(val id: String, val name: String, val parent: String, val path: List<String>)
data class LibraryFile(val id: String, val name: String, val size: ULong, val status: String, val sha256: String, val mime: String, val version: String,
    val folder: String, val local: Boolean, val starred: Boolean, val tags: List<String>, val note: String, val updatedAt: ULong,
    val backup: FileBackup? = null, val trashedAt: ULong? = null, val path: List<String> = emptyList())
data class FileBackup(val upload: String, val generation: ULong, val backedUpAt: ULong, val verified: Boolean)
data class FileVersion(val id: String, val size: ULong, val sha256: String, val createdAt: ULong, val device: String, val local: Boolean, val current: Boolean, val status: String, val backup: FileBackup?, val mime: String? = null)
fun LibraryFile.atVersion(v: FileVersion) = copy(version = v.id, mime = v.mime ?: (if (v.current) mime else "application/octet-stream"), size = v.size, sha256 = v.sha256, status = v.status, local = v.local, backup = v.backup)

data class FileDetails(val file: LibraryFile, val versions: List<FileVersion>)
data class Letter(val id: String, val sender: String, val text: String, val read: Boolean, val archived: Boolean, val state: String, val createdAt: ULong)
data class Approval(val id: String, val digest: String, val kind: String, val state: String, val source: String, val risk: String, val expiresAt: ULong, val supported: Boolean)
enum class AccessPage { WELCOME, JOIN_ACCOUNT, PAIRING, RECOVERY, HANDOVER }

data class UiState(
    val starting: Boolean = true,
    val bindingRetry: Boolean = false,
    val concealed: Boolean = true,
    val busy: Boolean = false,
    val operations: Set<String> = emptySet(),
    val session: SessionSnapshot? = null,
    val epoch: Long = 0,
    val error: String? = null,
    val notice: String? = null,
    val dark: Boolean? = null,
    val immediateLock: Boolean = false,
) {
    val unlocked: Boolean get() = session?.state == "unlocked"
    fun working(resource: String) = resource in operations
}

fun parseBackup(value: kotlinx.serialization.json.JsonElement?): FileBackup? {
    if (value == null || value == kotlinx.serialization.json.JsonNull) return null
    val row = value.objectValue()
    return FileBackup(row.required("upload_id"), row.requiredUnsigned("generation"), row.requiredUnsigned("backed_up_at"), row.requiredFlag("verified"))
}
fun parseFile(row: kotlinx.serialization.json.JsonObject) = LibraryFile(row.required("id"), row.required("name"), row.requiredUnsigned("size"), row.required("status"), row.required("sha256"), row.required("mime"), row.required("current"),
    row.required("folder"), row.requiredFlag("local"), row.requiredFlag("starred"), row.strings("tags"), row.required("note"), row.requiredUnsigned("updated_at"), parseBackup(row["backup"]), row.unsigned("trashed_at"), row.strings("path"))
fun parseFiles(json: String): Pair<List<LibraryFolder>, List<LibraryFile>> {
    val root = Json.parseToJsonElement(json).objectValue()
    return root.rows("folders").map(::parseFolder) to root.rows("files").map(::parseFile)
}
fun parseFolder(row: JsonObject) = LibraryFolder(row.required("id"), row.required("name"), row.required("parent"), row.strings("path"))
fun parseFileDetails(value: kotlinx.serialization.json.JsonElement): FileDetails {
    val root = value.objectValue()
    return FileDetails(parseFile(root["file"]!!.objectValue()), root.rows("versions").map {
        FileVersion(it.required("id"), it.requiredUnsigned("size"), it.required("sha256"), it.requiredUnsigned("created_at"), it.required("device"), it.requiredFlag("local"), it.requiredFlag("current"), it.required("status"), parseBackup(it["backup"]), it.text("mime"))
    })
}
fun parseApprovals(json: String) = Json.parseToJsonElement(json).arrayRows().map { Approval(it.required("id"), it.required("digest"), it.required("type"), it.required("state"), it.required("source"), it.required("risk"), it.requiredUnsigned("expires_at"), it.requiredFlag("mobile_supported")) }
fun parseLetters(json: String) = Json.parseToJsonElement(json).arrayRows().map { Letter(it.required("order_id"), it.required("sender"), it.required("text"), it.requiredFlag("read"), it.requiredFlag("archived"), it.required("state"), it.requiredUnsigned("created_at")) }

object LoginCallback {
    const val HOST = "dmsg.net"
    const val PATH = "/_/android/auth/callback"
    const val TARGET = "https://$HOST$PATH"
    fun valid(raw: String): Boolean {
        if (raw.length > 96 * 1024 || !raw.startsWith("$TARGET#")) return false
        val fragment = raw.substring(TARGET.length + 1)
        return fragment.isNotEmpty() && fragment.all { it.isLetterOrDigit() && it.code < 128 || it == '-' || it == '_' }
    }
}
