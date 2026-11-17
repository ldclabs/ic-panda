package net.dmsg.android

import android.content.Context
import android.content.res.Configuration
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.viewinterop.AndroidView
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.platform.WorkspaceEntries
import net.dmsg.android.core.WorkspaceLink
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

@RunWith(AndroidJUnit4::class)
class WorkspaceEntryTest {
    @get:Rule val compose = createComposeRule()

    @Test fun platformWidgetsUsePublicLabelsInBothThemesWithoutStartingAClient() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext.applicationContext as DMsgApplication
        val wasStarted = app.hasMainClient
        fun themed(dark: Boolean): Context = app.createConfigurationContext(Configuration(app.resources.configuration).apply {
            uiMode = (uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or if (dark) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO
        })
        val views = mutableListOf<View>()
        compose.setContent {
            AndroidView(factory = { context ->
                LinearLayout(context).apply {
                    orientation = LinearLayout.VERTICAL
                    setPadding(24, 24, 24, 24)
                    for (dark in listOf(false, true)) {
                        val host = themed(dark)
                        val density = host.resources.displayMetrics.density
                        for (wide in listOf(false, true)) {
                            val widget = WorkspaceEntries.widget(host, wide).apply(host, this)
                            addView(widget, LinearLayout.LayoutParams(((if (wide) 320 else 160) * density).toInt(), ((if (wide) 180 else 140) * density).toInt()).apply { bottomMargin = 16 })
                            views += widget
                            if (wide) {
                                val vault = widget.findViewById<TextView>(R.id.widget_vault)
                                assertEquals("Vault", vault.text.toString())
                                assertEquals(host.getColor(R.color.widget_forest), vault.currentTextColor)
                                assertTrue(vault.isClickable)
                            } else assertTrue(widget.findViewById<View>(R.id.widget_open).isClickable)
                        }
                    }
                }
            })
        }
        compose.waitForIdle()
        compose.runOnIdle { assertTrue(views.all { it.isShown && it.width > 0 && it.height > 0 }) }
        instrumentation.uiAutomation.waitForIdle(250, 5000)
        Thread.sleep(250) // RemoteViews draw after Compose's own idling resource.
        fun texts(view: View): List<String> = if (view is TextView) listOf(view.text.toString()) else if (view is ViewGroup) (0 until view.childCount).flatMap { texts(view.getChildAt(it)) } else emptyList()
        val labels = views.flatMap(::texts)
        assertTrue(labels.containsAll(listOf("Vault", "Messages", "Files", "Approvals", "Shared items")))
        assertTrue(labels.all { it in setOf("dMsg", "Open private workspace", "Vault", "Messages", "Files", "Approvals", "Shared items") })
        assertEquals(wasStarted, app.hasMainClient)
        val image = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
        val file = File(app.getExternalFilesDir(null), "qa-widgets-light-dark.png")
        file.outputStream().use { image.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        image.recycle()
        android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand("mkdir -p /sdcard/Download/dmsg-qa")).use { it.readBytes() }
        android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand("cp " + file.absolutePath + " /sdcard/Download/dmsg-qa/qa-widgets-light-dark.png")).use { it.readBytes() }
    }

    @Test fun shortcutIntentsHaveExactAppTargetsAndNoActionOrContentExtras() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        WorkspaceEntries.labels.keys.forEach { target ->
            val intent = WorkspaceEntries.intent(context, target)
            assertEquals(context.packageName, intent.component!!.packageName)
            assertEquals(MainActivity::class.java.name, intent.component!!.className)
            assertEquals(WorkspaceLink(target), WorkspaceLink.parse(intent.dataString!!))
            assertNull(intent.clipData)
            assertTrue(intent.extras == null || intent.extras!!.isEmpty)
            assertEquals(0, intent.flags and (android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION or android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION))
        }
    }
}
