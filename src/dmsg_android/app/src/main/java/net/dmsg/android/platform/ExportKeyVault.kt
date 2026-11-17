package net.dmsg.android.platform

import net.dmsg.android.R
import android.app.KeyguardManager
import android.content.Context
import android.security.keystore.*
import java.security.*
import java.security.spec.MGF1ParameterSpec
import java.security.spec.X509EncodedKeySpec
import javax.crypto.Cipher
import javax.crypto.spec.OAEPParameterSpec
import javax.crypto.spec.PSource

/** This alias namespace never contains account roots, device keys or intake keys. */
class ExportKeyVault(private val context: Context, private val prompts: PresencePrompts?) : ExportKeys {
    private val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private fun alias(header: ExportHeader) = context.packageName + ".export." + header.purpose + "." + header.installation + "." + header.id
    private val oaep = OAEPParameterSpec("SHA-256", "MGF1", MGF1ParameterSpec.SHA1, PSource.PSpecified.DEFAULT)
    override fun create(header: ExportHeader): ExportWrappedKey {
        check(context.getSystemService(KeyguardManager::class.java).isDeviceSecure) { "DEVICE_CREDENTIAL_NOT_SET" }
        val alias = alias(header)
        fun generate(strong: Boolean): KeyPair {
            val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(2048).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_RSA_OAEP).setDigests(KeyProperties.DIGEST_SHA256)
                .setUserAuthenticationRequired(true).setUserAuthenticationParameters(0, authenticators(header))
                .setInvalidatedByBiometricEnrollment(header.biometryOnly).setIsStrongBoxBacked(strong).build()
            return KeyPairGenerator.getInstance("RSA", "AndroidKeyStore").apply { initialize(spec) }.generateKeyPair()
        }
        try {
            val pair = if (context.packageManager.hasSystemFeature("android.hardware.strongbox_keystore")) {
                try { generate(true) } catch (_: StrongBoxUnavailableException) { generate(false) }
            } else generate(false)
            privateKey(header)
            val key = ByteArray(32).also(SecureRandom()::nextBytes)
            val payload = MessageDigest.getInstance("SHA-256").digest(header.binding()) + key
            try {
                val public = KeyFactory.getInstance("RSA").generatePublic(X509EncodedKeySpec(pair.public.encoded))
                val wrapped = Cipher.getInstance("RSA/ECB/OAEPPadding").run { init(Cipher.ENCRYPT_MODE, public, oaep); doFinal(payload) }
                return ExportWrappedKey(key.copyOf(), wrapped)
            } finally { key.fill(0); payload.fill(0) }
        } catch (error: Throwable) { store.deleteEntry(alias); throw error }
    }
    private fun authenticators(header: ExportHeader) = if (header.biometryOnly) KeyProperties.AUTH_BIOMETRIC_STRONG else KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL
    private fun privateKey(header: ExportHeader): PrivateKey {
        val key = store.getKey(alias(header), null) as? PrivateKey ?: error("EXPORT_KEY_UNAVAILABLE")
        val info = KeyFactory.getInstance("RSA", "AndroidKeyStore").getKeySpec(key, KeyInfo::class.java)
        check(info.securityLevel in setOf(KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT, KeyProperties.SECURITY_LEVEL_STRONGBOX)) { "HARDWARE_UNAVAILABLE" }
        check(info.isUserAuthenticationRequired && info.userAuthenticationValidityDurationSeconds <= 0 && info.userAuthenticationType == authenticators(header)) { "INTEGRITY_FAILED" }
        return key
    }
    override fun open(header: ExportHeader, valid: () -> Boolean): ByteArray {
        check(!context.getSystemService(KeyguardManager::class.java).isDeviceLocked) { "DEVICE_LOCKED" }
        val cipher = Cipher.getInstance("RSA/ECB/OAEPPadding").apply { init(Cipher.DECRYPT_MODE, privateKey(header), oaep) }
        val authorized = checkNotNull(prompts).cipher(str(R.string.text_open_dmsg_copies_206a0b9),
            if (header.purpose == ExportPurpose.credentials) str(R.string.text_authorize_this_password_selection_48c60bf) else str(R.string.text_allow_file_access_for_five_minutes_1287878), cipher, header.biometryOnly, valid)
        val plain = authorized.doFinal(header.wrapped)
        try {
            check(valid() && plain.size == 64 && MessageDigest.isEqual(plain.copyOfRange(0, 32), MessageDigest.getInstance("SHA-256").digest(header.binding()))) { "CANCELLED" }
            return plain.copyOfRange(32, 64)
        } finally { plain.fill(0) }
    }
    override fun remove(header: ExportHeader) { store.deleteEntry(alias(header)) }
}

fun systemExports(context: Context, purpose: ExportPurpose, prompts: PresencePrompts? = null) =
    SystemExportStore(java.io.File(context.noBackupFilesDir, "system/exports/" + purpose.name), purpose, ExportKeyVault(context, prompts))
