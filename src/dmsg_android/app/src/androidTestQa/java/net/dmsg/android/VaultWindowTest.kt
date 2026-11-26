package net.dmsg.android

import android.graphics.Bitmap
import android.os.ParcelFileDescriptor.AutoCloseInputStream
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import net.dmsg.android.core.ReadState
import net.dmsg.android.core.VaultQuery
import net.dmsg.android.platform.RecentsPrivacy
import net.dmsg.android.ui.DarkColors
import net.dmsg.android.ui.LightColors
import net.dmsg.android.ui.page
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File

class VaultWindowTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @Test fun fieldTypeMenuKeepsItsEditorVisible() {
        val client = compose.activity.qaClient
        compose.waitUntil(20_000) { client.state.value.unlocked }
        compose.activityRule.scenario.onActivity { RecentsPrivacy(it).install() }
        compose.runOnIdle { client.dynamicColor(false); client.vaultDrafts.open(null) }
        compose.waitUntil(5_000) { client.vaultDrafts.presented.value }
        compose.onNodeWithText("Customize fields").performScrollTo().performClick()
        for (dark in listOf(false, true)) {
            compose.runOnIdle { client.dark(dark) }
            val fieldType = if (dark) "Phone" else "Text"
            compose.onAllNodesWithText(fieldType).onFirst().performScrollTo().performClick()
            val menu = compose.onNode(hasText("Month") and hasAnyAncestor(isPopup()))
            menu.assertIsDisplayed()
            capture("field-menu", dark) { image ->
                val page = (if (dark) DarkColors else LightColors).page.toArgb()
                assertTrue("The editor page should be visible behind the field menu",
                    (image.height / 8 until image.height * 7 / 8 step 16).any { y ->
                        (0 until image.width step 16).any { x -> image.getPixel(x, y) == page }
                    })
            }
            val type = if (dark) "Month" else "Phone"
            compose.onNode(hasText(type) and hasAnyAncestor(isPopup())).performClick()
            compose.onNodeWithText(type).assertIsDisplayed()
        }
    }

    @Test fun stoppedVaultReloadsItsFilteredListWhenTheSessionResumes() {
        val client = compose.activity.qaClient
        compose.waitUntil(20_000) { client.vault.listing.state.value is ReadState.Ready }
        val query = VaultQuery(kind = "login")
        compose.runOnIdle { client.vault.browse(query) }
        compose.waitUntil(10_000) { client.vault.listing.state.value is ReadState.Ready }
        val scenario = compose.activityRule.scenario
        for (dark in listOf(false, true)) {
            compose.runOnIdle { client.dark(dark) }
            val handle = checkNotNull(client.scopedHandle())
            try {
                val generation = handle.controlGeneration()
                scenario.moveToState(Lifecycle.State.CREATED)
                scenario.onActivity { client.background() }
                compose.waitUntil(10_000) { handle.controlGeneration() != generation }
                // Finish the resume while lifecycle-aware collectors are still stopped.
                scenario.onActivity { client.foreground() }
                runBlocking { withTimeout(10_000) { client.state.first { !it.concealed && it.unlocked } } }
            } finally { handle.destroy() }
            scenario.moveToState(Lifecycle.State.RESUMED)
            compose.waitUntil(10_000) { client.vault.listing.state.value is ReadState.Ready }
            assertEquals(query, client.vault.query)
            compose.onNodeWithText("Studio account").assertIsDisplayed()
            capture("resumed-vault", dark)
        }
    }

    private fun capture(name: String, dark: Boolean, verify: (Bitmap) -> Unit = {}) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val automation = instrumentation.uiAutomation
        compose.waitForIdle()
        // The popup and its parent draw in separate system windows after Compose is idle.
        automation.waitForIdle(250, 5_000)
        val image = checkNotNull(automation.takeScreenshot())
        val mode = if (dark) "dark" else "light"
        try {
            val file = File(instrumentation.targetContext.getExternalFilesDir(null), "$name-$mode.png")
            file.outputStream().use { assertTrue(image.compress(Bitmap.CompressFormat.PNG, 100, it)) }
            // verify.sh collects screenshots from /sdcard/Download/dmsg-*.
            val report = "/sdcard/Download/dmsg-qa"
            for (command in listOf("mkdir -p $report", "cp ${file.absolutePath} $report/${file.name}")) {
                AutoCloseInputStream(automation.executeShellCommand(command)).use { it.readBytes() }
            }
            verify(image)
        } finally { image.recycle() }
    }
}
