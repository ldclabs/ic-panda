package net.dmsg.android.platform

import java.io.ByteArrayInputStream
import org.junit.Assert.*
import org.junit.Test

class BoundedInputTest {
    @Test fun shortReadsPreservePartBoundariesAndDoNotConsumeTheNextPart() {
        val source = ByteArray(19) { it.toByte() }
        val stream = object : ByteArrayInputStream(source) {
            override fun read(buffer: ByteArray, offset: Int, length: Int) = super.read(buffer, offset, minOf(3, length))
        }
        assertArrayEquals(source.copyOfRange(0, 8), stream.readChunk(8))
        assertArrayEquals(source.copyOfRange(8, 16), stream.readChunk(8))
        assertArrayEquals(source.copyOfRange(16, 19), stream.readChunk(8))
        assertTrue(stream.readChunk(8).isEmpty())
    }
}
