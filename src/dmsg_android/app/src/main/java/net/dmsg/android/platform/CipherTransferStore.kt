package net.dmsg.android.platform

import kotlinx.serialization.json.*
import net.dmsg.android.core.*
import java.io.File
import java.io.FileOutputStream
import java.net.URI
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest

data class CipherPart(val slot: String, val size: Long, val digest: String, val url: String)
data class CipherPlan(val id: String, val account: String, val device: String, val method: String, val upload: String, val expires: ULong, val parts: List<CipherPart>) {
    val size get() = parts.sumOf { it.size }
    fun valid(now: ULong? = null) {
        check(transferHex(id) && transferHex(upload) && transferHex(device) && validAccount(account) && method in setOf("GET", "PUT") && parts.size in (if (method == "GET") 0 else 1)..1001 && parts.map { it.slot }.distinct().size == parts.size) { "INTEGRITY_FAILED" }
        check(now == null || expires > now && expires - now <= 86_400_000uL) { "TRANSFER_EXPIRED" }
        var origin: String? = null
        for (part in parts) {
            check(part.slot == "manifest" && method == "PUT" || part.slot.matches(Regex("0|[1-9][0-9]{0,2}"))) { "INTEGRITY_FAILED" }
            check(part.size in 0..MAX_PART && transferHex(part.digest)) { "INTEGRITY_FAILED" }
            val uri = URI(part.url)
            check(part.url.length <= 2048 && uri.scheme == "https" && uri.host != null && uri.rawUserInfo == null && uri.rawFragment == null && uri.rawPath == "/mobile-transfer/v1/$account/$id/" + part.slot && uri.rawQuery?.matches(Regex("token=[0-9a-f]{64}")) == true) { "INTEGRITY_FAILED" }
            val source = uri.scheme + "://" + uri.rawAuthority
            if (origin == null) origin = source else check(origin == source) { "INTEGRITY_FAILED" }
        }
    }
    companion object { const val MAX_PART = 1024L * 1024L + 64L }
}
enum class CipherPhase(val wire: String) {
    TRANSFERRING("transferring"), PAUSED("paused"), FAILED("failed"), VERIFYING("verifying");

    companion object {
        fun parse(value: String) = entries.firstOrNull { it.wire == value } ?: error("INTEGRITY_FAILED")
    }
}

data class CipherJob(val plan: CipherPlan, val phase: CipherPhase, val attempt: Int = 0, val completed: Set<String> = emptySet(), val error: String? = null, val retries: Int = 0) {
    val ready get() = phase == CipherPhase.VERIFYING
    val bytes get() = plan.parts.filter { it.slot in completed }.sumOf { it.size }
    fun accepts(id: String, attempt: Int) = plan.id == id && this.attempt == attempt && phase == CipherPhase.TRANSFERRING
}
fun parseCipherPlan(value: JsonElement): CipherPlan = value.objectValue().let { row ->
    CipherPlan(row.required("id"), row.required("account"), row.required("device"), row.required("method"), row.required("upload_id"), row.requiredUnsigned("expires_at"), row.rows("parts").map {
        val size = it.requiredUnsigned("size"); check(size <= CipherPlan.MAX_PART.toULong()) { "INTEGRITY_FAILED" }
        CipherPart(it.required("slot"), size.toLong(), it.required("digest"), it.required("url"))
    }).also { it.valid() }
}

/** Separate ciphertext directory, one bounded plan, no core handle or root keys.
 * All writers are in the application process; completion is fenced by id+attempt. */
class CipherTransferStore(val root: File, private val installation: String) {
    private val index get() = File(root, "transfer.json")
    private data class Stamp(val modified: Long, val size: Long, val revision: Long)
    private var cached: Pair<Stamp, CipherJob>? = null
    private val identity = root.canonicalPath
    private fun stamp() = Stamp(Files.getLastModifiedTime(index.toPath()).to(java.util.concurrent.TimeUnit.NANOSECONDS), index.length(), revisions[identity] ?: 0L)
    init { check(root.mkdirs() || root.isDirectory); check(installation.length in 1..128) }
    fun snapshot(): CipherJob? = synchronized(lock) { read() }
    private fun read(): CipherJob? {
        if (!index.exists()) { cached = null; return null }
        val stamp = stamp()
        cached?.takeIf { it.first == stamp }?.let { return it.second }
        check(index.length() in 1..2L * 1024 * 1024) { "INTEGRITY_FAILED" }
        val row = Json.parseToJsonElement(index.readText()).objectValue()
        check(row.required("installation") == installation) { "INSTALLATION_MISMATCH" }
        val plan = parseCipherPlan(row["transfer"]!!)
        val phase = CipherPhase.parse(row.required("phase"))
        val attempt = row.requiredUnsigned("attempt")
        val retries = row.requiredUnsigned("retries")
        val completed = row.strings("completed").toSet()
        check(attempt <= Int.MAX_VALUE.toULong() && retries <= 4uL && completed.all { slot -> plan.parts.any { it.slot == slot } } && (phase != CipherPhase.VERIFYING || completed.size == plan.parts.size)) { "INTEGRITY_FAILED" }
        return CipherJob(plan, phase, attempt.toInt(), completed, row.text("error"), retries.toInt()).also { cached = stamp to it }
    }
    private fun save(value: CipherJob) {
        val bytes = buildJsonObject {
            put("installation", installation)
            put("transfer", planJson(value.plan))
            put("phase", value.phase.wire); put("attempt", value.attempt); put("retries", value.retries)
            putJsonArray("completed") { value.completed.forEach { add(it) } }
            put("error", value.error?.let(::JsonPrimitive) ?: JsonNull)
        }.toString().toByteArray()
        atomicWrite(index, bytes)
        revisions[identity] = (revisions[identity] ?: 0) + 1
        cached = stamp() to value
    }
    fun start(plan: CipherPlan, now: ULong): CipherJob = synchronized(lock) {
        check(read() == null) { "TRANSFER_PENDING" }; plan.valid(now)
        check(root.usableSpace >= plan.size + 8 * 1024 * 1024) { "DISK_FULL" }
        CipherJob(plan, phase = if (plan.parts.isEmpty()) CipherPhase.VERIFYING else CipherPhase.PAUSED,
            error = if (plan.parts.isEmpty()) null else "PREPARING_CIPHERTEXT").also(::save)
    }
    fun stage(plan: CipherPlan, slot: String, bytes: ByteArray) = synchronized(lock) {
        check(read()?.plan?.id == plan.id) { "NOT_FOUND" }
        val part = plan.parts.single { it.slot == slot }
        verify(part, bytes)
        atomicWrite(path(plan, slot), bytes)
    }
    fun verifiedPart(plan: CipherPlan, slot: String): ByteArray = synchronized(lock) {
        val part = plan.parts.single { it.slot == slot }; val file = path(plan, slot)
        check(file.isFile && file.length() == part.size) { "MISSING_CIPHERTEXT" }
        file.readBytes().also { verify(part, it) }
    }
    fun receive(id: String, attempt: Int, slot: String, bytes: ByteArray): Boolean = synchronized(lock) {
        val current = read()?.takeIf { it.accepts(id, attempt) } ?: return false
        stage(current.plan, slot, bytes); complete(id, attempt, slot)
    }
    fun complete(id: String, attempt: Int, slot: String): Boolean = synchronized(lock) {
        val current = read()?.takeIf { it.accepts(id, attempt) } ?: return false
        check(current.plan.parts.any { it.slot == slot }) { "INTEGRITY_FAILED" }
        val completed = current.completed + slot
        save(current.copy(completed = completed, phase = if (completed.size == current.plan.parts.size) CipherPhase.VERIFYING else CipherPhase.TRANSFERRING, error = null, retries = 0)); true
    }
    /** Invalidate the observed attempt before stopping its system job. A retry takes the same lock. */
    fun pause(id: String, attempt: Int, error: String? = null, stop: () -> Unit = {}): Boolean = synchronized(lock) {
        val current = read()?.takeIf { it.accepts(id, attempt) } ?: return false
        save(current.copy(phase = CipherPhase.PAUSED, attempt = Math.addExact(current.attempt, 1), error = error))
        stop()
        true
    }
    fun retry(expected: CipherJob, now: ULong): CipherJob = synchronized(lock) {
        val current = read() ?: error("NOT_FOUND")
        check(current.plan.id == expected.plan.id && current.attempt == expected.attempt) { "TRANSFER_STOPPED" }
        check(current.plan.expires > now) { "TRANSFER_EXPIRED" }
        val next = current.copy(attempt = Math.addExact(current.attempt, 1), phase = if (current.completed.size == current.plan.parts.size) CipherPhase.VERIFYING else CipherPhase.TRANSFERRING, error = null, retries = 0)
        save(next); next
    }
    /** Return true only for a bounded retry of the same authorization. */
    fun fail(id: String, attempt: Int, reason: String, transient: Boolean): Boolean = synchronized(lock) {
        val current = read()?.takeIf { it.accepts(id, attempt) } ?: return false
        val count = (current.retries + 1).coerceAtMost(4)
        val retry = transient && count <= 3 && current.plan.expires > System.currentTimeMillis().coerceAtLeast(0).toULong()
        save(current.copy(phase = if (retry) CipherPhase.TRANSFERRING else CipherPhase.FAILED, error = reason, retries = count)); retry
    }
    fun discard(plan: CipherPlan) = synchronized(lock) {
        plan.valid()
        val current = read()
        if (current != null) check(current.plan.id == plan.id) { "TRANSFER_PENDING" }
        if (current?.plan?.id == plan.id) Files.deleteIfExists(index.toPath())
        val directory = File(root, plan.id).canonicalFile
        check(directory.parentFile == root.canonicalFile) { "FORBIDDEN" }
        if (directory.exists()) check(directory.deleteRecursively()) { "FILE_UNAVAILABLE" }
    }
    private fun path(plan: CipherPlan, slot: String): File {
        check(transferHex(plan.id) && (slot == "manifest" || slot.matches(Regex("0|[1-9][0-9]{0,2}")))) { "INTEGRITY_FAILED" }
        check(plan.parts.any { it.slot == slot }) { "NOT_FOUND" }
        val directory = File(root, plan.id); check(directory.mkdirs() || directory.isDirectory)
        val target = File(directory, slot)
        check(directory.canonicalFile == File(root.canonicalFile, plan.id) && target.canonicalFile.parentFile == directory.canonicalFile) { "FORBIDDEN" }
        return target
    }
    companion object {
        private val lock = Any()
        private val revisions = mutableMapOf<String, Long>()
        fun digest(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
        private fun verify(part: CipherPart, bytes: ByteArray) { check(bytes.size.toLong() == part.size && digest(bytes) == part.digest) { "INTEGRITY_FAILED" } }
        private fun atomicWrite(file: File, bytes: ByteArray) {
            val temp = File(file.parentFile, file.name + ".new")
            try {
                FileOutputStream(temp).use { it.write(bytes); it.fd.sync() }
                Files.move(temp.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            } finally { temp.delete() }
        }
        private fun planJson(value: CipherPlan) = buildJsonObject {
            put("id", value.id); put("account", value.account); put("device", value.device); put("method", value.method); put("upload_id", value.upload)
            put("expires_at", JsonPrimitive(value.expires.toString().toBigInteger()))
            putJsonArray("parts") { value.parts.forEach { part -> add(buildJsonObject { put("slot", part.slot); put("size", part.size); put("digest", part.digest); put("url", part.url) }) } }
        }
    }
}
