package net.dmsg.android

import android.graphics.Bitmap
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import net.dmsg.android.core.ClientController
import net.dmsg.android.core.HighlightSpan
import net.dmsg.android.ui.DMsgApp
import net.dmsg.android.ui.DMsgTheme
import net.dmsg.android.ui.kit.buildBubbleHighlightedText
import net.dmsg.core.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.io.FileOutputStream
import java.util.UUID

class SearchExperienceTest {
    @get:Rule val compose = createComposeRule()

    @Test fun highlightedBubbleKeepsOriginalOffsetsAfterCapitalDottedI() {
        val text = "İstanbul · report"
        val snippet = listOf(HighlightSpan("report", true))
        compose.setContent {
            DMsgTheme(isDark = false) {
                Text(buildBubbleHighlightedText(text, snippet))
            }
        }
        val rendered = compose.onNodeWithText(text).assertIsDisplayed()
            .fetchSemanticsNode().config[SemanticsProperties.Text].single()
        val mark = rendered.spanStyles.single()
        assertEquals("report", rendered.text.substring(mark.start, mark.end))
    }

    private fun saveShot(dir: File, name: String) {
        compose.waitForIdle()
        val bmp = compose.onRoot().captureToImage().asAndroidBitmap()
        dir.mkdirs()
        FileOutputStream(File(dir, "$name.png")).use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bmp.recycle()
    }

    @Test fun pinyinSearchHighlightsAndScopeToggleWorkAcrossThemesAndLayouts() {
        val instrument = InstrumentationRegistry.getInstrumentation()
        val app = instrument.targetContext.applicationContext as DMsgApplication
        val directory = File(net.dmsg.android.platform.PrivateFiles(app).workspace, "search-ui-" + UUID.randomUUID())
        val shotsDir = File(app.cacheDir, "search-shots")
        shotsDir.deleteRecursively()
        val client = ClientController(app) { paths, platform ->
            MobileEngine.open(directory.path, paths.staging.path, OfflineAccountTest.FixtureHardware(platform))
        }
        var rtlState by mutableStateOf(false)
        var fontScaleState by mutableStateOf(1.0f)
        try {
            compose.runOnUiThread { client.start() }
            compose.setContent {
                val baseDensity = LocalDensity.current
                CompositionLocalProvider(
                    LocalLayoutDirection provides if (rtlState) LayoutDirection.Rtl else LocalLayoutDirection.current,
                    LocalDensity provides Density(baseDensity.density, fontScale = fontScaleState),
                ) {
                    DMsgApp(client, {}, {}, { _, _ -> })
                }
            }
            compose.waitUntil(15_000) { client.state.value.session?.state == "logged_out" && !client.state.value.busy && !client.state.value.concealed }
            compose.onNodeWithTag("access.local").performClick()
            compose.waitUntil(15_000) { client.state.value.unlocked }
            compose.runOnIdle { client.dark(false) }

            // Seed Chinese Vault items (one matching in notes for V01 check) and a Chinese folder via real Rust core.
            var seeded = false
            var seedError: String? = null
            compose.runOnIdle {
                client.perform({ core ->
                    core.commitVaultDraft(
                        VaultDraftInput("11".repeat(32), null, null, 1uL, "{}", false),
                        VaultEdit(
                            title = "招商银行企业版",
                            kind = "login",
                            notes = "北京总部开户行机密备注",
                            favorite = true,
                            tags = listOf("财务", "银行"),
                            fields = listOf(FieldEdit("username", "text", "Username", "", "finance@example.com")),
                            files = emptyList(),
                        ),
                    )
                    core.commitVaultDraft(
                        VaultDraftInput("22".repeat(32), null, null, 1uL, "{}", false),
                        VaultEdit(
                            title = "北京出差报销指南",
                            kind = "note",
                            notes = "差旅标准",
                            favorite = false,
                            tags = listOf("差旅"),
                            fields = emptyList(),
                            files = emptyList(),
                        ),
                    )
                    core.createFolder("root", "北京财务资料")
                }, key = "test:seed", quiet = true, onError = { seedError = it }) { seeded = true }
            }
            compose.waitUntil(15_000) { seeded || seedError != null }
            assertNull("Seed failed: $seedError", seedError)

            // 1. Activate real Vault search field and type pinyin initials ("zsyh") -> matches "招商银行企业版"
            compose.onNodeWithTag("browse-search-entry").performClick()
            compose.waitUntil(5_000) { compose.onAllNodesWithTag("browse-search-editor").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithTag("browse-search-editor").performTextInput("zsyh")
            compose.waitUntil(10_000) {
                compose.onAllNodesWithText("招商银行企业版", substring = true).fetchSemanticsNodes().isNotEmpty()
            }
            compose.onNodeWithText("招商银行企业版", substring = true).assertIsDisplayed()

            // 2. Replace with pinyin ("bj") -> matches both title ("北京出差报销指南") and notes ("招商银行企业版" with V01 hint)
            compose.onNodeWithTag("browse-search-editor").performTextReplacement("bj")
            compose.waitUntil(10_000) {
                compose.onAllNodesWithText("北京出差报销指南", substring = true).fetchSemanticsNodes().isNotEmpty() &&
                    compose.onAllNodesWithText(app.getString(R.string.search_matched_notes), substring = true).fetchSemanticsNodes().isNotEmpty()
            }
            // V01 invariant: notes plaintext must never appear in list view.
            assertTrue(compose.onAllNodesWithText("北京总部开户行机密备注", substring = true).fetchSemanticsNodes().isEmpty())
            compose.onNodeWithTag("browse-search-editor").performImeAction()
            saveShot(shotsDir, "vault-light")

            // 3. Switch scope to "All" -> unified GlobalSearchResults shows Vault + Folder ("北京财务资料")
            compose.onNodeWithText(app.getString(R.string.search_scope_all)).performClick()
            compose.waitUntil(10_000) {
                compose.onAllNodesWithText("北京财务资料", substring = true).fetchSemanticsNodes().isNotEmpty()
            }
            compose.onNodeWithText("北京财务资料", substring = true).assertIsDisplayed()
            saveShot(shotsDir, "global-light")

            // 4. Dark theme screenshot
            compose.runOnIdle { client.dark(true) }
            saveShot(shotsDir, "global-dark")

            // 5. RTL layout screenshot
            compose.runOnIdle { client.dark(false); rtlState = true }
            saveShot(shotsDir, "global-rtl")

            // 6. 200% font scale screenshot
            compose.runOnIdle { rtlState = false; fontScaleState = 2.0f }
            saveShot(shotsDir, "global-font200")
        } finally {
            runBlocking { withContext(Dispatchers.Main.immediate) { client.shutdown() } }
            directory.deleteRecursively()
        }
    }
}
