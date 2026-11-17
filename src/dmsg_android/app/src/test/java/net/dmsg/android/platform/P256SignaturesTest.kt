package net.dmsg.android.platform
import org.junit.Assert.*
import org.junit.Test
import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.Signature
import java.security.spec.ECGenParameterSpec

class P256SignaturesTest {
    @Test fun realEcdsaSignaturesRemainVerifiableAfterLowSNormalization() {
        val pair = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
        val message = ByteArray(32) { it.toByte() }
        repeat(12) {
            val original = Signature.getInstance("SHA256withECDSA").run { initSign(pair.private); update(message); sign() }
            val raw = P256Signatures.raw(original)
            assertEquals(64, raw.size)
            val r = BigInteger(1, raw.copyOfRange(0, 32)).toByteArray()
            val s = BigInteger(1, raw.copyOfRange(32, 64)).toByteArray()
            val der = byteArrayOf(0x30, (4+r.size+s.size).toByte(), 2, r.size.toByte()) + r + byteArrayOf(2,s.size.toByte()) + s
            assertTrue(Signature.getInstance("SHA256withECDSA").run { initVerify(pair.public); update(message); verify(der) })
            assertTrue(BigInteger(1, raw.copyOfRange(32,64)) <= BigInteger("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551",16).shiftRight(1))
        }
    }
    @Test fun zeroNegativeNoncanonicalAndTrailingIntegersAreRejected() {
        for (der in listOf(
            byteArrayOf(0x30,6,2,1,0,2,1,1),
            byteArrayOf(0x30,6,2,1,0x80.toByte(),2,1,1),
            byteArrayOf(0x30,7,2,2,0,1,2,1,1),
            byteArrayOf(0x30,7,2,1,1,2,1,1,0)
        )) assertThrows(IllegalArgumentException::class.java) { P256Signatures.raw(der) }
    }
}
