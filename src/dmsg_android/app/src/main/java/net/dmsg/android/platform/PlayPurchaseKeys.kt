package net.dmsg.android.platform

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Receipt-only key. Late payment callbacks may encrypt while the account is locked;
 * this key cannot open account contents or approve a purchase. */
class PlayPurchaseKeys(context: Context, private val root: File) : PurchaseSealer {
    private val keys = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private val installation: String
    private val alias: String
    init {
        synchronized(lock) {
            check(root.mkdirs() || root.isDirectory)
            val marker = File(root, "installation")
            if (!marker.exists()) {
                check(root.listFiles().isNullOrEmpty()) { "PAYMENT_STORAGE_UNAVAILABLE" }
                PrivateFiles.atomicWrite(marker, UUID.randomUUID().toString().toByteArray())
            }
            check(marker.length() in 1..128) { "PAYMENT_STORAGE_UNAVAILABLE" }
            installation = marker.readText().also { check(UUID.fromString(it).toString() == it) }
            alias = context.packageName + ".play.receipts." + installation
        }
    }
    private fun key(create: Boolean): SecretKey = synchronized(lock) {
        (keys.getKey(alias, null) as? SecretKey)?.let { return it }
        check(create && !File(root, "purchases.sealed").exists()) { "PAYMENT_STORAGE_UNAVAILABLE" }
        KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT).setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).setRandomizedEncryptionRequired(true).build())
        }.generateKey()
    }
    private fun aad() = ("dmsg/android-play-receipts/1:" + installation).toByteArray()
    override fun seal(bytes: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key(true)); updateAAD(aad()) }
        return cipher.iv + cipher.doFinal(bytes)
    }
    override fun open(bytes: ByteArray): ByteArray {
        check(bytes.size >= 28) { "PAYMENT_STORAGE_UNAVAILABLE" }
        return Cipher.getInstance("AES/GCM/NoPadding").run { init(Cipher.DECRYPT_MODE, key(false), GCMParameterSpec(128, bytes.copyOfRange(0,12))); updateAAD(aad()); doFinal(bytes,12,bytes.size-12) }
    }
    companion object { private val lock = Any() }
}
