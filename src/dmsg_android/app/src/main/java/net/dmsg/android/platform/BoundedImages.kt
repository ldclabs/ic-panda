package net.dmsg.android.platform

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64

/** Decode core-produced image bytes within a fixed source and display budget. */
object BoundedImages {
    fun thumbnail(encoded: String, edge: Int = 320): Bitmap? {
        require(edge in 1..1600)
        if (encoded.length > 12 * 1024 * 1024) return null
        val bytes = Base64.decode(encoded, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
        try {
            if (bytes.size > 8 * 1024 * 1024) return null
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            if (bounds.outWidth <= 0 || bounds.outHeight <= 0 || bounds.outWidth.toLong() * bounds.outHeight > 40_000_000) return null
            var sample = 1
            while (bounds.outWidth / sample > edge || bounds.outHeight / sample > edge) sample *= 2
            return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
        } finally { bytes.fill(0) }
    }
}
