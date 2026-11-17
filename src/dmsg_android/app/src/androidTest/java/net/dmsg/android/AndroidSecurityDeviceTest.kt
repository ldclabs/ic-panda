package net.dmsg.android

import android.os.SystemClock
import android.view.KeyEvent
import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.platform.AndroidSecurity
import net.dmsg.android.platform.PresencePrompts
import net.dmsg.android.platform.PrivateFiles
import net.dmsg.core.MobileException
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.math.BigInteger
import java.security.KeyStore
import java.security.Signature
import java.util.concurrent.Callable
import java.util.concurrent.Executors
import java.util.concurrent.Future
import java.util.concurrent.TimeUnit

/** Opt-in, explicitly disposable test device only; never changes its screen lock.
 * Uses the real system credential UI and production platform implementation.
 * Emulator KeyInfo, when supported, is not physical TEE proof. */
class AndroidSecurityDeviceTest {
    @Test fun realCredentialCryptoObjectsUnwrapAndSignOnlyThePreparedOperation() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue(arguments.getString("hardwareProbe") == "true")
        check(arguments.getString("disposableDevice") == "true") { "Explicit disposable test device required" }
        val pin = checkNotNull(arguments.getString("probePin"))
        check(pin.matches(Regex("[0-9]{4,8}")))
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext.applicationContext
        val prompts = PresencePrompts()
        val files = PrivateFiles(app)
        val security = AndroidSecurity(app, files, prompts)
        val keys = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val created = mutableSetOf<String>()
        fun alias(handle: ByteArray) = JSONObject(handle.toString(Charsets.UTF_8)).getString("alias")
        fun remember(handle: ByteArray) { created += alias(handle) }
        val executor = Executors.newSingleThreadExecutor()
        fun <T> background(block: () -> T): Future<T> = executor.submit(Callable(block))
        fun <T> authenticate(block: () -> T): T {
            val result = background(block)
            try {
                waitUntil {
                    if (result.isDone) result.get() // Surface an early platform failure, not a UI timeout.
                    val root = instrumentation.uiAutomation.rootInActiveWindow
                    val system = root?.packageName?.toString() in setOf("com.android.systemui", "com.android.settings")
                    system && find(root) { it.isPassword && it.isEditable } != null
                }
                for (digit in pin) {
                    val code = KeyEvent.KEYCODE_0 + (digit - '0')
                    instrumentation.sendKeyDownUpSync(code)
                }
                instrumentation.sendKeyDownUpSync(KeyEvent.KEYCODE_ENTER)
                return result.get(15, TimeUnit.SECONDS)
            } finally { if (!result.isDone) security.cancelPresence() }
        }
        fun rejected(code: String, block: () -> Unit) {
            try { block(); fail("Expected $code") }
            catch (error: MobileException.Failure) { assertEquals(code, error.code) }
        }
        fun verify(handle: ByteArray, message: ByteArray, raw: ByteArray) {
            assertEquals(64, raw.size)
            val order = BigInteger("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551", 16)
            assertTrue(BigInteger(1, raw.copyOfRange(32, 64)) <= order.shiftRight(1))
            val r = BigInteger(1, raw.copyOfRange(0, 32)).toByteArray()
            val s = BigInteger(1, raw.copyOfRange(32, 64)).toByteArray()
            val der = byteArrayOf(0x30, (r.size + s.size + 4).toByte(), 2, r.size.toByte()) + r + byteArrayOf(2, s.size.toByte()) + s
            val verifier = Signature.getInstance("SHA256withECDSA")
            verifier.initVerify(keys.getCertificate(alias(handle)).publicKey)
            verifier.update(message)
            assertTrue(verifier.verify(der))
            verifier.initVerify(keys.getCertificate(alias(handle)).publicKey)
            verifier.update(message + 1.toByte())
            assertFalse(verifier.verify(der))
        }
        try {
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                scenario.onActivity { prompts.attach(it) }
                waitUntil { prompts.canPrompt }
                assertTrue("A screen lock must be configured before this probe", security.presence() > 0.toUByte())
                val wrapped = security.createSecret(false)
                remember(wrapped.handle)
                assertFalse("Public wrapping must not prompt", prompts.isAuthenticating)
                val opened = authenticate { security.openSecret(wrapped.handle, wrapped.context, false) }
                assertArrayEquals(wrapped.secret, opened)
                opened.fill(0)
                wrapped.secret.fill(0)

                val signing = security.createSigning(false)
                remember(signing.handle)
                val message = ByteArray(32) { it.toByte() }
                val prepared = security.prepareSignature("wallet_test", false, signing.handle, message)
                val signature = authenticate { security.signPrepared(prepared) }
                verify(signing.handle, message, signature)
                rejected("CANCELLED") { security.signPrepared(prepared) }

                val cancel = security.prepareSignature("wallet_test", false, signing.handle, message)
                val later = security.prepareSignature("wallet_test", false, signing.handle, message)
                val waiting = background { security.signPrepared(cancel) }
                waitUntil { prompts.isAuthenticating }
                security.cancelPresence()
                try { waiting.get(5, TimeUnit.SECONDS); fail("Cancelled signature completed") }
                catch (error: java.util.concurrent.ExecutionException) {
                    assertEquals("CANCELLED", (error.cause as MobileException.Failure).code)
                }
                rejected("CANCELLED") { security.signPrepared(later) }
                assertTrue("Cancelling must retain the key", keys.containsAlias(alias(signing.handle)))

                val anchor = security.createAnchor()
                remember(anchor.handle)
                verify(anchor.handle, message, security.signAnchor(anchor.handle, message))
                val reopened = AndroidSecurity(app, PrivateFiles(app), prompts)
                verify(anchor.handle, message, reopened.signAnchor(anchor.handle, message))
                keys.deleteEntry(alias(anchor.handle))
                rejected("INTEGRITY_FAILED") { reopened.signAnchor(anchor.handle, message) }
                assertFalse("A missing anchor must not be silently replaced", keys.containsAlias(alias(anchor.handle)))
                android.util.Log.i("dMsgHardwareProbe", "Real credential CryptoObjects passed; reported level=${security.securityLevel}; device provenance must be recorded separately")
                scenario.onActivity { prompts.detach(it) }
            }
        } finally {
            security.cancelPresence()
            executor.shutdownNow()
            created.forEach { keys.deleteEntry(it) }
        }
    }

    private fun waitUntil(condition: () -> Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 15_000
        while (!condition()) {
            check(SystemClock.elapsedRealtime() < deadline) { "System authentication UI timed out" }
            Thread.sleep(50)
        }
    }

    private fun find(node: AccessibilityNodeInfo?, matches: (AccessibilityNodeInfo) -> Boolean): AccessibilityNodeInfo? {
        if (node == null) return null
        if (matches(node)) return node
        for (i in 0 until node.childCount) find(node.getChild(i), matches)?.let { return it }
        return null
    }
}
