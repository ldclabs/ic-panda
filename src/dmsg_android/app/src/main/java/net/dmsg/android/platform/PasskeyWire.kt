package net.dmsg.android.platform

import kotlinx.serialization.json.*
import net.dmsg.core.NativePasskeyResponse
import net.dmsg.core.PasskeyOperation
import net.dmsg.core.PasskeyStart
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Base64
import java.util.Locale

/** A transport for signed WebAuthn bytes, never an origin-rewriting adapter. */
object PasskeyWire {
    private val credentialNameFormat = DateTimeFormatter.ofPattern("'dMsg-'MM-dd-HHmm", Locale.ROOT)

    fun encode(bytes: ByteArray): String = Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    fun creating(operation: PasskeyOperation) = operation == PasskeyOperation.REGISTER || operation == PasskeyOperation.BIND

    fun request(start: PasskeyStart, now: ULong): String {
        require(start.challenge.size == 32 && start.state.size == 32 && start.deadlineMs > now) { "EXPIRED" }
        val timeout = (start.deadlineMs - now).coerceAtMost(300_000u)
        return buildJsonObject {
            put("challenge", encode(start.challenge))
            put("timeout", timeout.toLong())
            if (creating(start.operation)) {
                require(start.userId.size in 1..64 && start.credentialId == null) { "INTEGRITY_FAILED" }
                putJsonObject("rp") { put("id", "dmsg.net"); put("name", "dMsg") }
                putJsonObject("user") {
                    put("id", encode(start.userId))
                    val name = credentialNameFormat.format(Instant.ofEpochMilli(now.toLong()).atZone(ZoneId.systemDefault()))
                    put("name", name); put("displayName", name)
                }
                putJsonArray("pubKeyCredParams") { add(buildJsonObject { put("type", "public-key"); put("alg", -7) }) }
                put("attestation", "none")
                putJsonObject("authenticatorSelection") {
                    put("residentKey", "required"); put("requireResidentKey", true); put("userVerification", "required")
                }
            } else {
                put("rpId", "dmsg.net"); put("userVerification", "required")
                putJsonArray("allowCredentials") {
                    start.credentialId?.let { id ->
                        require(id.size in 1..1024) { "INTEGRITY_FAILED" }
                        add(buildJsonObject { put("type", "public-key"); put("id", encode(id)) })
                    }
                }
            }
        }.toString()
    }

    fun response(raw: String, start: PasskeyStart, signingHistory: List<ByteArray>): NativePasskeyResponse {
        require(raw.length <= 24 * 1024 && signingHistory.isNotEmpty() && signingHistory.all { it.size == 32 }) { "INTEGRITY_FAILED" }
        val root = Json.parseToJsonElement(raw).jsonObject
        fun JsonObject.string(key: String): String {
            val value = this[key] as? JsonPrimitive ?: error("INTEGRITY_FAILED")
            check(value.isString) { "INTEGRITY_FAILED" }; return value.content
        }
        fun JsonObject.bytes(key: String, maximum: Int): ByteArray {
            val encoded = string(key)
            check(encoded.length <= (maximum * 4 + 2) / 3) { "INTEGRITY_FAILED" }
            val decoded = Base64.getUrlDecoder().decode(encoded)
            check(decoded.size in 1..maximum && encode(decoded) == encoded) { "INTEGRITY_FAILED" }
            return decoded
        }
        check(root.string("type") == "public-key") { "INTEGRITY_FAILED" }
        val credential = root.bytes("rawId", 1024)
        check(root.string("id") == encode(credential)) { "INTEGRITY_FAILED" }
        start.credentialId?.let { check(it.contentEquals(credential)) { "FORBIDDEN" } }
        val payload = root["response"]?.jsonObject ?: error("INTEGRITY_FAILED")
        val client = payload.bytes("clientDataJSON", 4096)
        val data = Json.parseToJsonElement(client.decodeToString(throwOnInvalidSequence = true)).jsonObject
        check(data.string("type") == if (creating(start.operation)) "webauthn.create" else "webauthn.get") { "INTEGRITY_FAILED" }
        val origin = data.string("origin")
        check(signingHistory.any { origin == "android:apk-key-hash:" + encode(it) }) { "PASSKEY_ORIGIN_MISMATCH" }
        check(data.string("challenge") == encode(start.challenge)) { "INTEGRITY_FAILED" }
        check(data["topOrigin"] == null || data["topOrigin"] == JsonNull) { "INTEGRITY_FAILED" }
        check(data["crossOrigin"] == null || data["crossOrigin"] == JsonPrimitive(false)) { "INTEGRITY_FAILED" }
        return NativePasskeyResponse(credential, client,
            payload.bytes(if (creating(start.operation)) "attestationObject" else "authenticatorData", 8192),
            if (creating(start.operation)) byteArrayOf() else payload.bytes("signature", 80))
    }
}
