package net.dmsg.android.platform

import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import kotlinx.coroutines.*
import net.dmsg.core.scanQrLuma
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.util.concurrent.atomic.AtomicReference

/** Bounded, cancellable image reading; no image or QR content is uploaded. */
object QrImages {
    @OptIn(InternalCoroutinesApi::class)
    suspend fun read(context: Context, uri: Uri): String = withContext(Dispatchers.IO) {
        require(uri.scheme == "content") { "INTEGRITY_FAILED" }
        val signal = CancellationSignal()
        val stream = AtomicReference<InputStream?>()
        val job = currentCoroutineContext()[Job]
        val cancel = job?.invokeOnCompletion(onCancelling = true, invokeImmediately = true) { error ->
            if (error != null) { signal.cancel(); runCatching { stream.getAndSet(null)?.close() } }
        }
        try {
            val descriptor = context.contentResolver.openFileDescriptor(uri, "r", signal) ?: error("FILE_UNAVAILABLE")
            val bytes = ParcelFileDescriptor.AutoCloseInputStream(descriptor).use { input ->
                stream.set(input)
                val output = ByteArrayOutputStream()
                val buffer = ByteArray(64 * 1024)
                try {
                    while (true) {
                        ensureActive()
                        val count = input.read(buffer)
                        if (count < 0) break
                        check(output.size() + count <= 8 * 1024 * 1024) { "FILE_LIMIT" }
                        output.write(buffer, 0, count)
                    }
                    output.toByteArray()
                } finally { buffer.fill(0); stream.compareAndSet(input, null) }
            }
            try {
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                check(bounds.outWidth > 0 && bounds.outHeight > 0 && bounds.outWidth.toLong() * bounds.outHeight <= 40_000_000) { "INVALID_QR_IMAGE" }
                var sample = 1
                while (bounds.outWidth / sample > 1600 || bounds.outHeight / sample > 1600) sample *= 2
                ensureActive()
                val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample }) ?: error("INVALID_QR_IMAGE")
                try {
                    check(bitmap.width.toLong() * bitmap.height <= 4_000_000) { "FILE_LIMIT" }
                    val row = IntArray(bitmap.width)
                    val luma = ByteArray(bitmap.width * bitmap.height)
                    try {
                        for (y in 0 until bitmap.height) {
                            ensureActive()
                            bitmap.getPixels(row, 0, bitmap.width, 0, y, bitmap.width, 1)
                            for (x in row.indices) { val c = row[x]; luma[y * bitmap.width + x] = ((77 * (c shr 16 and 255) + 150 * (c shr 8 and 255) + 29 * (c and 255)) shr 8).toByte() }
                        }
                        scanQrLuma(bitmap.width.toUInt(), bitmap.height.toUInt(), luma).also { ensureActive() }
                    } finally { row.fill(0); luma.fill(0) }
                } finally { bitmap.recycle() }
            } finally { bytes.fill(0) }
        } finally { cancel?.dispose(); stream.getAndSet(null)?.close() }
    }
}
