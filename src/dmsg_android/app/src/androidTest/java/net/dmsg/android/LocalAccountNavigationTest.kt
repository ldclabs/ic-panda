package net.dmsg.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import net.dmsg.android.core.ClientController
import net.dmsg.android.core.LOCAL_ACCOUNT
import net.dmsg.android.ui.DMsgApp
import net.dmsg.core.MobileEngine
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID

class LocalAccountNavigationTest {
    @get:Rule val compose = createComposeRule()

    @Test fun localWorkspaceSurvivesLockAndSigningInToAnotherAccount() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        val directory = File(net.dmsg.android.platform.PrivateFiles(app).workspace, "local-ui-" + UUID.randomUUID())
        val client = ClientController(app) { paths, platform ->
            MobileEngine.open(directory.path, paths.staging.path, OfflineAccountTest.FixtureHardware(platform))
        }
        try {
            compose.runOnUiThread { client.start() }
            compose.setContent { DMsgApp(client, {}, {}, { _, _ -> }) }
            compose.waitUntil(15_000) { client.state.value.session?.state == "logged_out" && !client.state.value.busy && !client.state.value.concealed }
            compose.onNodeWithText("Open an existing account").performScrollTo().performClick()
            compose.waitUntil(10_000) { client.joiningAccount.value && compose.onAllNodesWithText("Scan invitation").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Scan invitation").performScrollTo().assertIsDisplayed()
            compose.onNodeWithText("Review").assertIsNotEnabled()
            compose.onNodeWithText("Close").performClick()
            compose.waitUntil(5_000) { !client.joiningAccount.value }
            compose.onNodeWithTag("access.local").performClick()
            compose.waitUntil(15_000) { client.state.value.unlocked }
            compose.runOnIdle { client.navigation.open("account") }
            compose.onNodeWithText("Lock workspace").performClick()
            compose.waitUntil(15_000) { client.state.value.session?.state == "locked" && !client.state.value.busy }
            assertEquals(LOCAL_ACCOUNT, client.state.value.session?.account)
            compose.onNodeWithText("Unlock workspace").performClick()
            compose.waitUntil(15_000) { client.state.value.unlocked }
            compose.onNodeWithText("Switch account").performClick()
            compose.onNodeWithText("Sign in to another account").performClick()
            compose.waitUntil(15_000) { client.state.value.session?.state == "logged_out" && !client.state.value.busy }
            compose.onNodeWithText("Use a login already linked to the account you want to open.").assertExists()
            compose.onNodeWithText("Sign in with Internet Identity").assertExists()
            compose.onNodeWithText("Cancel").performClick()
            compose.onNodeWithText("Saved accounts").performScrollTo().performClick()
            compose.onNodeWithText("Local account").performClick()
            compose.waitUntil(15_000) { client.state.value.session?.account == LOCAL_ACCOUNT && !client.state.value.busy }
            compose.onNodeWithText("Unlock workspace").performClick()
            compose.waitUntil(15_000) { client.state.value.unlocked }
        } finally {
            runBlocking { withContext(Dispatchers.Main.immediate) { client.shutdown() } }
            directory.deleteRecursively()
        }
    }
}
