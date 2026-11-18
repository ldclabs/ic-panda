package net.dmsg.android

import androidx.activity.compose.setContent
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import net.dmsg.android.ui.DMsgTheme
import net.dmsg.android.ui.EntryScreen
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class WelcomeIntentTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @Test fun newAndReturningUsersChooseAnIntentBeforeOpeningACredentialProvider() {
        val client = compose.activity.qaClient
        compose.waitUntil(15_000) { client.state.value.unlocked }
        val initial = client.state.value
        val session = initial.session!!.copy(state = "logged_out", account = null, fastUnlock = false, passkeyAvailable = true, setupPending = false)
        val sample = mutableStateOf(initial.copy(session = session))
        val requestsBefore = client.state.value.session!!.approvalsWaiting
        compose.runOnUiThread {
            compose.activity.setContent { DMsgTheme(false) { EntryScreen(client, sample.value, {}, remember { SnackbarHostState() }) } }
        }
        compose.onNodeWithText("Open an existing account").performClick()
        compose.onNodeWithText("Use an existing passkey").assertIsDisplayed()
        compose.onNodeWithText("Create passkey").assertDoesNotExist()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Set up a new workspace").performClick()
        compose.onNodeWithText("Create passkey").assertIsDisplayed()
        compose.onNodeWithText("Already have a passkey? Sign in").performClick()
        compose.onNodeWithText("Use an existing passkey").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.runOnIdle { sample.value = initial.copy(session = session.copy(passkeyAvailable = false)) }
        compose.onNodeWithText("Sign in with Internet Identity").assertIsDisplayed()
        compose.onNodeWithText("Create passkey").assertDoesNotExist()
        compose.runOnIdle { assertEquals(requestsBefore, client.state.value.session!!.approvalsWaiting) }
    }
}
