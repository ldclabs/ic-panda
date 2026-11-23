package net.dmsg.android

import android.graphics.Bitmap
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Key
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.ui.*
import net.dmsg.android.ui.kit.*
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

/**
 * Renders the component kit in light, dark and right-to-left at 200% text, checks touch targets
 * and saves screenshots to the app's external files (`kit-gallery/`) and /sdcard/Download/dmsg-kit-gallery for visual review.
 */
class KitGalleryTest {
    @get:Rule val compose = createComposeRule()

    private data class Look(val name: String, val dark: Boolean, val rtl: Boolean, val fontScale: Float)

    private val looks = listOf(
        Look("light", dark = false, rtl = false, fontScale = 1f),
        Look("dark", dark = true, rtl = false, fontScale = 1f),
        Look("rtl-200", dark = false, rtl = true, fontScale = 2f),
    )

    @Test fun kitRendersInEveryLookWithFullTouchTargets() {
        var look by mutableStateOf(looks.first())
        var page by mutableIntStateOf(0)
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(
                LocalDensity provides Density(density.density, look.fontScale),
                LocalLayoutDirection provides if (look.rtl) LayoutDirection.Rtl else LayoutDirection.Ltr,
            ) {
                DMsgTheme(look.dark) {
                    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.page) {
                        Column(Modifier.padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                            if (page == 0) SettingsPage(look.rtl) else BrowsePage(look.rtl)
                        }
                    }
                }
            }
        }
        val output = File(InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(null), "kit-gallery")
        output.mkdirs()
        for (next in looks) for (index in 0..1) {
            compose.runOnIdle { look = next; page = index }
            compose.waitForIdle()
            val minimum = with(compose.density) { 48.dp.toPx() } - 1f
            // Chips scrolled out of the filter row have no visible bounds and are skipped.
            compose.onAllNodes(hasClickAction()).fetchSemanticsNodes().filter { !it.boundsInRoot.isEmpty }.forEach { node ->
                val bounds = node.touchBoundsInRoot
                assertTrue("${next.name} page $index: ${node.config} touch target $bounds",
                    bounds.width >= minimum && bounds.height >= minimum)
            }
            val image = compose.onRoot().captureToImage().asAndroidBitmap()
            File(output, "${next.name}-$index.png").outputStream().use { image.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        // scripts/android/verify.sh collects /sdcard/Download/dmsg-* as the screenshot baseline.
        val shell = InstrumentationRegistry.getInstrumentation().uiAutomation
        for (command in listOf("mkdir -p /sdcard/Download/dmsg-kit-gallery", "cp -r ${output.absolutePath}/. /sdcard/Download/dmsg-kit-gallery/")) {
            android.os.ParcelFileDescriptor.AutoCloseInputStream(shell.executeShellCommand(command)).use { it.readBytes() }
        }
    }

    @Composable private fun SettingsPage(rtl: Boolean) {
        var toggled by remember { mutableStateOf(true) }
        var checked by remember { mutableStateOf(false) }
        var radio by remember { mutableIntStateOf(0) }
        Section(title = if (rtl) "الأمان" else "Security", footer = if (rtl) "يبقى المحتوى المشفّر على هذا الجهاز." else "Encrypted content stays on this device.") {
            ListRow(if (rtl) "الأجهزة" else "Devices", glyph = R.drawable.glyph_device, value = "3") {}
            ListRow(if (rtl) "الاسترداد" else "Recovery", glyph = R.drawable.glyph_recovery,
                supporting = if (rtl) "مفتاحا استرداد" else "Two recovery keys") {}
            ListRow(if (rtl) "كلمات المرور" else "Passwords", icon = Icons.Outlined.Key) {}
            CodeRow("SHA-256", "7ff80e0924f1a71ca4c98d7deb0ee606") {}
            SettingToggle(if (rtl) "المزامنة في الخلفية" else "Background sync", toggled, { toggled = it })
            SelectionRow(if (rtl) "نسخة مستقلة" else "Independent copy", checked) { checked = it }
            RadioChoiceRow(if (rtl) "الأحدث" else "Newest", radio == 0) { radio = 0 }
            RadioChoiceRow(if (rtl) "الاسم" else "Name", radio == 1) { radio = 1 }
        }
        Notice(if (rtl) "تمت المزامنة قبل ٣ دقائق." else "Synchronized 3 minutes ago.")
        Notice(if (rtl) "تعذّر الوصول إلى الخدمة." else "The service is unavailable.", warning = true) {
            TextButton(onClick = {}, colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onErrorContainer)) { Text(if (rtl) "إعادة المحاولة" else "Try again") }
        }
    }

    @Composable private fun BrowsePage(rtl: Boolean) {
        var scope by remember { mutableIntStateOf(0) }
        var favorites by remember { mutableStateOf(false) }
        var sort by remember { mutableStateOf("title") }
        BrowseScopes(trailing = { SortMenu(sort, listOf("title" to R.string.text_title_4c18fa6, "updated" to R.string.text_updated_9306ca8)) { sort = it } }) {
            BrowseChoice(if (rtl) "الكل" else "All", scope == 0) { scope = 0 }
            BrowseChoice(if (rtl) "المفضلة" else "Favorites", favorites, toggle = true) { favorites = !favorites }
            BrowseChoice(if (rtl) "كلمات المرور" else "Passwords", scope == 1) { scope = 1 }
            BrowseChoice(if (rtl) "الملاحظات" else "Notes", scope == 2) { scope = 2 }
            MenuChoice(if (rtl) "النوع" else "Type", scope == 3) { close -> DropdownMenuItem(text = { Text("SSH") }, onClick = { close(); scope = 3 }) }
        }
        listOf("login" to "Studio account", "note" to "Project notebook", "ssh" to "Build server").forEach { (kind, title) ->
            BrowseRow(title, Modifier, leading = { BrowseIcon(vaultGlyph(kind)) }) { Text("demo@example.test") }
        }
        SkeletonRows(count = 2)
        EmptyState(if (rtl) "لا توجد عناصر" else "No items yet", glyph = R.drawable.glyph_vault,
            detail = if (rtl) "أضف أول عنصر." else "Add the first item.")
    }
}
