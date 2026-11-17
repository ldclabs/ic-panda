package net.dmsg.android

import android.graphics.Bitmap
import android.os.ParcelFileDescriptor
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.core.Route
import net.dmsg.android.platform.str
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.io.File

class AccountNavigationFlowTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @Test fun groupedAccountActionsKeepTheirParentAndWorkspaceRootsStayUsable() {
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val client = compose.activity.qaClient
        val account = client.state.value.session!!.account
        compose.runOnIdle { client.dark(false); client.navigation.open("account") }

        compose.onNodeWithContentDescription(str(R.string.text_lock_workspace_78665ed)).assertIsDisplayed()
        compose.onNodeWithTag("membership-badge").assertIsDisplayed().performClick()
        compose.runOnIdle { assertEquals("membership", client.navigation.state.value.current.page) }
        back()
        row(R.string.text_approvals_1b5e94c).performClick()
        compose.runOnIdle { assertEquals("approvals", client.navigation.state.value.current.page) }
        back()

        row(R.string.text_system_integrations_9ea81ce).performClick()
        compose.onNodeWithText(str(R.string.text_password_autofill_872af78)).assertIsDisplayed()
        row(R.string.text_files_app_copies_96ebae6).performClick()
        compose.runOnIdle { assertEquals("system-files", client.navigation.state.value.current.page) }
        back()
        compose.runOnIdle { assertEquals("system-integrations", client.navigation.state.value.current.page) }
        capture("system-integrations")
        back()

        row(R.string.text_advanced).performClick()
        compose.onNodeWithText(str(R.string.text_agents_dcfd6ed)).assertIsDisplayed()
        row(R.string.text_cached_data_b29fb6f).performClick()
        compose.runOnIdle { assertEquals("display-cache", client.navigation.state.value.current.page) }
        back()
        compose.runOnIdle { assertEquals("advanced", client.navigation.state.value.current.page) }
        capture("advanced")
        back()
        row(R.string.text_account_security_41c2886).performClick()
        row(R.string.text_remove_local_data_c45125e).performClick()
        compose.runOnIdle { assertEquals("account-remove", client.navigation.state.value.current.page) }
        back()
        compose.runOnIdle { assertEquals("security", client.navigation.state.value.current.page) }
        back()
        compose.onNodeWithTag("membership-badge").performScrollTo().assertIsDisplayed()
        capture("account-root")

        for (workspace in listOf("vault", "messages", "files")) {
            compose.runOnIdle { client.navigation.open(workspace) }
            compose.runOnIdle { assertEquals(Route(workspace), client.navigation.state.value.current) }
            if (workspace == "vault") compose.onNodeWithText("Studio account").assertIsDisplayed()
            if (workspace == "files") {
                compose.waitUntil(10_000) { compose.onAllNodesWithText("Project materials").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText(str(R.string.text_on_this_device_6032118)).assertIsDisplayed()
                compose.onNodeWithText(str(R.string.text_cloud_backups_ab4daf1)).assertIsDisplayed()
            }
            capture("$workspace-root")
            if (workspace == "vault") {
                compose.onNodeWithText("Studio account").performClick()
                compose.waitUntil(10_000) { compose.onAllNodesWithText("demo@example.test").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText("Sample-only password").assertDoesNotExist()
                capture("vault-detail")
                back()
            }
        }
        compose.runOnIdle { assertEquals(account, client.state.value.session!!.account) }
    }

    private fun row(resource: Int): SemanticsNodeInteraction =
        compose.onNode(hasText(str(resource)) and hasClickAction()).performScrollTo().assertIsDisplayed()

    private fun back() = compose.onNodeWithContentDescription(str(R.string.text_back_5db5cac)).performClick()

    private fun capture(name: String) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        compose.waitForIdle()
        Thread.sleep(300)
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val file = File(instrumentation.targetContext.getExternalFilesDir(null), "current-$name.png")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
        for (command in listOf("mkdir -p /sdcard/Download/dmsg-current-roots", "cp ${file.absolutePath} /sdcard/Download/dmsg-current-roots/${file.name}")) {
            ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(command)).use { it.readBytes() }
        }
    }
}
