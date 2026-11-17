package net.dmsg.android.platform

import android.graphics.Bitmap
import android.graphics.ImageDecoder
import java.nio.ByteBuffer

object IncomingImages {
    fun decode(bytes: ByteArray): Bitmap {
        check(bytes.size <= 8 * 1024 * 1024) { "FILE_LIMIT" }
        return ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
            val width = info.size.width; val height = info.size.height
            check(width > 0 && height > 0 && width.toLong() * height <= 40_000_000) { "FILE_LIMIT" }
            val scale = minOf(1.0, 640.0 / maxOf(width, height))
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.setTargetSize(maxOf(1, (width * scale).toInt()), maxOf(1, (height * scale).toInt()))
        }
    }
}
