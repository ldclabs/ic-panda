package net.dmsg.android

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.view.WindowManager
import androidx.browser.auth.AuthTabIntent
import androidx.browser.customtabs.CustomTabsClient
import androidx.lifecycle.Lifecycle
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

    @Test fun browserLoginReceivesTheHttpsFragmentThroughTheSupportedReturnPath() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext.applicationContext as DMsgApplication
        val request = "https://dmsg.net/_/login#offline-test"
        val returned = Uri.parse("https://dmsg.net/_/android/auth/callback#YWJj")
        var launched: Intent? = null
        val monitor = object : Instrumentation.ActivityMonitor() {
            override fun onStartActivity(intent: Intent): Instrumentation.ActivityResult? {
                if (intent.dataString != request) return null
                launched = Intent(intent)
                return Instrumentation.ActivityResult(Activity.RESULT_OK, Intent().setData(returned))
            }
        }
        instrumentation.addMonitor(monitor)
        try {
            // ActivityScenario follows the intent after the fallback App Link's data has been cleared.
            val entry = Intent(Intent.ACTION_VIEW, null, app, MainActivity::class.java)
            ActivityScenario.launch<MainActivity>(entry).use { scenario ->
                await { !app.client.state.value.starting && app.client.state.value.session != null }
                scenario.onActivity { activity ->
                    app.client.clearNotice()
                    activity.openLogin(request)
                    val tab = checkNotNull(launched)
                    val browser = CustomTabsClient.getPackageName(activity, null)
                    assertEquals(browser, tab.`package`)
                    if (browser != null && CustomTabsClient.isAuthTabSupported(activity, browser)) {
                        assertTrue(tab.getBooleanExtra(AuthTabIntent.EXTRA_LAUNCH_AUTH_TAB, false))
                        assertEquals("dmsg.net", tab.getStringExtra(AuthTabIntent.EXTRA_HTTPS_REDIRECT_HOST))
                        assertEquals("/_/android/auth/callback", tab.getStringExtra(AuthTabIntent.EXTRA_HTTPS_REDIRECT_PATH))
                        assertFalse(tab.hasExtra(AuthTabIntent.EXTRA_REDIRECT_SCHEME))
                    } else {
                        assertFalse(tab.getBooleanExtra(AuthTabIntent.EXTRA_LAUNCH_AUTH_TAB, false))
                        activity.startActivity(Intent(Intent.ACTION_VIEW, returned, activity, MainActivity::class.java))
                    }
                }
                // A complete callback reached the login parser. This offline test has no pending II request.
                await { app.client.state.value.error == "LOGIN_EXPIRED" }
                scenario.onActivity { app.client.clearNotice() }
            }
        } finally { instrumentation.removeMonitor(monitor) }
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
            scenario.onActivity { app.client.lock() }
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

    @Test fun welcomeCanBeCapturedAfterReturningFromBackground() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext.applicationContext as DMsgApplication
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            await { !app.client.state.value.starting && app.client.state.value.session != null }
            scenario.moveToState(Lifecycle.State.CREATED)
            scenario.onActivity { activity ->
                assertEquals(android.graphics.Color.BLACK, (activity.window.decorView.foreground as? android.graphics.drawable.ColorDrawable)?.color)
            }
            scenario.moveToState(Lifecycle.State.RESUMED)
            await {
                var focused = false
                scenario.onActivity { focused = it.hasWindowFocus() }
                focused
            }
            scenario.onActivity { activity ->
                assertEquals(0, activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE)
            }
            instrumentation.waitForIdleSync()
            Thread.sleep(250)
            val screenshot = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
            val center = screenshot.getPixel(screenshot.width / 2, screenshot.height / 2)
            assertTrue("Screenshot should contain the welcome UI", (screenshot.height / 8 until screenshot.height * 7 / 8 step 8).any { y ->
                (screenshot.width / 8 until screenshot.width * 7 / 8 step 8).any { x -> screenshot.getPixel(x, y) != center }
            })
            val destination = File(checkNotNull(app.getExternalFilesDir(null)), "qa-welcome.png")
            destination.outputStream().use { assertTrue(screenshot.compress(Bitmap.CompressFormat.PNG, 100, it)) }
            screenshot.recycle()
            android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(
                "mkdir -p /sdcard/Download/dmsg-qa")).use { it.readBytes() }
            android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(
                "cp " + destination.absolutePath + " /sdcard/Download/dmsg-qa/welcome.png")).use { it.readBytes() }
        }
    }
}
