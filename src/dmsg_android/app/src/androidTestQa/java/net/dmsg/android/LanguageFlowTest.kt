package net.dmsg.android

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.core.Workspace
import net.dmsg.android.platform.AppLanguage
import net.dmsg.android.platform.str
import org.junit.After
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File

/** Real Activity recreation and localized UI; the account itself is an offline fixture. */
class LanguageFlowTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @After fun restoreLanguage() { compose.activityRule.scenario.onActivity { AppLanguage.choose(it, "en") } }

    @Test fun allSupportedLanguagesKeepTheAccountAndPositionAcrossActivityRecreation() {
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val client = compose.activity.qaClient
        val account = client.state.value.session!!.account
        compose.runOnIdle { client.dark(false); client.navigation.open("account") }
        val tags = InstrumentationRegistry.getArguments().getString("languageTags")?.split(',') ?: AppLanguage.supported.keys
        check(tags.isNotEmpty() && tags.all { it in AppLanguage.supported })
        for (tag in tags) {
            val direction = android.text.TextUtils.getLayoutDirectionFromLocale(java.util.Locale.forLanguageTag(tag))
            val previousActivity = compose.activity
            val recreates = previousActivity.resources.configuration.locales[0].toLanguageTag() != tag
            val expectedLanguage = previousActivity.createConfigurationContext(
                android.content.res.Configuration(previousActivity.resources.configuration).apply {
                    setLocales(android.os.LocaleList.forLanguageTags(tag))
                }).getString(R.string.app_language)
            compose.onNodeWithText(str(R.string.app_language)).performScrollTo().performClick()
            compose.onNodeWithTag("language-search").performTextInput(tag)
            compose.onNodeWithTag("language-$tag").performClick()
            try {
                compose.waitUntil(15_000) {
                    (!recreates || compose.activity !== previousActivity) &&
                        AppLanguage.selected() == tag &&
                        AppLanguage.locale().toLanguageTag() == tag &&
                        compose.activity.resources.configuration.locales[0].toLanguageTag() == tag &&
                        compose.activity.hasWindowFocus() &&
                        compose.onNodeWithText(expectedLanguage).isDisplayed() &&
                        compose.onAllNodesWithTag("language-picker").fetchSemanticsNodes().isEmpty()
                }
            } catch (error: Throwable) {
                val locale = compose.activity.resources.configuration.locales.toLanguageTags()
                throw AssertionError("Switch to $tag: selected=${AppLanguage.selected()} locale=$locale " +
                    "focus=${compose.activity.hasWindowFocus()} route=${client.navigation.state.value.current} " +
                    "scroll=${client.navigation.scrollPosition("account:index")}\n" +
                    compose.onRoot().printToString().take(10_000), error)
            }
            assertSame(client, compose.activity.qaClient)
            assertEquals(account, client.state.value.session!!.account)
            assertEquals(Workspace.ACCOUNT, client.navigation.state.value.workspace)
            // Compose semantics can be ready before the replacement Window's
            // transition finishes. Use the same accessibility/PixelCopy barrier
            // as the screenshot proof before asserting final visibility.
            capture("$tag-account", expectedLanguage)
            try { compose.onNodeWithText(expectedLanguage).assertIsDisplayed() }
            catch (error: Throwable) {
                val row = compose.onNodeWithText(str(R.string.app_language)).fetchSemanticsNode()
                val insets = compose.activity.window.decorView.rootWindowInsets
                val metadata = "Locale=$tag row=${row.boundsInWindow} root=${compose.onRoot().fetchSemanticsNode().boundsInWindow} " +
                    "scroll=${client.navigation.scrollPosition("account:index")} " +
                    "ime=${insets?.getInsets(android.view.WindowInsets.Type.ime())} focus=${compose.activity.hasWindowFocus()}"
                saveBitmap("$tag-account-failure")
                throw AssertionError(metadata + "\n" + compose.onRoot().printToString().take(10_000), error)
            }
            assertEquals(direction, compose.activity.resources.configuration.layoutDirection)
            compose.runOnIdle { client.navigation.open("vault") }
            compose.onNodeWithText("Studio account").assertIsDisplayed()
            val vaultX = compose.onAllNodesWithText(str(R.string.text_vault_ede2a06)).onLast().fetchSemanticsNode().boundsInWindow.center.x
            val accountX = compose.onAllNodesWithText(str(R.string.text_account_4ab56e9)).onLast().fetchSemanticsNode().boundsInWindow.center.x
            assertTrue("Navigation order must follow $tag", if (direction == android.view.View.LAYOUT_DIRECTION_RTL) vaultX > accountX else vaultX < accountX)
            capture("$tag-vault", "Studio account")
            compose.onNodeWithText("Studio account").performClick()
            try {
                compose.waitUntil(10_000) { compose.onAllNodesWithText(str(R.string.vault_field_username)).fetchSemanticsNodes().isNotEmpty() }
            } catch (error: Throwable) {
                val tree = runCatching { compose.onRoot().printToString().take(10_000) }.getOrDefault("No Compose root")
                throw AssertionError("Locale=$tag actual=${compose.activity.resources.configuration.locales.toLanguageTags()} " +
                    "focus=${compose.activity.hasWindowFocus()} route=${client.navigation.state.value.current} " +
                    "unlocked=${client.state.value.unlocked} concealed=${client.state.value.concealed} " +
                    "detail=${client.vault.detail.state.value.javaClass.simpleName}/${client.vault.detail.value()?.id}\n$tree", error)
            }
            compose.onNodeWithText("Sample-only password").assertDoesNotExist()
            if (tag == "ar") {
                val created = java.util.Calendar.getInstance().apply { timeInMillis = client.vault.detail.value()!!.createdAt.toLong() }
                val year = java.text.NumberFormat.getIntegerInstance(java.util.Locale.forLanguageTag(tag)).apply { isGroupingUsed = false }.format(created.get(java.util.Calendar.YEAR))
                // Item information shows a relative time; a tap reveals the full date.
                compose.onNodeWithText(str(R.string.text_created_36ba304)).performScrollTo().performClick()
                assertTrue("Date digits follow the selected Arabic locale", compose.onAllNodes(hasText(year, substring = true)).fetchSemanticsNodes().isNotEmpty())
            }
            capture("$tag-detail", "demo@example.test")
            compose.onNodeWithContentDescription(str(R.string.text_back_5db5cac)).performClick()
            compose.runOnIdle { client.navigation.open("files") }
            compose.waitUntil(10_000) { compose.onAllNodesWithText(str(R.string.text_files_are_encrypted_locally_cloud_backup_is_a_separate_explicit_a_28bc23b)).fetchSemanticsNodes().isNotEmpty() }
            capture("$tag-files", "Project materials")
            compose.runOnIdle { client.navigation.open("account") }
        }
    }

    private fun capture(name: String, visibleText: String) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        compose.waitForIdle(); instrumentation.waitForIdleSync()
        fun containsText(node: android.view.accessibility.AccessibilityNodeInfo?, depth: Int = 0): Boolean {
            if (node == null || depth > 20) return false
            if (node.text?.toString()?.contains(visibleText) == true) return true
            return (0 until node.childCount).any { containsText(node.getChild(it), depth + 1) }
        }
        try { compose.waitUntil(10_000) {
            containsText(instrumentation.uiAutomation.rootInActiveWindow)
        } } catch (error: Throwable) {
            val description = StringBuilder("Expected: $visibleText / $name\n")
            fun inspect(node: android.view.accessibility.AccessibilityNodeInfo?, depth: Int = 0) {
                if (node == null || depth > 20) return
                description.append(" ".repeat(depth)).append(node.className).append(" / ").append(node.text).append(" / ").append(node.contentDescription).append('\n')
                for (i in 0 until node.childCount) inspect(node.getChild(i), depth + 1)
            }
            inspect(instrumentation.uiAutomation.rootInActiveWindow)
            throw AssertionError(description.toString(), error)
        }
        Thread.sleep(300) // SurfaceFlinger/Activity transitions follow the Compose semantics frame.
        // PixelCopy waits for this Compose root to draw after Activity recreation.
        // This capture deliberately contains the app root, not the system bars.
        saveBitmap(name)
    }

    private fun saveBitmap(name: String) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val file = File(instrumentation.targetContext.getExternalFilesDir(null), "locale-$name.png")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }; bitmap.recycle()
        for (command in listOf("mkdir -p /sdcard/Download/dmsg-locale-qa", "cp ${file.absolutePath} /sdcard/Download/dmsg-locale-qa/${file.name}")) {
            android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(command)).use { it.readBytes() }
        }
    }
}
