package net.dmsg.android

import android.os.Debug
import android.os.SystemClock
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import net.dmsg.android.core.ReadState
import net.dmsg.android.platform.str
import net.dmsg.core.FieldEdit
import net.dmsg.core.VaultEdit
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

/** Real sealed native records and Compose pagination. Opt-in because seeding
 * 1000 durable records is intentionally outside the fast UI regression. */
class VaultScaleProbeTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @Test fun thousandItemsRemainSearchableAndAllFavoritesAreReachable() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("vaultScaleProbe") == "true")
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val client = compose.activity.qaClient
        val core = checkNotNull(client.scopedHandle())
        val report = JSONObject().put("items_added", 1000).put("favorites_added", 125)
            .put("api", android.os.Build.VERSION.SDK_INT).put("package", app.packageName)
            .put("variant", BuildConfig.BUILD_TYPE).put("page_size", shell("getconf PAGE_SIZE").trim())
            .put("scope", "Real offline Rust encrypted storage and Compose; QA build, not release or network timing")
        val started = SystemClock.elapsedRealtime()
        try {
            runBlocking(Dispatchers.IO) {
                repeat(1000) { index ->
                    core.saveItem(null, null, VaultEdit("Scale " + index.toString().padStart(4, '0'), "login", "Synthetic scale sample", index % 8 == 0,
                        listOf("scale-probe"), listOf(FieldEdit("password", "concealed", "", "", "Scale-only secret")), emptyList()))
                }
                val summaries = core.vaultPage("Scale ", null, false, 0u, 100u)
                assertEquals(100, summaries.items.size)
                assertTrue(summaries.items.all { it.fields.isEmpty() && it.notes.isEmpty() })
            }
            report.put("seed_ms", SystemClock.elapsedRealtime() - started)
            compose.runOnIdle { client.activity(); client.navigation.open("vault") }
            compose.onNode(hasSetTextAction()).performTextReplacement("Scale ")
            compose.waitUntil(15_000) { client.vault.listing.state.value is ReadState.Ready && client.vault.listing.value()?.total == 1000uL }
            val readStart = SystemClock.elapsedRealtime()
            var loaded = 100
            while (client.vault.listing.value()!!.hasMore) {
                compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText(str(R.string.text_load_more_4e1d9f8)))
                compose.onNodeWithText(str(R.string.text_load_more_4e1d9f8)).performClick()
                loaded += 100
                compose.waitUntil(10_000) { client.vault.listing.state.value is ReadState.Ready && client.vault.listing.value()?.items?.size == loaded }
            }
            assertEquals(1000, client.vault.listing.value()!!.items.map { it.id }.toSet().size)
            report.put("ui_pagination_ms", SystemClock.elapsedRealtime() - readStart)
            compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Scale 0999"))
            compose.onNodeWithText("Scale 0999").assertIsDisplayed()
            compose.onNodeWithText("Scale-only secret").assertDoesNotExist()
            shell("dumpsys gfxinfo ${app.packageName} reset")
            val trace = "/data/misc/perfetto-traces/dmsg-vault-scale.pftrace"
            val pid = shell("perfetto --background-wait -o $trace -t 30s --app ${app.packageName} sched freq idle am wm gfx view binder_driver dalvik")
                .trim().lineSequence().lastOrNull()?.toIntOrNull()
            val scrollStart = SystemClock.elapsedRealtime()
            repeat(16) { index ->
                compose.onNode(hasScrollToIndexAction()).performTouchInput { if (index % 2 == 0) swipeDown(durationMillis = 300) else swipeUp(durationMillis = 300) }
            }
            report.put("scroll_flow_ms", SystemClock.elapsedRealtime() - scrollStart)
            save("gfxinfo.txt", shell("dumpsys gfxinfo ${app.packageName} framestats"))
            save("meminfo.txt", shell("dumpsys meminfo ${app.packageName}"))
            if (pid != null) {
                shell("kill -TERM $pid")
                var previousSize = ""
                for (attempt in 0 until 20) {
                    val size = shell("stat -c %s $trace").trim()
                    if (size.toLongOrNull()?.let { it > 0 } == true && size == previousSize) break
                    previousSize = size
                    Thread.sleep(100)
                }
                shell("mkdir -p /sdcard/Download/dmsg-vault-scale")
                shell("cp $trace /sdcard/Download/dmsg-vault-scale/scroll.pftrace")
            }
            report.put("perfetto_started", pid != null)
            compose.onNode(hasText(str(R.string.text_favorites_960eb5d), substring = true)).performClick()
            compose.waitUntil(10_000) { client.vault.listing.state.value is ReadState.Ready && client.vault.listing.value()?.total == 125uL }
            compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText(str(R.string.text_load_more_4e1d9f8)))
            compose.onNodeWithText(str(R.string.text_load_more_4e1d9f8)).performClick()
            compose.waitUntil(10_000) { client.vault.listing.value()?.items?.size == 125 }
            assertTrue(client.vault.listing.value()!!.items.all { it.favorite })
            compose.onNode(hasText(str(R.string.text_favorites_960eb5d), substring = true)).performClick()
            compose.onNode(hasSetTextAction()).performTextReplacement("Scale 0999")
            compose.waitUntil(10_000) { client.vault.listing.state.value is ReadState.Ready && client.vault.listing.value()?.total == 1uL }
            compose.onNode(hasText("Scale 0999") and hasClickAction() and !hasSetTextAction()).performClick()
            compose.waitUntil(10_000) { client.vault.detail.value()?.title == "Scale 0999" }
            compose.onNodeWithText("Scale-only secret").assertDoesNotExist()
            val lockStart = SystemClock.elapsedRealtime()
            compose.runOnIdle { client.lock() }
            compose.waitUntil(10_000) { !client.state.value.unlocked }
            compose.onNodeWithText("Scale 0999").assertDoesNotExist()
            report.put("lock_flow_ms", SystemClock.elapsedRealtime() - lockStart)
            report.put("outcome", "passed")
        } finally {
            core.destroy()
            report.put("elapsed_ms", SystemClock.elapsedRealtime() - started)
            report.put("final_pss_kib", Debug.MemoryInfo().also(Debug::getMemoryInfo).totalPss)
            save("report.json", report.toString(2))
        }
    }

    private fun shell(command: String) = android.os.ParcelFileDescriptor.AutoCloseInputStream(
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
    ).use { it.readBytes().toString(Charsets.UTF_8) }

    private fun save(name: String, value: String) {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File(app.getExternalFilesDir(null), "scale-$name").apply { writeText(value) }
        shell("mkdir -p /sdcard/Download/dmsg-vault-scale")
        shell("cp ${file.absolutePath} /sdcard/Download/dmsg-vault-scale/$name")
    }
}
