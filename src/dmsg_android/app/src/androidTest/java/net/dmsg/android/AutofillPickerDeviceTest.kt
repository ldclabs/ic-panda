package net.dmsg.android

import android.accessibilityservice.AccessibilityServiceInfo
import android.graphics.Rect
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.platform.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Opt in on a disposable English-language device with hardware-backed keys and a test PIN. */
@RunWith(AndroidJUnit4::class)
class AutofillPickerDeviceTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val ui = instrumentation.uiAutomation

    private fun shell(command: String): String = ParcelFileDescriptor.AutoCloseInputStream(ui.executeShellCommand(command))
        .bufferedReader().use { it.readText().trim() }

    private fun waitFor(description: String, check: () -> Boolean) {
        val until = SystemClock.elapsedRealtime() + 18_000
        while (SystemClock.elapsedRealtime() < until) {
            if (check()) return
            SystemClock.sleep(120)
        }
        fail("Timed out waiting for $description")
    }

    private fun node(test: (AccessibilityNodeInfo) -> Boolean): AccessibilityNodeInfo? {
        fun visit(value: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
            if (value == null) return null
            if (value.isVisibleToUser && test(value)) return value
            for (index in 0 until value.childCount) visit(value.getChild(index))?.let { return it }
            return null
        }
        return ui.windows.firstNotNullOfOrNull { visit(it.root) }
    }

    private fun bounds(text: String): Rect? = node { it.text?.toString()?.lines()?.contains(text) == true }
        ?.let { item -> Rect().also { item.getBoundsInScreen(it) } }?.takeUnless { it.isEmpty }

    private fun tap(text: String) {
        waitFor(text) {
            val rect = bounds(text) ?: return@waitFor false
            shell("input tap ${rect.centerX()} ${rect.centerY()}"); true
        }
    }

    @Test fun fillRememberAndSwitchBetweenTwoAccountsInAnExternalApp() {
        val pin = InstrumentationRegistry.getArguments().getString("autofillTestPin").orEmpty()
        assumeTrue(pin.matches(Regex("[0-9]{4,16}")))
        val app = instrumentation.targetContext
        val fixture = instrumentation.context.packageName
        val uri = Uri.parse("content://$fixture.incoming-fixture/autofill-state")
        val store = systemExports(app, ExportPurpose.credentials)
        assumeTrue("Requires an empty disposable installation", store.header(false) == null)
        val logins = listOf("personal", "work").map { ExportCredentialInput("Sample", it, "fixture-$it", "", it) }
        val oldProvider = shell("settings get secure autofill_service")
        val originalFlags = ui.serviceInfo.flags
        try {
            ui.serviceInfo = ui.serviceInfo.apply { flags = flags or AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS }
            val catalog = try { store.prepareCredentials(logins, "test-account", false, "test-version") }
            catch (failure: IllegalStateException) {
                assumeTrue("The device has no hardware-backed per-use key", failure.message != "HARDWARE_UNAVAILABLE")
                throw failure
            }
            catalog.use { store.publish(it) }
            shell("settings put secure autofill_service ${app.packageName}/net.dmsg.android.platform.DMsgAutofillService")
            fun enterPin() {
                waitFor("system PIN entry") {
                    node { it.packageName.toString() in setOf("com.android.systemui", "com.android.settings") &&
                        (it.isPassword || it.text?.toString()?.contains("PIN", ignoreCase = true) == true) } != null
                }
                shell("input text $pin"); shell("input keyevent ENTER")
            }
            for ((index, account) in listOf("personal", "work", "personal").withIndex()) {
                shell("am start -W -n $fixture/net.dmsg.android.AutofillFixtureActivity --ez nativeLogin true")
                tap("Unlock dMsg passwords")
                enterPin()
                waitFor("both accounts") { bounds("personal") != null && bounds("work") != null }
                if (index == 2) assertTrue("Most recently used account first", bounds("work")!!.centerY() < bounds("personal")!!.centerY())
                tap(account)
                if (index == 0) {
                    waitFor("first-use confirmation") { bounds("Fill and remember") != null }
                    val updated = logins.map { if (it.username == "personal") it.copy(password = "rotated-personal") else it }
                    store.prepareCredentials(updated, "test-account", false, "rotated").use { store.publish(it) }
                }
                if (index < 2) tap("Fill and remember")
                if (index == 0) enterPin()
                waitFor("selected username and password in the host") {
                    app.contentResolver.query(uri, null, null, null, null)!!.use {
                        val expected = if (account == "personal") "rotated-personal" else "fixture-work"
                        it.moveToFirst() && it.getString(0) == expected && it.getString(2) == account
                    }
                }
                app.contentResolver.query(uri.buildUpon().path("/autofill-finish").build(), null, null, null, null)?.close()
                waitFor("host dismissal") {
                    app.contentResolver.query(uri, null, null, null, null)!!.use { it.moveToFirst() && it.getString(0) == "missing" }
                }
            }
        } finally {
            if (oldProvider == "null" || oldProvider.isBlank()) shell("settings delete secure autofill_service")
            else shell("settings put secure autofill_service '$oldProvider'")
            store.invalidate("test-account")
            ui.serviceInfo = ui.serviceInfo.apply { flags = originalFlags }
        }
    }
}
