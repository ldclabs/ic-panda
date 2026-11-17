package net.dmsg.android.platform

import java.math.BigInteger

internal object P256Signatures {
    private val order = BigInteger("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551", 16)
    fun scalar(value: BigInteger): ByteArray {
        require(value.signum() >= 0 && value.bitLength() <= 256)
        val bytes = value.toByteArray()
        return if (bytes.size > 32) bytes.copyOfRange(bytes.size - 32, bytes.size) else ByteArray(32 - bytes.size) + bytes
    }
    fun raw(der: ByteArray): ByteArray {
        require(der.size in 8..72 && der[0] == 0x30.toByte() && (der[1].toInt() and 0xff) == der.size - 2)
        var offset = 2
        fun integer(): BigInteger {
            require(offset + 2 <= der.size && der[offset++] == 2.toByte())
            val size = der[offset++].toInt() and 0xff
            require(size in 1..33 && offset + size <= der.size)
            val bytes = der.copyOfRange(offset, offset + size); offset += size
            require((bytes[0].toInt() and 0x80) == 0)
            require(size == 1 || bytes[0] != 0.toByte() || (bytes[1].toInt() and 0x80) != 0)
            return BigInteger(1, bytes).also { require(it.signum() > 0 && it < order) }
        }
        val r = integer(); val s = integer()
        require(offset == der.size)
        return scalar(r) + scalar(if (s > order.shiftRight(1)) order - s else s)
    }
}
