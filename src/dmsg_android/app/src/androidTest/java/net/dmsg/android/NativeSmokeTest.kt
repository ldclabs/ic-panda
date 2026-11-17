package net.dmsg.android

import android.graphics.Bitmap
import android.view.WindowManager
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import net.dmsg.core.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** Real JNI/core/Activity checks. No accounts, fake approvals or network success fixtures. */
@RunWith(AndroidJUnit4::class)
class NativeSmokeTest {
    @Test fun qrTransportRoundTripsThroughTheRealNativeDecoderAndBounds() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
        assertTrue(net.dmsg.android.core.NativeBootstrap.initialize(app))
        val text = "dmsg-device-request-offline-qa"
        val bytes = android.util.Base64.decode(qrPng(text).substringAfter(','), android.util.Base64.DEFAULT)
        val bitmap = android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
        try {
            val colors = IntArray(bitmap.width * bitmap.height)
            bitmap.getPixels(colors, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
            val pixels = ByteArray(colors.size) { (colors[it] and 255).toByte() }
            assertEquals(text, scanQrLuma(bitmap.width.toUInt(), bitmap.height.toUInt(), pixels))
            try { scanQrLuma(UInt.MAX_VALUE, UInt.MAX_VALUE, byteArrayOf(0)); fail("oversized QR frame was accepted") }
            catch (error: MobileException.Failure) { assertEquals("INVALID_INPUT", error.code) }
            pixels.fill(0); colors.fill(0)
        } finally { bitmap.recycle(); bytes.fill(0) }
    }

    @Test fun revocationDownloadsDoNotPermitCleartextApplicationTraffic() {
        val policy = android.security.NetworkSecurityPolicy.getInstance()
        assertFalse(policy.isCleartextTrafficPermitted("dmsg.net"))
        assertFalse(policy.isCleartextTrafficPermitted("api.dmsg.net"))
        assertFalse(policy.isCleartextTrafficPermitted("icp-api.io"))
        assertFalse(policy.isCleartextTrafficPermitted("untrusted.example"))
        assertTrue(policy.isCleartextTrafficPermitted("c.pki.goog"))
        assertTrue(policy.isCleartextTrafficPermitted("e7.c.lencr.org"))
    }

    private fun await(timeout: Long = 15_000, condition: () -> Boolean) {
        val until = android.os.SystemClock.elapsedRealtime() + timeout
        while (!condition()) {
            check(android.os.SystemClock.elapsedRealtime() < until) { "Condition timed out" }
            Thread.sleep(50)
        }
    }

    @Test fun nativeStartupRotationAndLockInvalidateOldScopedHandles() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext.applicationContext as DMsgApplication
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            await { !app.client.state.value.starting && app.client.state.value.session != null }
            assertNull(app.client.state.value.error)
            assertTrue(buildIdentity().matches(Regex("[a-f0-9]{64}")))
            val old = checkNotNull(app.client.scopedHandle())
            val before = runBlocking { old.sessionState() }
            runBlocking { old.accounts() }
            val controller = app.client
            scenario.recreate()
            assertSame(controller, app.client)
            scenario.onActivity { activity ->
                assertTrue(activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
                app.client.lock()
            }
            await { app.client.state.value.session?.generation?.let { it > before.generation } == true }
            try {
                // sessionState/controlGeneration are intentionally unscoped controller
                // metadata. Account reads must use the immutable operation generation.
                runBlocking { old.accounts() }
                fail("old native scope survived lock")
            } catch (error: MobileException.Failure) { assertEquals("LOCKED", error.code) }
            finally { old.destroy() }
            val fresh = checkNotNull(app.client.scopedHandle())
            runBlocking { fresh.accounts() }
            try {
                runBlocking { fresh.saveAuthoredDraft(MessageDraft("a1".repeat(32), "fixture", MessageDraftStamp("ab".repeat(16), ULong.MAX_VALUE), emptyList(), null, null)) }
                fail("locked draft write was accepted")
            } catch (error: MobileException.Failure) { assertEquals("LOCKED", error.code) }
            val token = OperationToken()
            try {
                token.cancel()
                runBlocking { fresh.sync(false, token) }
                fail("cancelled native operation ran")
            } catch (error: MobileException.Failure) { assertEquals("CANCELLED", error.code) }
            finally { token.destroy(); fresh.destroy() }
        }
    }

    @Test fun captureWelcomeWithoutChangingProductionScreenshotPolicy() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext.applicationContext as DMsgApplication
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            await { !app.client.state.value.starting && app.client.state.value.session != null }
            scenario.onActivity { activity ->
                assertTrue(activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
                // Only instrumentation clears it, temporarily, to capture this no-account screen.
                activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
            }
            instrumentation.waitForIdleSync()
            Thread.sleep(250)
            val screenshot = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
            val destination = File(checkNotNull(app.getExternalFilesDir(null)), "qa-welcome.png")
            destination.outputStream().use { assertTrue(screenshot.compress(Bitmap.CompressFormat.PNG, 100, it)) }
            screenshot.recycle()
            android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(
                "mkdir -p /sdcard/Download/dmsg-qa")).use { it.readBytes() }
            android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(
                "cp " + destination.absolutePath + " /sdcard/Download/dmsg-qa/welcome.png")).use { it.readBytes() }
            scenario.onActivity { it.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE) }
        }
    }
}
