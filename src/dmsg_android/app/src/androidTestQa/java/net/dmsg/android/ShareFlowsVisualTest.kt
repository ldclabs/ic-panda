package net.dmsg.android

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.core.content.FileProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.core.ReadState
import net.dmsg.android.core.Route
import net.dmsg.android.platform.AppLanguage
import net.dmsg.android.platform.str
import org.junit.After
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

/** Verifies unified File Detail sharing section and Vault Snapshot Composer (Link/Channel/Nearby). */
@RunWith(AndroidJUnit4::class)
class ShareFlowsVisualTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @After fun restore() {
        compose.activityRule.scenario.onActivity { activity ->
            AppLanguage.choose(activity, "en")
            activity.qaClient.systemAppearance()
        }
    }

    private fun seedFile(name: String = "01-design-mockup.png"): String {
        val client = compose.activity.qaClient
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(app.cacheDir, "dmsg-shared/share-qa-${UUID.randomUUID()}").apply { mkdirs() }
        val file = File(dir, name)
        val bitmap = Bitmap.createBitmap(320, 240, Bitmap.Config.ARGB_8888)
        Canvas(bitmap).drawColor(Color.rgb(20, 92, 69))
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
        val uri = FileProvider.getUriForFile(app, app.packageName + ".files", file)
        compose.runOnIdle {
            client.navigation.open("files")
            assertTrue(client.transfers.beginImport("root"))
            client.transfers.importResult(uri)
        }
        compose.waitUntil(20_000) {
            client.transfers.tasks.value.values.any { it.kind == "import" && it.phase == "done" } &&
                client.library.listing.value()?.files?.any { it.name == name } == true
        }
        dir.deleteRecursively()
        return client.library.listing.value()!!.files.first { it.name == name }.id
    }

    private fun switchLanguage(tag: String) {
        val previous = compose.activity
        val recreates = previous.resources.configuration.locales[0].toLanguageTag() != tag
        compose.activityRule.scenario.onActivity { AppLanguage.choose(it, tag) }
        compose.waitUntil(15_000) {
            (!recreates || compose.activity !== previous) &&
                AppLanguage.selected() == tag &&
                compose.activity.resources.configuration.locales[0].toLanguageTag() == tag &&
                compose.activity.hasWindowFocus()
        }
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
    }

    private fun verifyAndCapture(prefix: String, captureDark: Boolean) {
        val client = compose.activity.qaClient
        compose.waitUntil(20_000) { client.state.value.unlocked }
        val fileId = seedFile()
        compose.runOnIdle {
            client.dark(false)
            client.navigation.push(Route("files", id = fileId))
        }
        compose.waitUntil(10_000) { client.library.detail.value()?.file?.id == fileId }
        compose.onNodeWithText(str(R.string.share_section)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(str(R.string.nearby_share)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(str(R.string.text_share_by_link_05aafef)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(str(R.string.share_to_another_app)).performScrollTo().assertIsDisplayed()
        capture("share-file-$prefix-light")
        if (captureDark) {
            compose.runOnIdle { client.dark(true) }
            capture("share-file-$prefix-dark")
            compose.runOnIdle { client.dark(false) }
        }

        // Vault Detail -> single Share secret snapshot menu item -> switch Destination to Share nearby
        compose.runOnIdle { client.navigation.open("vault") }
        compose.waitUntil(10_000) { client.vault.listing.value()?.items?.any { it.title == "Studio account" } == true }
        compose.onNodeWithText("Studio account").performClick()
        compose.waitUntil(15_000) {
            client.navigation.state.value.current.let { it.page == "vault" && !it.id.isNullOrEmpty() } &&
                client.vault.detail.state.value is ReadState.Ready &&
                compose.onAllNodesWithContentDescription(str(R.string.more)).fetchSemanticsNodes().size == 1
        }
        compose.onNodeWithContentDescription(str(R.string.more)).performClick()
        compose.onNodeWithText(str(R.string.nearby_share)).assertDoesNotExist()
        compose.onNodeWithText(str(R.string.text_share_secret_snapshot_d4484d8)).performClick()
        compose.waitUntil(10_000) { client.secretSharing.draft.value != null }
        compose.onNodeWithText(str(R.string.text_share_link_b335ffa)).performClick()
        compose.onNodeWithText(str(R.string.nearby_share)).performClick()
        compose.waitUntil(5_000) { client.secretSharing.draft.value?.nearby == true }
        compose.onNodeWithText(str(R.string.text_create_link_2ae3b5b)).assertDoesNotExist()
        compose.onNodeWithText(str(R.string.nearby_scan_qr)).performScrollTo().assertIsDisplayed()
        capture("share-vault-nearby-$prefix-light")
        if (captureDark) {
            compose.runOnIdle { client.dark(true) }
            capture("share-vault-nearby-$prefix-dark")
            compose.runOnIdle { client.dark(false) }
        }
        compose.runOnIdle {
            client.secretSharing.discardDraft()
            client.navigation.back()
            client.navigation.back()
        }
    }

    @Test fun unifiedShareFlowsAcrossLanguagesThemesAndRtl() {
        verifyAndCapture("en", captureDark = true)
        switchLanguage("zh-Hans")
        verifyAndCapture("zh", captureDark = true)
        switchLanguage("ar")
        assertEquals(android.view.View.LAYOUT_DIRECTION_RTL, compose.activity.resources.configuration.layoutDirection)
        verifyAndCapture("ar", captureDark = false)
    }

    @Test fun unifiedShareFlowsAtLargeFontScale() {
        val previousScale = shell("settings get system font_scale").trim()
        try {
            shell("settings put system font_scale 2.0")
            compose.waitUntil(15_000) { compose.activity.resources.configuration.fontScale >= 1.99f && compose.activity.hasWindowFocus() }
            verifyAndCapture("200pct", captureDark = false)
        } finally {
            shell(if (previousScale == "null") "settings delete system font_scale" else "settings put system font_scale $previousScale")
        }
    }

    private fun capture(name: String) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        compose.waitForIdle()
        instrumentation.waitForIdleSync()
        Thread.sleep(300)
        val bitmap = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
        val app = instrumentation.targetContext
        val file = File(app.getExternalFilesDir(null), "$name.png")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
        shell("mkdir -p /sdcard/Download/dmsg-share-qa")
        shell("cp ${file.absolutePath} /sdcard/Download/dmsg-share-qa/$name.png")
    }

    private fun shell(command: String): String = android.os.ParcelFileDescriptor.AutoCloseInputStream(
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
    ).use { it.readBytes().toString(Charsets.UTF_8) }
}
