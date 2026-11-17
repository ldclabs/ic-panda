package net.dmsg.android.platform

import kotlinx.serialization.json.*
import net.dmsg.core.PasskeyOperation
import net.dmsg.core.PasskeyStart
import org.junit.Assert.*
import org.junit.Test

class PasskeyWireTest {
    private val certificate = ByteArray(32) { 0x35 }
    private fun start(operation: PasskeyOperation = PasskeyOperation.LOGIN, credential: ByteArray? = null) =
        PasskeyStart(ByteArray(32) { 1 }, ByteArray(32) { 9 }, ByteArray(32) { 3 }, credential, operation, 1800000300000u)

    private fun response(start: PasskeyStart, origin: String = "android:apk-key-hash:" + PasskeyWire.encode(certificate)): Pair<String, ByteArray> {
        val client = buildJsonObject {
            put("origin", origin); put("type", if (PasskeyWire.creating(start.operation)) "webauthn.create" else "webauthn.get")
            put("challenge", PasskeyWire.encode(start.challenge)); put("crossOrigin", false)
        }.toString().toByteArray()
        return buildJsonObject {
            put("type", "public-key"); put("rawId", "AQID"); put("id", "AQID")
            putJsonObject("response") {
                put("clientDataJSON", PasskeyWire.encode(client))
                if (PasskeyWire.creating(start.operation)) put("attestationObject", "AQID")
                else { put("authenticatorData", "AQID"); put("signature", "AQID") }
            }
        }.toString() to client
    }

    @Test fun signedClientDataIsPreservedAndForeignOriginsAreRejected() {
        val start = start()
        val (raw, original) = response(start)
        assertArrayEquals(original, PasskeyWire.response(raw, start, certificate).clientDataJson)
        for (origin in listOf("https://dmsg.net", "android:apk-key-hash:unknown", "android:apk-key-hash:" + PasskeyWire.encode(certificate) + "=")) {
            assertThrows(IllegalStateException::class.java) { PasskeyWire.response(response(start, origin).first, start, certificate) }
        }
        assertThrows(IllegalStateException::class.java) { PasskeyWire.response(raw, start(credential = byteArrayOf(5)), certificate) }
    }

    @Test fun creationRequiresDiscoverableEs256AndRequiredUv() {
        val start = start(PasskeyOperation.REGISTER)
        val request = Json.parseToJsonElement(PasskeyWire.request(start, 1800000000000u)).jsonObject
        assertEquals("dmsg.net", request["rp"]!!.jsonObject["id"]!!.jsonPrimitive.content)
        assertEquals("required", request["authenticatorSelection"]!!.jsonObject["residentKey"]!!.jsonPrimitive.content)
        assertEquals("required", request["authenticatorSelection"]!!.jsonObject["userVerification"]!!.jsonPrimitive.content)
        assertEquals(-7, request["pubKeyCredParams"]!!.jsonArray.single().jsonObject["alg"]!!.jsonPrimitive.int)
        assertEquals(0, PasskeyWire.response(response(start).first, start, certificate).signature.size)
        assertThrows(IllegalArgumentException::class.java) { PasskeyWire.request(start, start.deadlineMs) }
    }

    @Test fun resetSessionsRestrictsTheSelectedCredential() {
        val start = start(PasskeyOperation.RESET_SESSIONS, byteArrayOf(1,2,3))
        val request = Json.parseToJsonElement(PasskeyWire.request(start, 1800000000000u)).jsonObject
        assertEquals("AQID", request["allowCredentials"]!!.jsonArray.single().jsonObject["id"]!!.jsonPrimitive.content)
        assertEquals("required", request["userVerification"]!!.jsonPrimitive.content)
    }
}
