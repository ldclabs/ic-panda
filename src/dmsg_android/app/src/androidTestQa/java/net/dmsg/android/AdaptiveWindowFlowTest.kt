package net.dmsg.android

import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import androidx.window.layout.FoldingFeature
import androidx.window.testing.layout.FoldingFeature as TestFold
import androidx.window.testing.layout.TestWindowLayoutInfo as TestLayout
import androidx.window.testing.layout.WindowLayoutInfoPublisherRule
import net.dmsg.android.core.Workspace
import net.dmsg.android.platform.AppLanguage
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File

/** Actual window recreation and tracker integration, with an injected fold sensor. */
class AdaptiveWindowFlowTest {
    @get:Rule(order = 0) val folds = WindowLayoutInfoPublisherRule()
    @get:Rule(order = 1) val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @Test fun rotationFoldAndSystemLargeTextRetainTheOriginalAccountAndSelection() {
        val originalFont = shell("settings get system font_scale").trim()
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val client = compose.activity.qaClient
        val account = client.state.value.session!!.account
        compose.runOnIdle { client.dark(false); client.navigation.open("vault") }
        compose.onNodeWithText("Studio account").performClick()
        val selected = client.navigation.state.value.current.id
        assertNotNull(selected)
        try {
            compose.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
            compose.waitUntil(15_000) {
                compose.activity.resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE &&
                    compose.onAllNodesWithTag("workspace-panes").fetchSemanticsNodes().size == 1
            }
            assertSame(client, compose.activity.qaClient)
            assertEquals(selected, client.navigation.state.value.current.id)
            lateinit var hinge: android.graphics.Rect
            compose.runOnIdle {
                val feature = TestFold(compose.activity, -1, 24, FoldingFeature.State.HALF_OPENED, FoldingFeature.Orientation.VERTICAL)
                hinge = feature.bounds
                folds.overrideWindowLayoutInfo(TestLayout(listOf(feature)))
            }
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("workspace-hinge").fetchSemanticsNodes().size == 1 }
            for (workspace in Workspace.entries) {
                compose.runOnIdle { client.navigation.open(workspace.route) }
                compose.onNodeWithTag("workspace-panes").assertExists()
                val list = compose.onNodeWithTag("workspace-list").fetchSemanticsNode().boundsInWindow
                val detail = compose.onNodeWithTag("workspace-detail").fetchSemanticsNode().boundsInWindow
                assertTrue(workspace.name, list.right <= hinge.left + 1)
                assertTrue(workspace.name, detail.left >= hinge.right - 1)
                assertEquals(account, client.state.value.session!!.account)
                capture("landscape-hinge-${workspace.route}")
            }
            compose.activityRule.scenario.onActivity { AppLanguage.choose(it, "ar") }
            compose.waitUntil(15_000) { compose.activity.window.decorView.layoutDirection == android.view.View.LAYOUT_DIRECTION_RTL }
            compose.runOnIdle {
                val feature = TestFold(compose.activity, -1, 24, FoldingFeature.State.HALF_OPENED, FoldingFeature.Orientation.VERTICAL)
                hinge = feature.bounds
                folds.overrideWindowLayoutInfo(TestLayout(listOf(feature)))
            }
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("workspace-hinge").fetchSemanticsNodes().size == 1 }
            val rtlIndex = compose.onNodeWithTag("workspace-list").fetchSemanticsNode().boundsInWindow
            val rtlDetail = compose.onNodeWithTag("workspace-detail").fetchSemanticsNode().boundsInWindow
            assertTrue(rtlIndex.left >= hinge.right - 1)
            assertTrue(rtlDetail.right <= hinge.left + 1)
            capture("arabic-landscape-hinge-account")

            shell("settings put system font_scale 2.0")
            compose.waitUntil(15_000) {
                compose.activity.resources.configuration.fontScale >= 1.99f &&
                    compose.onAllNodesWithTag("workspace-panes").fetchSemanticsNodes().isEmpty()
            }
            // A real tracker emits the current fold to the recreated Activity.
            compose.runOnIdle {
                val feature = TestFold(compose.activity, -1, 24, FoldingFeature.State.HALF_OPENED, FoldingFeature.Orientation.VERTICAL)
                hinge = feature.bounds
                folds.overrideWindowLayoutInfo(TestLayout(listOf(feature)))
            }
            compose.waitUntil(10_000) {
                val area = compose.onNodeWithTag("workspace-content").fetchSemanticsNode().boundsInWindow
                area.right <= hinge.left + 1 || area.left >= hinge.right - 1
            }
            assertSame(client, compose.activity.qaClient)
            assertEquals(account, client.state.value.session!!.account)
            capture("arabic-system-200-percent")
            compose.runOnIdle { folds.overrideWindowLayoutInfo(TestLayout(emptyList())); client.navigation.open("vault") }
            assertEquals(selected, client.navigation.state.value.current.id)
            compose.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }
            compose.waitUntil(15_000) { compose.activity.resources.configuration.orientation == Configuration.ORIENTATION_PORTRAIT }
            assertEquals(selected, client.navigation.state.value.current.id)
            assertEquals(account, client.state.value.session!!.account)
            capture("arabic-portrait-200-percent-retained-selection")
        } finally {
            shell(if (originalFont == "null") "settings delete system font_scale" else "settings put system font_scale $originalFont")
            compose.activityRule.scenario.onActivity { AppLanguage.choose(it, "en"); it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }
        }
    }

    private fun shell(command: String): String = android.os.ParcelFileDescriptor.AutoCloseInputStream(
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
    ).use { it.readBytes().toString(Charsets.UTF_8) }

    private fun capture(name: String) {
        compose.waitForIdle()
        val image = compose.onRoot().captureToImage().asAndroidBitmap()
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File(app.getExternalFilesDir(null), "$name.png")
        file.outputStream().use { image.compress(Bitmap.CompressFormat.PNG, 100, it) }; image.recycle()
        shell("mkdir -p /sdcard/Download/dmsg-window-qa")
        shell("cp ${file.absolutePath} /sdcard/Download/dmsg-window-qa/${file.name}")
    }
}
