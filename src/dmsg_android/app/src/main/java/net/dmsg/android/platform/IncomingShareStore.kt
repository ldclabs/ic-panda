package net.dmsg.android.platform

import kotlinx.serialization.json.*
import net.dmsg.android.core.*
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest
import java.util.UUID

interface IncomingSealer {
    fun seal(scope: String, bytes: ByteArray): ByteArray
    fun open(scope: String, bytes: ByteArray): ByteArray
}
data class IncomingShare(val id: String, val name: String, val mime: String, val size: Long, val digest: String, val parts: Int, val account: String? = null, val folder: String? = null) {
    val complete get() = transferHex(digest)
}
data class SharedReceipt(val fingerprint: String, val id: String, val at: Long)
private data class IncomingIndex(val entries: List<IncomingShare> = emptyList(), val receipts: List<SharedReceipt> = emptyList(), val needsReview: Boolean = false)

/** Encrypted intake only. This store cannot open a workspace or choose an account. */
class IncomingShareStore(val root: File, private val box: IncomingSealer) {
    init { check(root.mkdirs() || root.isDirectory) }
    private val index get() = File(root, "pending.sealed")
    private fun read(): IncomingIndex {
        if (!index.exists()) return IncomingIndex()
        check(index.length() in 1..256L * 1024) { "INTEGRITY_FAILED" }
        val plain = box.open("index", index.readBytes())
        try {
            val value = Json.parseToJsonElement(plain.toString(Charsets.UTF_8)).objectValue()
            val entries = value.rows("entries").map { row ->
                IncomingShare(row.required("id"), row.required("name"), row.required("mime"), row.requiredUnsigned("size").also { check(it <= MAX_FILE.toULong()) }.toLong(), row.required("digest"), row.requiredUInt("parts").toInt(), row.text("account"), row.text("folder")).also(::validate)
            }
            check(entries.size <= 10 && entries.map { it.id }.distinct().size == entries.size && entries.sumOf { it.size } <= MAX_TOTAL) { "INTEGRITY_FAILED" }
            val receipts = value.rows("receipts").map { SharedReceipt(it.required("fingerprint"), it.required("id"), it.requiredUnsigned("at").toLong()).also { receipt -> check(transferHex(receipt.fingerprint) && uuid(receipt.id) && receipt.at >= 0) } }
            check(receipts.size <= 256) { "INTEGRITY_FAILED" }
            return IncomingIndex(entries, receipts, value.requiredFlag("needs_review"))
        } finally { plain.fill(0) }
    }
    private fun save(value: IncomingIndex) {
        val data = buildJsonObject {
            put("needs_review", value.needsReview)
            putJsonArray("entries") { value.entries.forEach { item -> add(buildJsonObject {
                put("id", item.id); put("name", item.name); put("mime", item.mime); put("size", item.size); put("digest", item.digest); put("parts", item.parts)
                put("account", item.account?.let(::JsonPrimitive) ?: JsonNull); put("folder", item.folder?.let(::JsonPrimitive) ?: JsonNull)
            }) } }
            putJsonArray("receipts") { value.receipts.forEach { receipt -> add(buildJsonObject { put("fingerprint", receipt.fingerprint); put("id", receipt.id); put("at", receipt.at) }) } }
        }.toString().toByteArray()
        try { atomic(index, box.seal("index", data)) } finally { data.fill(0) }
    }
    fun list(): List<IncomingShare> = synchronized(lock) { read().entries }
    fun pendingReview(): Boolean = synchronized(lock) { read().let { it.needsReview && it.entries.isNotEmpty() } }
    fun acknowledgeReview() = synchronized(lock) { save(read().copy(needsReview = false)) }

    /** Provider size is a bound when supplied; all bytes are sealed before disk. */
    fun stage(input: InputStream, name: String, mime: String, expected: Long?, sourceHash: String, current: () -> Boolean): IncomingShare? {
        check(transferHex(sourceHash) && (expected == null || expected in 0..MAX_FILE)) { "FILE_LIMIT" }
        val provisional = IncomingShare(UUID.randomUUID().toString(), safeName(name), mime.take(128), expected ?: MAX_FILE, "", 0)
        synchronized(lock) {
            val existing = read()
            check(existing.entries.size < 10 && existing.entries.sumOf { it.size } + provisional.size <= MAX_TOTAL && root.usableSpace > provisional.size + 8 * 1024 * 1024) { "FILE_LIMIT" }
            save(existing.copy(entries = existing.entries + provisional, needsReview = true))
        }
        val directory = directory(provisional.id)
        var complete = false
        try {
            val hash = MessageDigest.getInstance("SHA-256")
            val buffer = ByteArray(PART)
            var size = 0L; var parts = 0
            try {
                while (true) {
                    check(current()) { "CANCELLED" }
                    var count = 0
                    while (count < buffer.size) {
                        check(current()) { "CANCELLED" }
                        val read = input.read(buffer, count, buffer.size - count)
                        if (read < 0) break
                        if (read == 0) continue
                        count += read
                    }
                    if (count == 0) break
                    size += count; check(size <= (expected ?: MAX_FILE)) { "FILE_LIMIT" }
                    hash.update(buffer, 0, count)
                    val bytes = if (count == buffer.size) buffer else buffer.copyOf(count)
                    try { synchronized(lock) {
                        check(read().entries.any { it.id == provisional.id }) { "CANCELLED" }
                        atomic(File(directory, parts.toString()), box.seal(provisional.id + ":" + parts, bytes))
                    } }
                    finally { if (bytes !== buffer) bytes.fill(0) }
                    parts++
                }
            } finally { buffer.fill(0) }
            check(current() && (expected == null || size == expected)) { "SOURCE_CHANGED" }
            val digest = hash.digest().joinToString("") { "%02x".format(it) }
            val fingerprint = CipherTransferStore.digest((sourceHash + ":" + digest + ":" + provisional.name).toByteArray())
            val now = System.currentTimeMillis()
            synchronized(lock) {
                val existing = read()
                check(existing.entries.any { it.id == provisional.id }) { "CANCELLED" }
                val receipts = existing.receipts.filter { now >= it.at && now - it.at < 86_400_000 }
                if (receipts.any { it.fingerprint == fingerprint }) {
                    save(existing.copy(entries = existing.entries.filterNot { it.id == provisional.id }, receipts = receipts)); return null
                }
                val item = provisional.copy(size = size, digest = digest, parts = parts)
                validate(item)
                save(existing.copy(entries = existing.entries.map { if (it.id == item.id) item else it }, receipts = (receipts + SharedReceipt(fingerprint, item.id, now)).takeLast(256)))
                complete = true
                return item
            }
        } finally {
            if (!complete) {
                directory.deleteRecursively()
                synchronized(lock) {
                    val value = read()
                    save(value.copy(entries = value.entries.filterNot { it.id == provisional.id }))
                }
            }
        }
    }
    fun claim(id: String, account: String, folder: String): IncomingShare = synchronized(lock) {
        check(validAccount(account) && folder.length in 1..128) { "INVALID_INPUT" }
        val value = read()
        val item = value.entries.single { it.id == id }
        check(item.complete && (item.account == null || item.account == account)) { "WRONG_ACCOUNT" }
        if (item.account != null) return item // A retry keeps the original destination.
        val chosen = item.copy(account = account, folder = folder)
        save(value.copy(entries = value.entries.map { if (it.id == id) chosen else it })); chosen
    }
    fun materialize(item: IncomingShare, destination: File, current: () -> Boolean) = synchronized(lock) {
        validate(item)
        check(item.complete && read().entries.any { it == item }) { "NOT_FOUND" }
        var finished = false
        try {
            val hash = MessageDigest.getInstance("SHA-256"); var size = 0L
            FileOutputStream(destination).use { output ->
                for (part in 0 until item.parts) {
                    check(current()) { "CANCELLED" }
                    val bytes = plainPart(item, part)
                    try { size += bytes.size; check(size <= item.size) { "INTEGRITY_FAILED" }; hash.update(bytes); output.write(bytes) }
                    finally { bytes.fill(0) }
                }
                output.fd.sync()
            }
            check(current() && size == item.size && hash.digest().joinToString("") { "%02x".format(it) } == item.digest) { "INTEGRITY_FAILED" }
            finished = true
        } finally { if (!finished) destination.delete() }
    }
    fun preview(item: IncomingShare): String? = synchronized(lock) {
        if (!item.complete || !item.mime.startsWith("text/") || item.parts == 0 || read().entries.none { it == item }) return null
        val bytes = plainPart(item, 0)
        try { bytes.copyOf(minOf(bytes.size, 128 * 1024)).toString(Charsets.UTF_8) } finally { bytes.fill(0) }
    }
    fun image(item: IncomingShare): ByteArray = synchronized(lock) {
        check(item.complete && item.mime.startsWith("image/") && item.size <= 8 * 1024 * 1024 && read().entries.any { it == item }) { "FILE_LIMIT" }
        val output = java.io.ByteArrayOutputStream(item.size.toInt())
        try {
            for (part in 0 until item.parts) {
                val bytes = plainPart(item, part)
                try { check(output.size() + bytes.size <= item.size); output.write(bytes) } finally { bytes.fill(0) }
            }
            output.toByteArray().also { check(it.size.toLong() == item.size && CipherTransferStore.digest(it) == item.digest) { "INTEGRITY_FAILED" } }
        } finally { output.close() }
    }
    private fun plainPart(item: IncomingShare, part: Int): ByteArray {
        check(part in 0 until item.parts)
        val file = File(directory(item.id), part.toString())
        check(file.isFile && file.length() in 28..PART.toLong() + 28 && file.canonicalFile.parentFile == directory(item.id).canonicalFile) { "INTEGRITY_FAILED" }
        return box.open(item.id + ":" + part, file.readBytes()).also { check(it.size <= PART) { "INTEGRITY_FAILED" } }
    }
    fun remove(id: String) = synchronized(lock) {
        val value = read(); check(uuid(id))
        save(value.copy(entries = value.entries.filterNot { it.id == id }))
        check(directory(id).deleteRecursively()) { "FILE_UNAVAILABLE" }
    }
    fun removeAccount(account: String) = synchronized(lock) { read().entries.filter { it.account == account }.forEach { remove(it.id) } }
    private fun directory(id: String): File {
        check(uuid(id))
        val file = File(root, id); check(file.canonicalFile == File(root.canonicalFile, id)) { "FORBIDDEN" }
        check(file.mkdirs() || file.isDirectory); return file
    }
    private fun validate(item: IncomingShare) {
        check(uuid(item.id) && item.name == safeName(item.name) && item.mime.length <= 128 && item.size in 0..MAX_FILE && item.parts in 0..100 && (item.digest.isEmpty() || transferHex(item.digest)) && (item.account == null || validAccount(item.account)) && (item.account == null) == (item.folder == null)) { "INTEGRITY_FAILED" }
    }
    companion object {
        const val PART = 1024 * 1024
        const val MAX_FILE = 100L * 1024 * 1024
        const val MAX_TOTAL = 200L * 1024 * 1024
        private val lock = Any()
        private fun uuid(value: String) = runCatching { UUID.fromString(value).toString() == value }.getOrDefault(false)
        fun safeName(value: String) = value.replace(Regex("[\\\\/\\p{Cntrl}]"), "_").take(180).takeIf { it.isNotBlank() && it != "." && it != ".." } ?: "Shared file"
        private fun atomic(file: File, bytes: ByteArray) {
            val temp = File(file.parentFile, file.name + ".new")
            try { FileOutputStream(temp).use { it.write(bytes); it.fd.sync() }; Files.move(temp.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING) }
            finally { temp.delete() }
        }
    }
}
