package net.dmsg.android.platform

import android.app.KeyguardManager
import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.os.SystemClock
import android.security.keystore.*
import net.dmsg.core.HardwareSecret
import net.dmsg.core.MobileException
import net.dmsg.core.Platform
import net.dmsg.core.SigningKey
import org.json.JSONObject
import java.io.File
import java.security.*
import java.security.spec.ECGenParameterSpec
import java.security.spec.MGF1ParameterSpec
import java.security.spec.X509EncodedKeySpec
import java.security.interfaces.ECPublicKey
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicBoolean
import javax.crypto.*
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.OAEPParameterSpec
import javax.crypto.spec.PSource

/** Authentication prepares single-use Keystore operations outside Client. */
class AndroidSecurity(
    private val context: Context,
    private val files: PrivateFiles,
    private val prompts: PresencePrompts,
) : Platform {
    private val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private val prefix = "dmsg." + context.packageName + "." + files.installation + "."
    private val storageAlias = prefix + "storage"
    private val cancellation = AtomicLong()
    private val sequence = AtomicLong()
    private val prepared = mutableMapOf<ULong, PreparedSigning>()
    @Volatile var securityLevel = "Unavailable"
        private set
    private data class Handle(val alias: String, val kind: String, val only: Boolean)
    private data class SigningOperation(val signature: Signature, val publicKey: PublicKey) {
        fun discard() { runCatching { signature.initVerify(publicKey) } }
    }
    private class PreparedSigning(val epoch: Long, val deadline: Long, val reason: String, val only: Boolean,
        val message: ByteArray, val operation: SigningOperation) {
        val live = AtomicBoolean(true)
        val started = AtomicBoolean(false)
        private val discarded = AtomicBoolean(false)
        fun discard() { if (discarded.compareAndSet(false, true)) { message.fill(0); operation.discard() } }
    }

    override fun presence(): UByte {
        if (!context.getSystemService(KeyguardManager::class.java).isDeviceSecure) return 0u
        val manager = context.getSystemService(BiometricManager::class.java)
        if (manager.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL) != BiometricManager.BIOMETRIC_SUCCESS) return 0u
        return if (supportsBiometryOnly()) 2u else 1u
    }
    override fun supportsBiometryOnly(): Boolean =
        context.getSystemService(BiometricManager::class.java).canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS

    // Android exposes no enrollment digest. The explicit key policy enforces it.
    override fun biometryState(): ByteArray? = null
    private fun digest(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes)
    private fun record(name: String) = File(files.credentials, digest(name.toByteArray()).joinToString("") { "%02x".format(it) })
    private fun aad(scope: String) = ("dmsg/android-envelope/1:" + files.installation + ":" + scope).toByteArray()
    private fun encoded(handle: Handle) = JSONObject().put("v", 1).put("kind", handle.kind)
        .put("alias", handle.alias).put("biometry_only", handle.only).toString().toByteArray()

    private fun decode(bytes: ByteArray, kind: String, only: Boolean? = null): Handle {
        require(bytes.size in 1..1024)
        val obj = JSONObject(bytes.toString(Charsets.UTF_8))
        require(obj.getInt("v") == 1 && obj.getString("kind") == kind)
        val handle = Handle(obj.getString("alias"), kind, obj.getBoolean("biometry_only"))
        require(handle.alias.startsWith(prefix + kind + ".") && handle.alias.length < 240)
        require(only == null || handle.only == only)
        return handle
    }
    private fun hardware(info: KeyInfo) {
        securityLevel = when (info.securityLevel) {
            KeyProperties.SECURITY_LEVEL_STRONGBOX -> "StrongBox"
            KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> "TEE"
            else -> throw PresencePrompts.failure("HARDWARE_UNAVAILABLE")
        }
    }
    private fun privateKey(handle: Handle, authenticated: Boolean): PrivateKey {
        val key = synchronized(store) { store.getKey(handle.alias, null) as? PrivateKey }
            ?: throw PresencePrompts.failure("INTEGRITY_FAILED")
        val info = KeyFactory.getInstance(key.algorithm, "AndroidKeyStore").getKeySpec(key, KeyInfo::class.java)
        hardware(info)
        require(info.isUserAuthenticationRequired == authenticated)
        if (authenticated) {
            require(info.userAuthenticationValidityDurationSeconds <= 0)
            val types = if (handle.only) KeyProperties.AUTH_BIOMETRIC_STRONG else KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL
            require(info.userAuthenticationType == types)
        }
        return key
    }
    private fun pair(kind: String, only: Boolean, authentication: Boolean, rsa: Boolean = false): Pair<Handle, KeyPair> {
        if (authentication && (presence() == 0.toUByte() || only && !supportsBiometryOnly())) throw PresencePrompts.failure("HARDWARE_UNAVAILABLE")
        val handle = Handle(prefix + kind + "." + UUID.randomUUID(), kind, only)
        fun create(strongBox: Boolean): KeyPair {
            val builder = KeyGenParameterSpec.Builder(handle.alias, if (rsa) KeyProperties.PURPOSE_DECRYPT else KeyProperties.PURPOSE_SIGN)
                .setIsStrongBoxBacked(strongBox)
            if (rsa) builder.setKeySize(2048).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_RSA_OAEP).setDigests(KeyProperties.DIGEST_SHA256)
            else builder.setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1")).setDigests(KeyProperties.DIGEST_SHA256)
            if (authentication) builder.setUserAuthenticationRequired(true)
                .setUserAuthenticationParameters(0, if (only) KeyProperties.AUTH_BIOMETRIC_STRONG else KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL)
                .setInvalidatedByBiometricEnrollment(only)
            val generator = KeyPairGenerator.getInstance(if (rsa) "RSA" else "EC", "AndroidKeyStore")
            generator.initialize(builder.build())
            return generator.generateKeyPair()
        }
        try {
            val value = if (context.packageManager.hasSystemFeature("android.hardware.strongbox_keystore")) {
                try { create(true) } catch (_: StrongBoxUnavailableException) { create(false) }
            } else create(false)
            privateKey(handle, authentication)
            return handle to value
        } catch (error: Throwable) { synchronized(store) { store.deleteEntry(handle.alias) }; throw error }
    }
    @Synchronized private fun storageKey(create: Boolean): SecretKey {
        (store.getKey(storageAlias, null) as? SecretKey)?.let { return it }
        if (!create || !files.credentials.listFiles().isNullOrEmpty()) throw PresencePrompts.failure("INTEGRITY_FAILED")
        val generator = KeyGenerator.getInstance("AES", "AndroidKeyStore")
        generator.init(KeyGenParameterSpec.Builder(storageAlias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256).build())
        return generator.generateKey()
    }
    override fun get(name: String): ByteArray? = guarded {
        val file = record(name)
        if (!file.exists()) return@guarded null
        require(file.length() in 29..131072)
        val bytes = file.readBytes(); require(bytes[0] == 1.toByte())
        Cipher.getInstance("AES/GCM/NoPadding").run {
            init(Cipher.DECRYPT_MODE, storageKey(false), GCMParameterSpec(128, bytes.copyOfRange(1, 13)))
            updateAAD(aad(name)); doFinal(bytes, 13, bytes.size - 13)
        }
    }
    override fun set(name: String, secret: ByteArray) = guarded {
        require(secret.size <= 120 * 1024)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, storageKey(true)); updateAAD(aad(name)) }
        PrivateFiles.atomicWrite(record(name), byteArrayOf(1) + cipher.iv + cipher.doFinal(secret))
    }
    override fun delete(name: String) {
        val file = record(name)
        if (file.exists() && !file.delete()) throw PresencePrompts.failure("PLATFORM_UNAVAILABLE")
    }

    // Public wrapping asks nothing. Private unwrap is auth-per-use. Both OAEP
    // digests are explicit, matching Android Keystore's MGF1 SHA-1 support.
    override fun createSecret(biometryOnly: Boolean): HardwareSecret = guarded {
        val (handle, pair) = pair("rsa-oaep-v1", biometryOnly, true, rsa = true)
        val bytes = encoded(handle)
        val secret = ByteArray(32).also { SecureRandom().nextBytes(it) }
        val payload = digest(bytes) + secret
        try {
            val publicKey = KeyFactory.getInstance("RSA").generatePublic(X509EncodedKeySpec(pair.public.encoded))
            val cipher = Cipher.getInstance("RSA/ECB/OAEPPadding")
            cipher.init(Cipher.ENCRYPT_MODE, publicKey, oaep)
            HardwareSecret(bytes, byteArrayOf(1) + cipher.doFinal(payload), secret.copyOf())
        } catch (error: Throwable) { synchronized(store) { store.deleteEntry(handle.alias) }; throw error }
        finally { secret.fill(0); payload.fill(0) }
    }
    override fun openSecret(handle: ByteArray, context: ByteArray, biometryOnly: Boolean): ByteArray = guarded {
        val epoch = cancellation.get()
        val parsed = decode(handle, "rsa-oaep-v1", biometryOnly)
        require(context.size == 257 && context[0] == 1.toByte())
        val cipher = Cipher.getInstance("RSA/ECB/OAEPPadding")
        cipher.init(Cipher.DECRYPT_MODE, privateKey(parsed, true), oaep)
        val authorized = prompts.cipher(str(net.dmsg.android.R.string.security_unlock), str(net.dmsg.android.R.string.security_workspace), cipher, biometryOnly) { epoch == cancellation.get() }
        val payload = authorized.doFinal(context, 1, context.size - 1)
        try {
            if (epoch != cancellation.get()) throw PresencePrompts.failure("CANCELLED")
            require(payload.size == 64 && MessageDigest.isEqual(payload.copyOfRange(0, 32), digest(handle)))
            payload.copyOfRange(32, 64)
        } finally { payload.fill(0) }
    }
    private fun publicBytes(key: PublicKey, compressed: Boolean): ByteArray {
        val ec = key as ECPublicKey; val x = P256Signatures.scalar(ec.w.affineX); val y = P256Signatures.scalar(ec.w.affineY)
        return if (compressed) byteArrayOf(if (ec.w.affineY.testBit(0)) 3 else 2) + x else byteArrayOf(4) + x + y
    }
    override fun createSigning(biometryOnly: Boolean): SigningKey = guarded {
        val (handle, pair) = pair("approval-p256-v1", biometryOnly, true)
        SigningKey(publicBytes(pair.public, true), encoded(handle))
    }
    override fun deleteSigning(handle: ByteArray) = guarded {
        require(handle.size in 1..1024)
        val kind = JSONObject(handle.toString(Charsets.UTF_8)).getString("kind")
        require(kind in setOf("approval-p256-v1", "rsa-oaep-v1", "anchor-p256-v1"))
        val key = decode(handle, kind)
        synchronized(store) { store.deleteEntry(key.alias) }
    }
    override fun createAnchor(): SigningKey = guarded {
        val (handle, pair) = pair("anchor-p256-v1", false, false)
        SigningKey(publicBytes(pair.public, false), encoded(handle))
    }
    override fun signAnchor(handle: ByteArray, message: ByteArray): ByteArray = guarded {
        require(message.isNotEmpty() && message.size <= 512)
        val key = privateKey(decode(handle, "anchor-p256-v1", false), false)
        P256Signatures.raw(Signature.getInstance("SHA256withECDSA").run { initSign(key); update(message); sign() })
    }
    override fun authorize(reason: String, biometryOnly: Boolean): ULong = throw PresencePrompts.failure("SIGNATURE_PREPARATION_REQUIRED")
    override fun prepareSignature(reason: String, biometryOnly: Boolean, handle: ByteArray, message: ByteArray): ULong = guarded {
        require(message.size == 32)
        val epoch = cancellation.get()
        val decoded = decode(handle, "approval-p256-v1")
        require(!decoded.only || biometryOnly)
        val key = privateKey(decoded, true)
        val public = synchronized(store) { store.getCertificate(decoded.alias).publicKey }
        val operation = SigningOperation(Signature.getInstance("SHA256withECDSA").apply { initSign(key) }, public)
        val item = PreparedSigning(epoch, SystemClock.elapsedRealtime() + 90_000, reason.take(100), biometryOnly, message.copyOf(), operation)
        synchronized(prepared) {
            if (epoch != cancellation.get()) { item.discard(); throw PresencePrompts.failure("CANCELLED") }
            if (prepared.size >= 64) { item.discard(); throw PresencePrompts.failure("QUOTA_EXCEEDED") }
            sequence.incrementAndGet().toULong().also { prepared[it] = item }
        }
    }
    override fun signPrepared(preparation: ULong): ByteArray = guarded {
        val item = synchronized(prepared) { prepared[preparation] } ?: throw PresencePrompts.failure("CANCELLED")
        if (!item.started.compareAndSet(false, true)) throw PresencePrompts.failure("FORBIDDEN")
        fun valid() = item.live.get() && item.epoch == cancellation.get() && SystemClock.elapsedRealtime() < item.deadline
        try {
            if (!valid()) throw PresencePrompts.failure("CANCELLED")
            val signature = prompts.signature(str(net.dmsg.android.R.string.security_confirm), approvalTitle(item.reason), item.operation.signature, item.only, item, ::valid)
            if (!valid()) throw PresencePrompts.failure("CANCELLED")
            val signed = P256Signatures.raw(signature.run { update(item.message); sign() })
            if (!valid()) { signed.fill(0); throw PresencePrompts.failure("CANCELLED") }
            signed
        } finally {
            synchronized(prepared) { prepared.remove(preparation) }
            item.live.set(false); item.discard()
        }
    }
    override fun releasePrepared(preparation: ULong) {
        val item = synchronized(prepared) { prepared.remove(preparation) } ?: return
        item.live.set(false)
        prompts.cancel(item)
        if (!item.started.get()) item.discard()
    }
    // Android never authorizes an unspecified future message. These legacy
    // callbacks remain in the shared bridge only for other platform providers.
    override fun authorizeKeys(reason: String, biometryOnly: Boolean, handles: List<ByteArray>): ULong = throw PresencePrompts.failure("SIGNATURE_PREPARATION_REQUIRED")
    override fun signAuthorized(authorization: ULong, handle: ByteArray, message: ByteArray): ByteArray = throw PresencePrompts.failure("SIGNATURE_PREPARATION_REQUIRED")
    override fun releaseAuthorization(authorization: ULong) = Unit
    override fun confirm(requestId: String, digest: String, reason: String) = guarded {
        val epoch = cancellation.get()
        require(requestId.isNotEmpty() && requestId.length <= 128 && digest.length == 64)
        val (handle, _) = pair("approval-p256-v1", false, true)
        try {
            val signature = Signature.getInstance("SHA256withECDSA").apply { initSign(privateKey(handle, true)) }
            prompts.signature(str(net.dmsg.android.R.string.security_confirm), approvalTitle(reason), signature, false) { epoch == cancellation.get() }
                .run { update("$requestId:$digest".toByteArray()); sign().fill(0) }
        } finally { synchronized(store) { store.deleteEntry(handle.alias) } }
    }
    override fun cancelPresence() {
        cancellation.incrementAndGet(); prompts.cancel()
        val preparedIds = synchronized(prepared) { prepared.keys.toList() }
        preparedIds.forEach(::releasePrepared)
    }
    private inline fun <T> guarded(block: () -> T): T = try { block() }
    catch (error: MobileException) { throw error }
    catch (_: KeyPermanentlyInvalidatedException) { throw PresencePrompts.failure("INTEGRITY_FAILED") }
    catch (_: UnrecoverableKeyException) { throw PresencePrompts.failure("INTEGRITY_FAILED") }
    catch (_: AEADBadTagException) { throw PresencePrompts.failure("INTEGRITY_FAILED") }
    catch (_: BadPaddingException) { throw PresencePrompts.failure("INTEGRITY_FAILED") }
    catch (_: IllegalArgumentException) { throw PresencePrompts.failure("INTEGRITY_FAILED") }
    catch (_: Exception) { throw PresencePrompts.failure("HARDWARE_UNAVAILABLE") }
    private companion object {
        val oaep = OAEPParameterSpec("SHA-256", "MGF1", MGF1ParameterSpec.SHA1, PSource.PSpecified.DEFAULT)
    }
}
