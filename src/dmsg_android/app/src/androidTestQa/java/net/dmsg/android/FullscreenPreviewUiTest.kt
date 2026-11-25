package net.dmsg.android

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.view.ViewTreeObserver
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.core.content.FileProvider
import androidx.core.view.WindowCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.core.LoadedPreviewContent
import net.dmsg.android.core.PreviewFilter
import net.dmsg.android.platform.AppLanguage
import net.dmsg.android.platform.str
import org.junit.After
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

/** End-to-end fullscreen preview instrumentation & visual matrix against the real offline Rust core. */
@RunWith(AndroidJUnit4::class)
class FullscreenPreviewUiTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @After fun restore() {
        compose.activityRule.scenario.onActivity { activity ->
            AppLanguage.choose(activity, "en")
            activity.qaClient.systemAppearance()
        }
    }

    private fun seedPreviewFiles(secondImage: String? = null): Triple<String, String, String> {
        val client = compose.activity.qaClient
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(app.cacheDir, "dmsg-shared/preview-qa-${UUID.randomUUID()}").apply { mkdirs() }

        val pngName = "01-design-mockup.png"
        val pngFile = File(dir, pngName)
        val bitmap = Bitmap.createBitmap(900, 600, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        canvas.drawColor(Color.rgb(20, 92, 69))
        val cardPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.rgb(251, 253, 248) }
        canvas.drawRoundRect(250f, 100f, 650f, 500f, 64f, 64f, cardPaint)
        val dotPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.rgb(20, 92, 69) }
        canvas.drawCircle(450f, 300f, 88f, dotPaint)
        pngFile.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        val nextImage = secondImage?.let { name ->
            canvas.drawColor(Color.WHITE)
            File(dir, name).also { file ->
                file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            }
        }
        bitmap.recycle()

        val mdName = "02-architecture-notes.md"
        val mdFile = File(dir, mdName)
        mdFile.writeText(
            """
            # dMsg 全屏预览架构说明

            Supports **Markdown reading**, *italic*, ~~removed~~ and `dmsg preview`.

            > Raw HTML is stripped and external images stay blocked.

            - [x] Pinch & double-tap image zoom
            - [x] Filter by All, Previewable, Images, Documents
            - [ ] Future formats on demand

            | Type | Limit | Renderer |
            | :--- | :---: | ---: |
            | Image | 20 MiB | Bounded |
            | Markdown | 2 MiB | Safe AST |

            ```kotlin
            val maxBytes = PreviewFilterLogic.MAX_MARKDOWN_BYTES
            println(maxBytes)
            ```

            ![External diagram](https://example.invalid/diagram.png)

            See [dMsg Docs](https://dmsg.net) for details.
            """.trimIndent(),
        )

        val pdfName = "03-quarterly-report.pdf"
        val pdfFile = File(dir, pdfName)
        pdfFile.writeBytes("%PDF-1.4\n1 0 obj <<>> endobj\ntrailer <<>>\n%%EOF\n".toByteArray())

        val uris = listOfNotNull(pngFile, nextImage, mdFile, pdfFile).map { file ->
            FileProvider.getUriForFile(app, app.packageName + ".files", file)
        }
        compose.runOnIdle {
            client.navigation.open("files")
            assertTrue(client.transfers.beginImport("root"))
            client.transfers.importResults(uris)
        }
        compose.waitUntil(25_000) {
            client.transfers.tasks.value.values.any { it.kind == "import" && it.phase == "done" } &&
                client.library.listing.value()?.files?.let { files ->
                    files.any { it.name == pngName } && files.any { it.name == mdName } && files.any { it.name == pdfName } &&
                        (secondImage == null || files.any { it.name == secondImage })
                } == true
        }
        dir.deleteRecursively()
        return Triple(pngName, mdName, pdfName)
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
    }

    @Test fun statusBarStaysLightOnImagesAndRestoresAfterClosing() {
        val client = compose.activity.qaClient
        compose.waitUntil(20_000) { client.state.value.unlocked }
        val second = "01-next.png"
        val (first) = seedPreviewFiles(second)
        compose.runOnIdle { client.dark(false); client.navigation.open("files") }
        compose.onNodeWithText(first).performClick()
        fun waitForImage(name: String) = compose.waitUntil(15_000) {
            (client.fullscreenPreview.session.value?.content as? LoadedPreviewContent.Image)?.name == name
        }
        waitForImage(first)
        val window = compose.activity.window
        val decor = window.decorView
        val bars = WindowCompat.getInsetsController(window, decor)
        var sawDarkIcons = false
        val onDraw = ViewTreeObserver.OnDrawListener {
            if (bars.isAppearanceLightStatusBars) sawDarkIcons = true
        }
        compose.runOnIdle {
            assertFalse(bars.isAppearanceLightStatusBars)
            decor.viewTreeObserver.addOnDrawListener(onDraw)
        }
        try {
            compose.onNodeWithTag("fullscreen-preview").performTouchInput { swipeLeft() }
            waitForImage(second)
            compose.onNodeWithTag("fullscreen-preview").performTouchInput { swipeRight() }
            waitForImage(first)
            compose.runOnIdle { assertFalse("Dark status icons appeared during image paging", sawDarkIcons) }
        } finally {
            compose.runOnIdle { decor.viewTreeObserver.removeOnDrawListener(onDraw) }
        }
        compose.onNodeWithTag("preview-close").performClick()
        compose.runOnIdle { assertTrue(bars.isAppearanceLightStatusBars) }
    }

    @Test fun fullscreenPreviewFilterMarkdownAndVisualMatrix() {
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val (pngName, mdName, pdfName) = seedPreviewFiles()

        // 1. English light & dark + full interactive flow (image -> filter -> markdown -> source -> fallback -> close)
        val client = compose.activity.qaClient
        compose.runOnIdle { client.dark(false); client.navigation.open("files") }
        compose.onNodeWithText(pngName).assertIsDisplayed().performClick()
        compose.waitUntil(15_000) { client.fullscreenPreview.session.value?.content is LoadedPreviewContent.Image }
        compose.onNodeWithTag("fullscreen-preview").assertExists()
        capture("android-preview-image-en-light")

        compose.runOnIdle { client.dark(true) }
        capture("android-preview-image-en-dark")
        compose.runOnIdle { client.dark(false) }

        // Filter to Documents -> jumps directly to Markdown document.
        compose.onNodeWithTag("preview-filter").performClick()
        compose.onNode(hasText(str(R.string.preview_filter_documents), substring = true) and hasClickAction()).performClick()
        compose.waitUntil(15_000) { client.fullscreenPreview.session.value?.content is LoadedPreviewContent.Markdown }
        assertEquals(PreviewFilter.DOCUMENTS, client.fullscreenPreview.session.value?.filter)
        compose.onNodeWithText("dMsg 全屏预览架构说明", substring = true).assertExists()
        capture("android-preview-markdown-en-light")

        compose.runOnIdle { client.dark(true) }
        capture("android-preview-markdown-en-dark")
        compose.runOnIdle { client.dark(false) }

        // Toggle Markdown Reading <-> Source
        compose.onNodeWithTag("preview-markdown-mode").performClick()
        compose.onNodeWithText("# dMsg 全屏预览架构说明", substring = true).assertExists()
        compose.onNodeWithTag("preview-markdown-mode").performClick()
        compose.onNodeWithText("dMsg 全屏预览架构说明", substring = true).assertExists()

        // Filter back to All, switch to unsupported PDF -> renders fallback metadata card.
        compose.onNodeWithTag("preview-filter").performClick()
        compose.onNode(hasText(str(R.string.text_all_eae0ae1), substring = true) and hasClickAction()).performClick()
        compose.waitForIdle()
        assertEquals(mdName, client.fullscreenPreview.session.value?.activeItem?.name)
        compose.onNodeWithTag("fullscreen-preview").performTouchInput { swipeLeft() }
        compose.waitUntil(15_000) { client.fullscreenPreview.session.value?.content is LoadedPreviewContent.Fallback }
        assertEquals(pdfName, client.fullscreenPreview.session.value?.activeItem?.name)
        compose.onNodeWithTag("preview-fallback").assertExists()
        capture("android-preview-fallback-en-light")

        // Close via top bar button
        compose.onNodeWithTag("preview-close").performClick()
        compose.waitUntil(5_000) { client.fullscreenPreview.session.value == null }

        // 2. Simplified Chinese (zh-Hans) light & dark
        switchLanguage("zh-Hans")
        val zhClient = compose.activity.qaClient
        compose.runOnIdle { zhClient.dark(false); zhClient.navigation.open("files") }
        compose.waitUntil(10_000) { zhClient.library.listing.value()?.files?.any { it.name == mdName } == true }
        compose.onNodeWithText(mdName).performClick()
        compose.waitUntil(15_000) { zhClient.fullscreenPreview.session.value?.content is LoadedPreviewContent.Markdown }
        capture("android-preview-markdown-zh-light")
        compose.runOnIdle { zhClient.dark(true) }
        capture("android-preview-markdown-zh-dark")

        val zhPdf = zhClient.fullscreenPreview.session.value!!.allItems.first { it.name == pdfName }
        compose.runOnIdle { zhClient.dark(false); zhClient.fullscreenPreview.select(zhPdf.id) }
        compose.waitUntil(15_000) { zhClient.fullscreenPreview.session.value?.content is LoadedPreviewContent.Fallback }
        capture("android-preview-fallback-zh-light")
        compose.runOnIdle { zhClient.fullscreenPreview.close() }

        // 3. Arabic RTL (ar) light
        switchLanguage("ar")
        assertEquals(android.view.View.LAYOUT_DIRECTION_RTL, compose.activity.resources.configuration.layoutDirection)
        val arClient = compose.activity.qaClient
        compose.runOnIdle { arClient.dark(false); arClient.navigation.open("files") }
        compose.waitUntil(10_000) { arClient.library.listing.value()?.files?.any { it.name == mdName } == true }
        compose.onNodeWithText(mdName).performClick()
        compose.waitUntil(15_000) { arClient.fullscreenPreview.session.value?.content is LoadedPreviewContent.Markdown }
        capture("android-preview-markdown-ar-light")

        val arImg = arClient.fullscreenPreview.session.value!!.allItems.first { it.name == pngName }
        compose.runOnIdle { arClient.fullscreenPreview.select(arImg.id) }
        compose.waitUntil(15_000) { arClient.fullscreenPreview.session.value?.content is LoadedPreviewContent.Image }
        capture("android-preview-image-ar-light")
        compose.runOnIdle { arClient.fullscreenPreview.close() }
    }

    @Test fun fullscreenPreviewAtLargeFontScale() {
        val previousScale = shell("settings get system font_scale").trim()
        try {
            shell("settings put system font_scale 2.0")
            compose.waitUntil(15_000) { compose.activity.resources.configuration.fontScale >= 1.99f && compose.activity.hasWindowFocus() }
            checkLargeFontPreview()
        } finally {
            shell(if (previousScale == "null") "settings delete system font_scale" else "settings put system font_scale $previousScale")
        }
    }

    private fun checkLargeFontPreview() {
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val (_, mdName, pdfName) = seedPreviewFiles()
        val client = compose.activity.qaClient
        compose.runOnIdle { client.dark(false); client.navigation.open("files") }
        compose.onNodeWithText(mdName).performClick()
        compose.waitUntil(15_000) { client.fullscreenPreview.session.value?.content is LoadedPreviewContent.Markdown }
        capture("android-preview-markdown-200pct-light")

        val pdfItem = client.fullscreenPreview.session.value!!.allItems.first { it.name == pdfName }
        compose.runOnIdle { client.fullscreenPreview.select(pdfItem.id) }
        compose.waitUntil(15_000) { client.fullscreenPreview.session.value?.content is LoadedPreviewContent.Fallback }
        capture("android-preview-fallback-200pct-light")
        compose.runOnIdle { client.fullscreenPreview.close() }
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
        shell("mkdir -p /sdcard/Download/dmsg-preview-qa")
        shell("cp ${file.absolutePath} /sdcard/Download/dmsg-preview-qa/$name.png")
    }

    private fun shell(command: String): String = android.os.ParcelFileDescriptor.AutoCloseInputStream(
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
    ).use { it.readBytes().toString(Charsets.UTF_8) }
}
