package net.dmsg.android.platform

import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.util.Base64
import java.io.IOException
import java.nio.ByteBuffer

/** Decode oriented images within a fixed source and display budget. */
object BoundedImages {
    fun thumbnail(encoded: String, edge: Int = 320): Bitmap? {
        if (encoded.length > 12 * 1024 * 1024) return null
        val bytes = Base64.decode(encoded, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
        return try { thumbnail(bytes, edge) } finally { bytes.fill(0) }
    }

    fun thumbnail(bytes: ByteArray, edge: Int): Bitmap? {
        if (bytes.isEmpty() || bytes.size > 8 * 1024 * 1024) return null
        return preview(ImageDecoder.createSource(ByteBuffer.wrap(bytes)), edge)
    }

    fun preview(source: ImageDecoder.Source, edge: Int): Bitmap? = try {
        decode(source, edge)
    } catch (_: IOException) {
        null
    } catch (_: IllegalStateException) {
        null
    }

    fun decode(bytes: ByteArray, edge: Int): Bitmap {
        check(bytes.size <= 8 * 1024 * 1024) { "FILE_LIMIT" }
        return decode(ImageDecoder.createSource(ByteBuffer.wrap(bytes)), edge)
    }

    fun decode(source: ImageDecoder.Source, edge: Int): Bitmap {
        require(edge in 1..1600)
        return ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
            val width = info.size.width
            val height = info.size.height
            check(width > 0 && height > 0 && width.toLong() * height <= 40_000_000) { "FILE_LIMIT" }
            val scale = minOf(1.0, edge.toDouble() / maxOf(width, height))
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.setTargetSize(maxOf(1, (width * scale).toInt()), maxOf(1, (height * scale).toInt()))
        }
    }
}
