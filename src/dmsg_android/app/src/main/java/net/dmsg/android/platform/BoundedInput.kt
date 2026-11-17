package net.dmsg.android.platform

import java.io.InputStream

/** Reads a bounded part across short reads, including on Android 12. */
internal fun InputStream.readChunk(limit: Int): ByteArray {
    require(limit >= 0)
    val bytes = ByteArray(limit)
    var size = 0
    try {
        while (size < limit) {
            val count = read(bytes, size, limit - size)
            if (count < 0) break
            if (count == 0) {
                val next = read()
                if (next < 0) break
                bytes[size++] = next.toByte()
            } else size += count
        }
        return if (size == limit) bytes else bytes.copyOf(size).also { bytes.fill(0) }
    } catch (error: Throwable) { bytes.fill(0); throw error }
}
