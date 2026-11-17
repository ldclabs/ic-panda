package net.dmsg.android.platform

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.os.UserManager
import android.os.CancellationSignal
import net.dmsg.android.BuildConfig
import androidx.core.content.FileProvider
import android.content.Intent
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.InternalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

class PrivateFiles(private val context: Context) {
    val root = File(context.noBackupFilesDir, "dmsg/" + if (BuildConfig.IS_STAGING) "staging" else "production")
    val workspace = File(root, "workspace")
    val staging = File(root, "staging")
    val credentials = File(root, "credentials")
    val shared = File(context.cacheDir, "dmsg-shared")
    val installation: String
    private val epoch = AtomicLong()
    private val streams = ConcurrentHashMap<String, InputStream>()
    private val outputs = ConcurrentHashMap<String, OutputStream>()
    private val signals = ConcurrentHashMap<String, CancellationSignal>()
    private val cleanup = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    init {
        check(context.getSystemService(UserManager::class.java).isUserUnlocked) { "DEVICE_LOCKED" }
        listOf(root, workspace, staging, credentials, shared).forEach { check(it.mkdirs() || it.isDirectory) }
        val marker = File(root, "installation")
        if (!marker.exists()) {
            // Never adopt a restored workspace under a fresh installation identity.
            check(workspace.listFiles().isNullOrEmpty()) { "INSTALLATION_MISMATCH" }
            atomicWrite(marker, UUID.randomUUID().toString().toByteArray())
        }
        installation = marker.readText().also { require(UUID.fromString(it).toString() == it) }
        cleanShared()
        val now = System.currentTimeMillis()
        staging.listFiles()?.filter { it.isDirectory }?.forEach { account ->
            account.listFiles()?.filter { it.isDirectory && it.name.startsWith("dmsg-import-") && now - it.lastModified() > 24 * 60 * 60_000L }?.forEach { it.deleteRecursively() }
        }
    }

    @OptIn(InternalCoroutinesApi::class)
    suspend fun stage(uri: Uri, account: String, maxBytes: Long = MAX_FILE_BYTES, current: () -> Boolean): String {
        require(uri.scheme == "content") { "FILE_UNAVAILABLE" }
        require(account.matches(Regex("[0-9a-v]{20}")))
        require(maxBytes in 1..MAX_FILE_BYTES)
        val at = epoch.get()
        val job = UUID.randomUUID().toString()
        val signal = CancellationSignal().also { signals[job] = it }
        val task = currentCoroutineContext()[Job]
        val cancelled = task?.invokeOnCompletion(onCancelling = true, invokeImmediately = true) { cause ->
            if (cause != null) { val input = streams[job]; cleanup.launch { runCatching { signal.cancel() }; runCatching { input?.close() } } }
        }
        val owner = File(staging, account)
        val directory = File(owner, "dmsg-import-$job")
        try {
            check(owner.mkdirs() || owner.isDirectory)
            check(at == epoch.get() && current() && task?.isActive != false) { "CANCELLED" }
            val name = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null, signal)?.use {
                if (!it.moveToFirst()) null else {
                    if (!it.isNull(1)) check(it.getLong(1) <= maxBytes) { "FILE_LIMIT" }
                    it.getString(0)
                }
            }?.replace(Regex("[\\\\/\\p{Cntrl}]"), "_")?.take(180)?.takeIf { it.isNotBlank() && it != "." && it != ".." } ?: "imported-file"
            check(directory.mkdir())
            val destination = File(directory, name)
            context.contentResolver.openAssetFileDescriptor(uri, "r", signal)?.use { descriptor -> descriptor.createInputStream().use { input ->
                streams[job] = input
                destination.outputStream().use { output ->
                    val buffer = ByteArray(64 * 1024)
                    var total = 0L
                    try { while (true) {
                        if (at != epoch.get() || !current() || task?.isActive == false) throw kotlinx.coroutines.CancellationException()
                        val count = input.read(buffer)
                        if (count < 0) break
                        total += count
                        check(total <= maxBytes && root.usableSpace > count + 8 * 1024 * 1024) { "FILE_LIMIT" }
                        if (at != epoch.get() || !current()) throw kotlinx.coroutines.CancellationException()
                        output.write(buffer, 0, count)
                    } } finally { buffer.fill(0) }
                }
            } } ?: error("FILE_UNAVAILABLE")
            check(at == epoch.get() && current()) { "CANCELLED" }
            return account + "/" + directory.name + "/" + destination.name
        } catch (error: Throwable) {
            directory.deleteRecursively()
            throw error
        } finally { cancelled?.dispose(); streams.remove(job); signals.remove(job) }
    }

    fun finishImport(handle: String) {
        val file = File(staging, handle).canonicalFile
        require(file.parentFile?.parentFile?.parentFile == staging.canonicalFile)
        file.parentFile?.deleteRecursively()
    }

    data class Capture(val uri: Uri, val file: File)
    fun capture(account: String): Capture {
        require(account.matches(Regex("[0-9a-v]{20}")))
        val directory = File(shared, "$account/capture-${UUID.randomUUID()}").apply { check(mkdirs()) }
        val file = File(directory, "Photo-${System.currentTimeMillis()}.jpg").apply { check(createNewFile()) }
        return Capture(FileProvider.getUriForFile(context, context.packageName + ".files", file), file)
    }
    fun finishCapture(capture: Capture) {
        require(capture.file.canonicalFile.toPath().startsWith(shared.canonicalFile.toPath()))
        runCatching { context.revokeUriPermission(capture.uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION) }
        capture.file.parentFile?.deleteRecursively()
    }

    fun shareCopy(path: String, account: String, current: () -> Boolean): File {
        require(account.matches(Regex("[0-9a-v]{20}")))
        val source = workspaceFile(path)
        val at = epoch.get()
        val owner = File(shared, account).apply { check(mkdirs() || isDirectory) }
        val job = UUID.randomUUID().toString()
        val directory = File(owner, job).apply { check(mkdir()) }
        val destination = File(directory, source.name)
        try {
            source.inputStream().use { input ->
                streams[job] = input
                destination.outputStream().use { output ->
                    val buffer = ByteArray(64 * 1024)
                    try { while (true) {
                        check(at == epoch.get() && current()) { "CANCELLED" }
                        val read = input.read(buffer); if (read < 0) break
                        check(at == epoch.get() && current()) { "CANCELLED" }
                        output.write(buffer, 0, read)
                    } } finally { buffer.fill(0) }
                }
            }
            check(at == epoch.get() && current()) { "CANCELLED" }
            return destination
        } catch (error: Throwable) { directory.deleteRecursively(); throw error }
        finally { streams.remove(job) }
    }

    fun workspaceFile(path: String): File = File(path).canonicalFile.also { source ->
        require(source.toPath().startsWith(workspace.canonicalFile.toPath()))
        require(source.isFile && source.length() <= MAX_FILE_BYTES)
    }

    @OptIn(InternalCoroutinesApi::class)
    suspend fun exportCopy(path: String, destination: Uri, current: () -> Boolean) {
        require(destination.scheme == "content") { "FILE_UNAVAILABLE" }
        val source = workspaceFile(path)
        val at = epoch.get()
        val job = UUID.randomUUID().toString()
        val signal = CancellationSignal().also { signals[job] = it }
        val task = currentCoroutineContext()[Job]
        val cancelled = task?.invokeOnCompletion(onCancelling = true, invokeImmediately = true) { cause ->
            if (cause != null) {
                val input = streams[job]; val output = outputs[job]
                cleanup.launch { runCatching { signal.cancel() }; runCatching { input?.close() }; runCatching { output?.close() } }
            }
        }
        try {
            check(at == epoch.get() && current() && task?.isActive != false) { "CANCELLED" }
            context.contentResolver.openFileDescriptor(destination, "wt", signal)?.use { descriptor ->
                android.os.ParcelFileDescriptor.AutoCloseOutputStream(descriptor).use { output ->
                    outputs[job] = output
                    source.inputStream().use { input ->
                        streams[job] = input
                        val buffer = ByteArray(64 * 1024)
                        try {
                            while (true) {
                                if (at != epoch.get() || !current() || task?.isActive == false) throw kotlinx.coroutines.CancellationException()
                                val count = input.read(buffer); if (count < 0) break
                                if (at != epoch.get() || !current()) throw kotlinx.coroutines.CancellationException()
                                output.write(buffer, 0, count)
                            }
                            output.flush()
                        } finally { buffer.fill(0) }
                    }
                }
            } ?: error("FILE_UNAVAILABLE")
            check(at == epoch.get() && current() && task?.isActive != false) { "CANCELLED" }
        } finally { cancelled?.dispose(); signals.remove(job); streams.remove(job); outputs.remove(job) }
    }

    /** Only called for a destination just created by the app's document picker. */
    fun removeIncompleteExport(uri: Uri) {
        if (uri.scheme == "content") runCatching { android.provider.DocumentsContract.deleteDocument(context.contentResolver, uri) }
    }

    /** Invalidate immediately; cancel/close only captured work away from the UI thread. */
    fun fenceIO(): () -> Unit {
        epoch.incrementAndGet()
        val oldSignals = signals.values.toList()
        val oldStreams = streams.values.toList()
        val oldOutputs = outputs.values.toList()
        val oldShares = shared.listFiles().orEmpty().flatMap { it.listFiles().orEmpty().toList() }
        return {
            oldSignals.forEach { runCatching { it.cancel() } }
            oldStreams.forEach { runCatching { it.close() } }
            oldOutputs.forEach { runCatching { it.close() } }
            oldShares.forEach { directory -> revokeCopies(directory); directory.deleteRecursively() }
        }
    }
    private fun revokeCopies(directory: File) {
        directory.walkBottomUp().filter { it.isFile }.forEach { file ->
            runCatching { context.revokeUriPermission(FileProvider.getUriForFile(context, context.packageName + ".files", file), Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION) }
        }
    }
    fun cleanShared() {
        revokeCopies(shared)
        shared.listFiles()?.forEach { it.deleteRecursively() }
    }

    companion object {
        const val MAX_FILE_BYTES = 1000L * 1024 * 1024
        fun atomicWrite(file: File, bytes: ByteArray) {
            val temporary = File(file.parentFile, ".${file.name}.${UUID.randomUUID()}.tmp")
            try {
                temporary.outputStream().use { it.write(bytes); it.fd.sync() }
                check(temporary.renameTo(file)) { "STORAGE_UNAVAILABLE" }
            } finally { temporary.delete() }
        }
    }
}
