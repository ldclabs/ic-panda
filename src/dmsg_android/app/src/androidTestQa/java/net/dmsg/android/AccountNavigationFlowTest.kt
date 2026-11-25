package net.dmsg.android

import android.graphics.Bitmap
import android.os.ParcelFileDescriptor
import androidx.activity.BackEventCompat
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.core.FileDetails
import net.dmsg.android.core.Route
import net.dmsg.android.core.decode
import net.dmsg.android.core.parseFileDetails
import net.dmsg.android.platform.str
import net.dmsg.android.ui.messageTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

class AccountNavigationFlowTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @Test fun compactNavigationBarSpansTheWindowAndLeavesTheLastActionReachable() {
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val client = compose.activity.qaClient
        compose.runOnIdle { client.navigation.open("account") }
        val bar = compose.onNodeWithTag("workspace-navigation").assertIsDisplayed().getUnclippedBoundsInRoot()
        val window = compose.onNodeWithTag("workspace-content").getUnclippedBoundsInRoot()
        assertTrue(bar.left <= window.left && bar.right >= window.right && bar.bottom >= window.bottom)
        val tab = SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Tab)
        compose.onAllNodes(tab).assertCountEquals(4)
        compose.onNode(tab and hasText(str(R.string.text_account_4ab56e9))).assertIsSelected()
        compose.onNode(hasScrollAction()).performSemanticsAction(SemanticsActions.ScrollBy) { scroll -> scroll(0f, 10_000f) }
        val signOut = compose.onNodeWithText(str(R.string.text_sign_out_7bfa045)).getUnclippedBoundsInRoot()
        assertTrue(signOut.bottom <= bar.top)
        capture("navigation-account")
        compose.onNode(tab and hasText(str(R.string.text_files_c79ddc2))).performClick()
        val files = compose.onNode(tab and hasText(str(R.string.text_files_c79ddc2))).assertIsSelected()
        val target = files.getUnclippedBoundsInRoot()
        assertTrue(target.bottom - target.top >= 48.dp)
        val importAction = compose.onNodeWithContentDescription(str(R.string.text_import_files_2fd22c6)).assertIsDisplayed().getUnclippedBoundsInRoot()
        assertTrue(importAction.bottom <= bar.top)
        capture("navigation-files")
    }

    @Test fun pagesMoveAlongTheSharedAxisAndTheBackGestureRevealsTheParent() {
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val client = compose.activity.qaClient
        val parent = hasTestTag("membership-badge")
        val child = hasText(str(R.string.text_security_c4c7059))
        compose.runOnIdle { client.navigation.open("account") }
        compose.waitForIdle()

        compose.mainClock.autoAdvance = false
        compose.runOnIdle { client.navigation.push(Route("security")) }
        compose.mainClock.advanceTimeBy(64)
        val entering = compose.onNode(child).getUnclippedBoundsInRoot().left
        compose.mainClock.advanceTimeBy(1_000)
        val settled = compose.onNode(child).getUnclippedBoundsInRoot().left
        assertTrue("forward enters from the end: $entering > $settled", entering > settled)
        compose.onNode(parent).assertDoesNotExist()

        val dispatcher = compose.activity.onBackPressedDispatcher
        fun gesture(progress: Float) = BackEventCompat(progress * 600f, 1_300f, progress, BackEventCompat.EDGE_LEFT)
        compose.runOnUiThread { dispatcher.dispatchOnBackStarted(gesture(0f)); dispatcher.dispatchOnBackProgressed(gesture(0.4f)) }
        compose.mainClock.advanceTimeBy(64)
        compose.onNode(parent).assertExists()
        compose.onNode(child).assertExists()
        compose.runOnUiThread { dispatcher.dispatchOnBackCancelled() }
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNode(parent).assertDoesNotExist()
        assertEquals("security", client.navigation.state.value.current.page)

        compose.runOnUiThread { dispatcher.dispatchOnBackStarted(gesture(0f)); dispatcher.dispatchOnBackProgressed(gesture(0.6f)) }
        compose.mainClock.advanceTimeBy(64)
        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNode(child).assertDoesNotExist()
        compose.onNode(parent).assertExists()
        assertEquals("account", client.navigation.state.value.current.page)
        compose.mainClock.autoAdvance = true
    }

    @Test fun cancellingBackKeepsTheSelectedFileVersion() {
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val client = compose.activity.qaClient
        var seeded: FileDetails? = null
        compose.runOnIdle {
            client.perform({ parseFileDetails(it.seedUiFileHistory().decode()) }, key = "qa:file-history") { seeded = it }
        }
        compose.waitUntil(10_000) { seeded != null }
        val file = checkNotNull(seeded)
        val earlier = file.versions.first { !it.current }
        compose.runOnIdle { client.navigation.open("files"); client.navigation.select(file.file.id) }
        compose.waitUntil(10_000) { client.library.detail.value()?.file?.id == file.file.id }
        val versions = compose.onNodeWithText(str(R.string.text_versions_0ff2bdb))
        val beforeSwipe = versions.getUnclippedBoundsInRoot().top
        compose.onNodeWithTag("workspace-content").performTouchInput {
            swipeUp(startY = height * 0.75f, endY = height * 0.4f, durationMillis = 400)
        }
        assertTrue("A swipe must scroll the file details under the compact bar", versions.getUnclippedBoundsInRoot().top < beforeSwipe - 32.dp)
        compose.onNode(hasText(messageTime(earlier.createdAt)) and hasClickAction())
            .performSemanticsAction(SemanticsActions.OnClick) { it() }
        val restore = compose.onNodeWithText(str(R.string.text_make_this_the_current_version_ddcd50e))
        restore.assertExists()

        val dispatcher = compose.activity.onBackPressedDispatcher
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread {
            dispatcher.dispatchOnBackStarted(BackEventCompat(0f, 1_300f, 0f, BackEventCompat.EDGE_LEFT))
            dispatcher.dispatchOnBackProgressed(BackEventCompat(240f, 1_300f, 0.4f, BackEventCompat.EDGE_LEFT))
        }
        compose.mainClock.advanceTimeBy(64)
        compose.runOnUiThread { dispatcher.dispatchOnBackCancelled() }
        compose.mainClock.advanceTimeBy(1_000)
        compose.mainClock.autoAdvance = true
        assertEquals(file.file.id, client.navigation.state.value.current.id)
        restore.assertExists()
    }

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
        compose.onNodeWithText(str(R.string.text_multisig_wallets_451404c)).assertDoesNotExist()
        row(R.string.text_cached_data_b29fb6f).performClick()
        compose.runOnIdle { assertEquals("display-cache", client.navigation.state.value.current.page) }
        back()
        compose.runOnIdle { assertEquals("advanced", client.navigation.state.value.current.page) }
        capture("advanced")
        compose.runOnIdle { client.dark(true) }
        capture("advanced-dark")
        compose.runOnIdle { client.dark(false) }
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
