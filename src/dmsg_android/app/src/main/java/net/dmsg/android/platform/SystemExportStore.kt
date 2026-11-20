package net.dmsg.android.platform

import kotlinx.serialization.json.*
import net.dmsg.android.core.*
import java.io.Closeable
import java.io.File
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

enum class ExportPurpose { credentials, files }
data class ExportHeader(val id: String, val installation: String, val purpose: ExportPurpose, val account: String, val created: Long, val expires: Long, val count: Int, val biometryOnly: Boolean, val wrapped: ByteArray, val digest: String? = null) {
    fun binding() = ("$installation:$purpose:$id:$account:$created:$expires:$count:$biometryOnly" + digest?.let { ":$it" }.orEmpty()).toByteArray()
}
data class ExportCredential(val id: String, val title: String, val username: String, val origin: String, val itemId: String? = null) {
    fun afterUnlock(previous: ExportHeader, current: ExportHeader, entries: List<ExportCredential>): ExportCredential? {
        if (previous.installation != current.installation || previous.account != current.account) return null
        return entries.firstOrNull { candidate ->
            val sameItem = if (previous.id == current.id) candidate.id == id else itemId != null && candidate.itemId == itemId
            sameItem && candidate.title == title && candidate.username == username && candidate.origin == origin
        }
    }
}
data class ExportCredentialInput(val title: String, val username: String, val password: String, val origin: String, val itemId: String? = null)
data class ExportFile(val id: String, val name: String, val mime: String, val size: Long, val sha256: String, val parts: Int, val modified: Long)
data class ExportFileInput(val path: File, val name: String, val mime: String, val size: Long, val sha256: String)
data class ExportWrappedKey(val key: ByteArray, val wrapped: ByteArray)
interface ExportKeys {
    fun create(header: ExportHeader): ExportWrappedKey
    fun open(header: ExportHeader, valid: () -> Boolean): ByteArray
    fun remove(header: ExportHeader)
}
class OpenExport(val header: ExportHeader, val key: ByteArray) : Closeable { override fun close() { key.fill(0) } }
class PreparedExport internal constructor(val opened: OpenExport, internal val revision: Long, internal val previous: String?) : Closeable { override fun close() = opened.close() }

/** A standalone, installation-bound copy store. No core handle or workspace key. */
class SystemExportStore(val root: File, val purpose: ExportPurpose, private val keys: ExportKeys, private val now: () -> Long = System::currentTimeMillis) {
    private class Guard { var revision = 0L; val preparing = mutableSetOf<String>() }
    private val guard = synchronized(guards) { guards.getOrPut(root.canonicalPath) { Guard() } }
    val installation: String
    init {
        synchronized(guard) {
            check(root.mkdirs() || root.isDirectory)
            val marker = File(root, "installation")
            if (!marker.exists()) {
                check(root.listFiles().isNullOrEmpty()) { "INSTALLATION_MISMATCH" }
                PrivateFiles.atomicWrite(marker, UUID.randomUUID().toString().toByteArray())
            }
            check(marker.length() in 1..128) { "INSTALLATION_MISMATCH" }
            installation = marker.readText().also(::uuid)
            // After process death, incomplete copies have no publication pointer.
            // Live preparations in this process remain protected by this guard.
            val current = header(false)?.id
            root.listFiles().orEmpty().filter { it.isDirectory && it.name != current && it.name !in guard.preparing }.forEach { directory ->
                if (runCatching { uuid(directory.name) }.isSuccess) {
                    remove(ExportHeader(directory.name, installation, purpose, "", 0, 0, 0, false, byteArrayOf()))
                }
            }
        }
    }
    fun header(checkExpiry: Boolean = true): ExportHeader? = synchronized(guard) {
        val file = File(root, "current.json")
        if (!file.exists()) return@synchronized null
        check(file.length() in 1..4096) { "INTEGRITY_FAILED" }
        val row = Json.parseToJsonElement(file.readText()).objectValue()
        val result = ExportHeader(row.required("id").also(::uuid), row.required("installation"), ExportPurpose.valueOf(row.required("purpose")), row.required("account"),
            row.requiredUnsigned("created").toLong(), row.requiredUnsigned("expires").toLong(), row.requiredUInt("count").toInt(), row.requiredFlag("biometry_only"), Base64.getDecoder().decode(row.required("wrapped")), row.text("digest"))
        check(result.installation == installation && result.purpose == purpose && result.account.matches(Regex("[a-f0-9]{64}")) && (if (purpose == ExportPurpose.credentials) result.count >= 0 else result.count in 1..10) && result.wrapped.size in 1..1024 && result.created > 0 && (if (result.digest != null) purpose == ExportPurpose.credentials && result.expires == 0L else result.expires > result.created && result.expires - result.created <= DAY)) { "INTEGRITY_FAILED" }
        if (checkExpiry) checkFresh(result)
        result
    }
    private fun checkFresh(header: ExportHeader) { check(now() >= header.created && (header.digest != null || now() < header.expires)) { "EXPORT_EXPIRED" } }
    fun current(header: ExportHeader): Boolean = runCatching { this.header()?.id == header.id }.getOrDefault(false)
    fun open(valid: () -> Boolean = { true }): OpenExport {
        val header = header() ?: error("EXPORT_NOT_CONFIGURED")
        val key = keys.open(header) { valid() && current(header) }
        try { check(key.size == 32 && valid() && current(header)) { "CANCELLED" }; return OpenExport(header, key) }
        catch (error: Throwable) { key.fill(0); throw error }
    }
    private fun prepare(account: String, count: Int, only: Boolean, digest: String? = null): PreparedExport {
        val time = now()
        val header = ExportHeader(UUID.randomUUID().toString(), installation, purpose, digest(account.toByteArray()), time, if (digest == null) time + DAY else 0L, count, only, byteArrayOf(), digest)
        val (revision, previous) = synchronized(guard) { (guard.revision to header(false)?.id).also { guard.preparing += header.id } }
        try {
            val wrapped = keys.create(header)
            check(wrapped.key.size == 32) { "INTEGRITY_FAILED" }
            val prepared = PreparedExport(OpenExport(header.copy(wrapped = wrapped.wrapped), wrapped.key), revision, previous)
            try { check(directory(header).mkdirs()); return prepared }
            catch (error: Throwable) { prepared.close(); throw error }
        } catch (error: Throwable) {
            synchronized(guard) { guard.preparing -= header.id; remove(header) }
            throw error
        }
    }
    fun prepareCredentials(entries: List<ExportCredentialInput>, account: String, only: Boolean, digest: String? = null, valid: () -> Boolean = { true }): PreparedExport {
        check(purpose == ExportPurpose.credentials) { "INVALID_INPUT" }
        val prepared = prepare(account, entries.size, only, digest)
        try {
            val manifest = buildJsonArray {
                entries.forEach { value ->
                    check(valid()) { "CANCELLED" }
                    check(value.password.toByteArray().size in 1..65536 && value.username.toByteArray().size <= 65536) { "FILE_LIMIT" }
                    val id = UUID.randomUUID().toString()
                    val bytes = value.password.toByteArray()
                    try { seal(prepared.opened, id, bytes) } finally { bytes.fill(0) }
                    add(buildJsonObject { put("id", id); put("title", value.title); put("username", value.username); put("origin", value.origin); value.itemId?.let { put("item", it) } })
                }
            }
            seal(prepared.opened, "manifest", manifest.toString().toByteArray()); return prepared
        } catch (error: Throwable) { discard(prepared); throw error }
    }
    fun prepareFiles(entries: List<ExportFileInput>, account: String, only: Boolean, valid: () -> Boolean = { true }): PreparedExport {
        check(purpose == ExportPurpose.files && entries.size in 1..10 && entries.all { it.size in 0..MAX_FILE } && entries.sumOf { it.size } <= 200L * PART) { "FILE_LIMIT" }
        val prepared = prepare(account, entries.size, only)
        try {
            val manifest = buildJsonArray {
                entries.forEach { value ->
                    check(value.name.toByteArray().size <= 4096 && value.mime.length <= 255 && value.sha256.matches(Regex("[a-f0-9]{64}"))) { "INVALID_INPUT" }
                    val id = UUID.randomUUID().toString(); var size = 0L; var parts = 0
                    val hash = MessageDigest.getInstance("SHA-256")
                    value.path.inputStream().use { input ->
                        while (true) {
                            check(valid()) { "CANCELLED" }
                            val bytes = input.readChunk(PART)
                            if (bytes.isEmpty()) break
                            try { size += bytes.size; check(size <= value.size) { "INTEGRITY_FAILED" }; hash.update(bytes); seal(prepared.opened, "$id.$parts", bytes); parts++ }
                            finally { bytes.fill(0) }
                        }
                    }
                    check(size == value.size && hash.digest().hex() == value.sha256) { "INTEGRITY_FAILED" }
                    add(buildJsonObject { put("id", id); put("name", value.name); put("mime", value.mime); put("size", size); put("sha256", value.sha256); put("parts", parts); put("modified", prepared.opened.header.created) })
                }
            }
            seal(prepared.opened, "manifest", manifest.toString().toByteArray()); return prepared
        } catch (error: Throwable) { discard(prepared); throw error }
    }
    fun publish(prepared: PreparedExport, valid: () -> Boolean = { true }) = synchronized(guard) {
        val value = prepared.opened.header
        check(value.purpose == purpose && value.installation == installation && valid() && guard.revision == prepared.revision && header(false)?.id == prepared.previous) { "CANCELLED" }
        checkFresh(value)
        val previous = header(false)
        val encoded = buildJsonObject {
            put("id", value.id); put("installation", value.installation); put("purpose", purpose.name); put("account", value.account)
            put("created", value.created); put("expires", value.expires); put("count", value.count); put("biometry_only", value.biometryOnly); put("wrapped", Base64.getEncoder().encodeToString(value.wrapped)); value.digest?.let { put("digest", it) }
        }
        PrivateFiles.atomicWrite(File(root, "current.json"), encoded.toString().toByteArray()); guard.revision++
        guard.preparing -= value.id
        previous?.let { remove(it) }
    }
    fun invalidate(account: String? = null) = synchronized(guard) {
        val current = header(false)
        if (account != null && current != null && current.account != digest(account.toByteArray())) return@synchronized
        guard.revision++
        check(!File(root, "current.json").exists() || File(root, "current.json").delete()) { "STORAGE_UNAVAILABLE" }
        current?.let { remove(it) }
        root.listFiles()?.filter { it.name.startsWith("used-") }?.forEach { check(it.delete()) { "STORAGE_UNAVAILABLE" } }
    }
    /** Revoke a superseded catalog while retaining remembered choices. */
    fun withdrawCredentials(old: ExportHeader) = synchronized(guard) {
        if (purpose == ExportPurpose.credentials && header(false)?.id == old.id) {
            check(File(root, "current.json").delete()) { "STORAGE_UNAVAILABLE" }
            guard.revision++
            remove(old)
        }
    }

    fun discard(prepared: PreparedExport) {
        synchronized(guard) {
            guard.preparing -= prepared.opened.header.id
            if (header(false)?.id != prepared.opened.header.id) remove(prepared.opened.header)
        }
        prepared.close()
    }
    private fun remove(header: ExportHeader) { keys.remove(header); directory(header).deleteRecursively() }
    private fun directory(header: ExportHeader): File {
        uuid(header.id); check(header.installation == installation && header.purpose == purpose) { "INTEGRITY_FAILED" }
        return File(root, header.id).also { check(it.canonicalFile == File(root.canonicalFile, header.id)) { "INTEGRITY_FAILED" } }
    }
    private fun file(header: ExportHeader, name: String) = File(directory(header), name).also {
        check(name == "manifest" || name.matches(Regex("[a-f0-9-]{36}(\\.[0-9]{1,3})?"))) { "INTEGRITY_FAILED" }
        check(it.canonicalFile.parentFile == directory(header).canonicalFile) { "INTEGRITY_FAILED" }
    }
    private fun limit(name: String) = if (purpose == ExportPurpose.credentials && name == "manifest") 64 * PART else PART

    private fun usageFile(account: String, service: String) = File(root, "used-" + digest("$account:$service".toByteArray()))

    fun recentLogins(account: String, service: String?): List<String> = synchronized(guard) {
        if (service == null) return@synchronized emptyList()
        val file = usageFile(account, service)
        if (file.exists()) Json.parseToJsonElement(file.readText()).jsonArray.map { it.jsonPrimitive.content } else emptyList()
    }

    fun remember(item: String, service: String, opened: OpenExport) = synchronized(guard) {
        check(current(opened.header)) { "EXPORT_EXPIRED" }
        val account = opened.header.account
        val ids = listOf(item) + recentLogins(account, service).filterNot { it == item }
        PrivateFiles.atomicWrite(usageFile(account, service), buildJsonArray { ids.forEach { add(it) } }.toString().toByteArray())
    }

    private fun seal(opened: OpenExport, name: String, bytes: ByteArray) {
        check(bytes.size <= limit(name))
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, SecretKeySpec(opened.key, "AES")); updateAAD(opened.header.binding() + ":$name".toByteArray()) }
        PrivateFiles.atomicWrite(file(opened.header, name), cipher.iv + cipher.doFinal(bytes))
    }
    fun plain(opened: OpenExport, name: String): ByteArray {
        check(current(opened.header)) { "EXPORT_EXPIRED" }
        val source = file(opened.header, name)
        check(source.length() in 28..limit(name) + 28L) { "INTEGRITY_FAILED" }
        val bytes = source.readBytes()
        return Cipher.getInstance("AES/GCM/NoPadding").run {
            init(Cipher.DECRYPT_MODE, SecretKeySpec(opened.key, "AES"), GCMParameterSpec(128, bytes.copyOfRange(0, 12)))
            updateAAD(opened.header.binding() + ":$name".toByteArray()); doFinal(bytes, 12, bytes.size - 12)
        }
    }
    private fun metadata(opened: OpenExport): List<JsonObject> {
        val bytes = plain(opened, "manifest")
        try { return Json.parseToJsonElement(bytes.toString(Charsets.UTF_8)).arrayRows().also { check(it.size == opened.header.count) { "INTEGRITY_FAILED" } } }
        finally { bytes.fill(0) }
    }
    fun credentials(opened: OpenExport): List<ExportCredential> {
        check(purpose == ExportPurpose.credentials)
        return metadata(opened).map { ExportCredential(it.required("id").also(::uuid), it.required("title"), it.required("username"), it.required("origin"), it.text("item")) }
    }
    fun files(opened: OpenExport): List<ExportFile> {
        check(purpose == ExportPurpose.files)
        return metadata(opened).map { row ->
            ExportFile(row.required("id").also(::uuid), row.required("name"), row.required("mime"), row.requiredUnsigned("size").toLong(), row.required("sha256"), row.requiredUInt("parts").toInt(), row.requiredUnsigned("modified").toLong()).also {
                check(it.size in 0..MAX_FILE && it.parts == ((it.size + PART - 1) / PART).toInt() && it.sha256.matches(Regex("[a-f0-9]{64}"))) { "INTEGRITY_FAILED" }
            }
        }
    }
    companion object {
        const val PART = 1024 * 1024
        const val MAX_FILE = 100L * PART
        const val DAY = 86_400_000L
        private val guards = mutableMapOf<String, Guard>()
        private fun uuid(value: String) { check(UUID.fromString(value).toString() == value) { "INTEGRITY_FAILED" } }
        fun digest(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).hex()
        private fun ByteArray.hex() = joinToString("") { "%02x".format(it) }
    }
}
