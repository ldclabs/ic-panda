package net.dmsg.android.core

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import net.dmsg.android.platform.PrivateFiles

data class VerifiedFilePreview(val file: LibraryFile, val kind: String, val text: String? = null, val truncated: Boolean = false, val bitmap: Bitmap? = null)

/** Native open verifies the complete immutable version before any preview bytes are read. */
class FilePreviewCoordinator(private val client: ClientController, private val paths: () -> PrivateFiles?) {
    val preview = ReadStore<VerifiedFilePreview>()
    fun open(id: String, version: String) {
        val files = paths() ?: return
        client.clearNotice()
        val request = preview.begin(false)
        client.perform({ core ->
            val detail = parseFileDetails(core.fileDetails(id).decode())
            val selected = detail.versions.singleOrNull { it.id == version } ?: error("NOT_FOUND")
            check(selected.local && detail.file.trashedAt == null) { "NOT_LOCAL" }
            val file = detail.file.copy(version = version, mime = selected.mime ?: (if (selected.current) detail.file.mime else "application/octet-stream"), size = selected.size, sha256 = selected.sha256, status = selected.status, backup = selected.backup)
            val opened = client.operation { core.openFile(id, version, it) }.decode().objectValue()
            val nonce = opened.required("nonce")
            try {
                check(opened.required("file") == id && opened.required("version") == version) { "INTEGRITY_FAILED" }
                val source = files.workspaceFile(opened.required("path"))
                check(source.length().toULong() == file.size) { "INTEGRITY_FAILED" }
                when {
                    file.mime.startsWith("text/") || file.mime in setOf("application/json", "application/xml", "application/yaml", "application/toml", "application/javascript") -> {
                        val bytes = ByteArray(128 * 1024)
                        try {
                            val length = source.inputStream().use { input ->
                                var length = 0
                                while (length < bytes.size) { val count = input.read(bytes, length, bytes.size - length); if (count < 0) break; length += count }
                                length
                            }
                            VerifiedFilePreview(file, "text", String(bytes, 0, length, Charsets.UTF_8), file.size > length.toULong())
                        }
                        finally { bytes.fill(0) }
                    }
                    file.mime.startsWith("image/") && file.size <= 32uL * 1024u * 1024u -> {
                        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                        BitmapFactory.decodeFile(source.path, bounds)
                        if (bounds.outWidth <= 0 || bounds.outHeight <= 0 || bounds.outWidth.toLong() * bounds.outHeight > 40_000_000) VerifiedFilePreview(file, "external")
                        else {
                            var sample = 1
                            while (bounds.outWidth / sample > 1600 || bounds.outHeight / sample > 1600) sample *= 2
                            val bitmap = BitmapFactory.decodeFile(source.path, BitmapFactory.Options().apply { inSampleSize = sample; inPreferredConfig = Bitmap.Config.ARGB_8888 })
                            if (bitmap == null) VerifiedFilePreview(file, "external") else VerifiedFilePreview(file, "image", bitmap = bitmap)
                        }
                    }
                    else -> VerifiedFilePreview(file, "external")
                }
            } finally { withContext(NonCancellable) { runCatching { core.closeFile(nonce) } } }
        }, key = "file:preview", replace = true, quiet = true, onError = { preview.fail(request, it) }) { preview.accept(request, it) }
    }
    fun clear() { client.cancelOperation("file:preview"); preview.reset() }
}
