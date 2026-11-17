package net.dmsg.android.platform

import android.app.KeyguardManager
import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Key for intake only; it never wraps an account root or an exported password. */
class IncomingKeys(private val context: Context, private val root: File) : IncomingSealer {
    private val installation: String
    private val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private val alias: String
    init {
        synchronized(lock) {
            check(root.mkdirs() || root.isDirectory)
            val marker = File(root, "installation")
            if (!marker.exists()) {
                check(root.listFiles().isNullOrEmpty()) { "INSTALLATION_MISMATCH" }
                PrivateFiles.atomicWrite(marker, UUID.randomUUID().toString().toByteArray())
            }
            check(marker.length() in 1..128) { "INSTALLATION_MISMATCH" }
            installation = marker.readText().also { check(UUID.fromString(it).toString() == it) }
            alias = context.packageName + ".incoming." + installation
        }
    }
    private fun key(create: Boolean): SecretKey = synchronized(lock) {
        check(!context.getSystemService(KeyguardManager::class.java).isDeviceLocked) { "DEVICE_LOCKED" }
        val found = store.getKey(alias, null) as? SecretKey
        if (found != null) return found
        check(create && !File(root, "pending.sealed").exists()) { "INCOMING_KEY_UNAVAILABLE" }
        val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setRandomizedEncryptionRequired(true)
        // Android 12–14 have unlocked-device key deletion/re-authorization bugs.
        // Intake remains independently encrypted there, with an explicit lock gate.
        if (Build.VERSION.SDK_INT >= 35) spec.setUnlockedDeviceRequired(true)
        KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply { init(spec.build()) }.generateKey()
    }
    private fun aad(scope: String) = ("dmsg/android-incoming/1:" + installation + ":" + scope).toByteArray()
    override fun seal(scope: String, bytes: ByteArray): ByteArray {
        check(bytes.size <= IncomingShareStore.PART)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key(true)); updateAAD(aad(scope)) }
        return cipher.iv + cipher.doFinal(bytes)
    }
    override fun open(scope: String, bytes: ByteArray): ByteArray {
        check(bytes.size in 28..IncomingShareStore.PART + 28) { "INTEGRITY_FAILED" }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key(false), GCMParameterSpec(128, bytes.copyOfRange(0, 12))); updateAAD(aad(scope)) }
        return cipher.doFinal(bytes, 12, bytes.size - 12)
    }
    companion object { private val lock = Any() }
}
