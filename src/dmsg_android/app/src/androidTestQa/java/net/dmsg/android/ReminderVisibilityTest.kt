package net.dmsg.android

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import net.dmsg.android.core.*
import net.dmsg.android.ui.DMsgTheme
import net.dmsg.android.ui.reminderVisible
import net.dmsg.core.MobileEngine
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class ReminderVisibilityTest {
    @get:Rule val compose = createComposeRule()
    @Test fun onlyTheActuallyVisibleWatermarkIsObservedWithoutOpeningANotification() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        lateinit var native: MobileEngine
        lateinit var directory: File
        lateinit var client: ClientController
        compose.runOnUiThread {
            client = ClientController(app) { paths, platform ->
                directory = File(paths.workspace, "ui-fixtures/" + UUID.randomUUID())
                MobileEngine.openUiFixture(directory.path, paths.staging.path, platform).also { native = it }
            }
            client.start()
        }
        var through by mutableStateOf(0uL)
        var covered by mutableStateOf(true)
        val target = NoticeTarget("channel", "ab".repeat(32))
        try {
            compose.waitUntil(15_000) { client.state.value.unlocked }
            runBlocking { native.seedUiReminders() }
            val account = client.state.value.session!!.account!!
            fun observed() = runBlocking { parseReminder(native.notificationReference(account, target.json()).decode()).observed }
            compose.setContent {
                DMsgTheme(false) {
                    Box(Modifier.fillMaxSize()) { Text("Current conversation watermark " + through, Modifier.reminderVisible(client, target, through)) }
                    if (covered) AlertDialog(onDismissRequest = { covered = false }, title = { Text("Reviewing another task") }, confirmButton = { TextButton(onClick = { covered = false }) { Text("Return to conversation") } })
                }
            }
            compose.waitForIdle()
            compose.runOnIdle { through = 1u }
            compose.waitForIdle()
            Thread.sleep(350)
            assertEquals(0uL, observed())
            assertNull(client.notifications.opened.value)
            compose.onNodeWithText("Return to conversation").performClick()
            compose.waitUntil(10_000) { observed() == 1uL }
            compose.runOnIdle { client.lock(false) }
            compose.waitUntil(10_000) { !client.state.value.unlocked }
            assertTrue(client.notifications.visible().isEmpty())
        } finally {
            runBlocking { withContext(Dispatchers.Main.immediate) { client.shutdown() } }
            directory.deleteRecursively()
        }
    }
}
