package net.dmsg.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import net.dmsg.android.core.LOCAL_ACCOUNT
import net.dmsg.android.core.NativeBootstrap
import net.dmsg.android.platform.AndroidSecurity
import net.dmsg.android.platform.P256Signatures
import net.dmsg.android.platform.PresencePrompts
import net.dmsg.android.platform.PrivateFiles
import net.dmsg.core.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyPairGenerator
import java.security.SecureRandom
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

@RunWith(AndroidJUnit4::class)
class OfflineAccountTest {
    // Substitute only OS credential callbacks. The production JNI bridge, SQLite and file library run on Android.
    private class FixtureHardware(base: Platform) : Platform by base {
        private val storage = ConcurrentHashMap<String, ByteArray>()
        private val secret = ByteArray(32).also(SecureRandom()::nextBytes)
        private val pair = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
        override fun get(name: String) = storage[name]?.copyOf()
        override fun set(name: String, secret: ByteArray) { storage[name] = secret.copyOf() }
        override fun delete(name: String) { storage.remove(name) }
        override fun presence(): UByte = 1u
        override fun createSecret(biometryOnly: Boolean) = HardwareSecret(byteArrayOf(1), byteArrayOf(2), secret.copyOf())
        override fun openSecret(handle: ByteArray, context: ByteArray, biometryOnly: Boolean): ByteArray {
            check(handle.contentEquals(byteArrayOf(1)) && context.contentEquals(byteArrayOf(2)))
            return secret.copyOf()
        }
        override fun createAnchor(): SigningKey {
            val point = (pair.public as ECPublicKey).w
            return SigningKey(byteArrayOf(4) + P256Signatures.scalar(point.affineX) + P256Signatures.scalar(point.affineY), byteArrayOf(3))
        }
        override fun signAnchor(handle: ByteArray, message: ByteArray): ByteArray {
            check(handle.contentEquals(byteArrayOf(3)))
            return P256Signatures.raw(Signature.getInstance("SHA256withECDSA").run { initSign(pair.private); update(message); sign() })
        }
        override fun deleteSigning(handle: ByteArray) {}
        override fun confirm(requestId: String, digest: String, reason: String) { check(requestId.isNotBlank() && digest.isNotBlank()) }
    }

    /** The operator supplies synthetic desktop and iOS archives to the disposable app's files directory. */
    @Test fun exchangeStandaloneArchives() = runBlocking {
        val instrument = InstrumentationRegistry.getInstrumentation()
        org.junit.Assume.assumeTrue(InstrumentationRegistry.getArguments().getString("offlineInterop") == "true")
        val app = instrument.targetContext.applicationContext
        assertTrue(NativeBootstrap.initialize(app))
        val exchange = File(app.filesDir, "offline-interop")
        for (source in listOf("desktop", "ios", "android")) {
            val base = File(app.cacheDir, "exchange-" + UUID.randomUUID()).apply { mkdirs() }
            val staging = File(base, "staging").apply { mkdirs() }
            val hardware = FixtureHardware(AndroidSecurity(app, PrivateFiles(app), PresencePrompts()))
            try {
                MobileEngine.open(File(base, "home").path, staging.path, hardware).use { root ->
                    val local = root.createLocalAccount()
                    root.scoped(local.generation).use { engine ->
                        val name = LOCAL_ACCOUNT + "/dmsg-import-exchange/" + source + ".dmsgvault"
                        val staged = File(staging, name)
                        staged.parentFile!!.mkdirs()
                        File(exchange, source + ".dmsgvault").copyTo(staged)
                        val importId = engine.vaultBackupBegin(true)
                        val preview = JSONObject(engine.vaultBackupInspect(importId, name, "123456").json)
                        assertEquals(LOCAL_ACCOUNT, preview.getString("source"))
                        val result = JSONObject(engine.vaultBackupCommit(importId, preview.getString("digest"), listOf(0u, 1u), "root").json)
                        assertEquals(2, result.getInt("items")); assertEquals(1, result.getInt("files"))
                        val operation = engine.vaultBackupBegin(false)
                        val inventory = JSONObject(engine.vaultBackupInventory(operation).json).getJSONObject("inventory").getJSONArray("items")
                        val rows = (0 until inventory.length()).map { inventory.getJSONObject(it) }
                        assertEquals(2, rows.size)
                        val items = rows.map { engine.vaultItem(it.getString("id"), null).item }
                        assertTrue(items.all { it.saveState == "local_only" && !it.synced })
                        assertEquals(items[0].files.single(), items[1].files.single())
                        if (source == "desktop") {
                            val approval = JSONObject(engine.vaultBackupRequest(operation, items.map { it.id }, true, "123456", "123456").json)
                            val request = approval.getString("id")
                            engine.decide(request, approval.getString("digest"), true, OperationToken())
                            engine.vaultBackupExport(operation, request)
                            File(engine.vaultBackupOutput(operation)).copyTo(File(exchange, "android.dmsgvault"), overwrite = true)
                        }
                        engine.vaultBackupCancel(operation)
                    }
                }
            } finally { base.deleteRecursively() }
        }
    }

    @Test fun restartReadsLocalVaultAndFile() = runBlocking {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
        assertTrue(NativeBootstrap.initialize(app))
        val base = File(app.cacheDir, "offline-" + UUID.randomUUID()).apply { mkdirs() }
        val home = File(base, "home").absolutePath
        val staging = File(base, "staging").apply { mkdirs() }
        val hardware = FixtureHardware(AndroidSecurity(app, PrivateFiles(app), PresencePrompts()))
        try {
            val item = MobileEngine.open(home, staging.path, hardware).use { root ->
                val session = root.createLocalAccount()
                assertEquals(LOCAL_ACCOUNT, session.account); assertEquals("local", session.accountKind)
                root.scoped(session.generation).use { engine ->
                    val note = engine.saveItem(null, null, VaultEdit("Local fixture", "note", "Device only", false, emptyList(), emptyList(), null)).item
                    assertEquals("local_only", note.saveState); assertFalse(note.synced)
                    val name = LOCAL_ACCOUNT + "/dmsg-import-fixture/sample.txt"
                    File(staging, name).apply { parentFile!!.mkdirs(); writeText("offline file") }
                    engine.importFile(name, "root", null, OperationToken())
                    engine.lock()
                    note.id
                }
            }
            MobileEngine.open(home, staging.path, hardware).use { root ->
                assertEquals("locked", root.sessionState().state)
                val unlocked = root.unlockFast()
                root.scoped(unlocked.generation).use { engine ->
                    assertEquals("Device only", engine.vaultItem(item, null).item.notes)
                    val file = JSONObject(engine.files("root").json).getJSONArray("files").getJSONObject(0)
                    assertEquals("sample.txt", file.getString("name"))
                    assertEquals(12, file.getInt("size"))
                    assertEquals(0uL, unlocked.pendingSync)
                }
            }
        } finally { base.deleteRecursively() }
    }
}
