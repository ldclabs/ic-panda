package net.dmsg.android.platform

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ImageDecoder
import java.io.ByteArrayOutputStream
import java.io.File

/** ImageDecoder applies orientation; the new JPEG contains only reviewed pixels. */
object ProfileImages {
    fun prepare(file: File): ByteArray {
        check(file.length() in 1..32L * 1024 * 1024) { "FILE_LIMIT" }
        val decoded = BoundedImages.decode(ImageDecoder.createSource(file), 512)
        try {
            val opaque = Bitmap.createBitmap(decoded.width, decoded.height, Bitmap.Config.ARGB_8888)
            try {
                Canvas(opaque).apply { drawColor(Color.WHITE); drawBitmap(decoded, 0f, 0f, null) }
                return ByteArrayOutputStream().use { output ->
                    check(opaque.compress(Bitmap.CompressFormat.JPEG, 85, output)) { "FILE_UNAVAILABLE" }
                    output.toByteArray().also { check(it.size <= 512 * 1024) { "FILE_LIMIT" } }
                }
            } finally { opaque.recycle() }
        } finally { decoded.recycle() }
    }
}
