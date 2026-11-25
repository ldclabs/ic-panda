package net.dmsg.android

import androidx.activity.compose.setContent
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import net.dmsg.android.core.AccessPage
import net.dmsg.android.ui.DMsgTheme
import net.dmsg.android.ui.EntryScreen
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class WelcomeIntentTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @Test fun systemBackDuringSignInOnlyWaitsOnTheAlternateLoginPage() {
        val client = compose.activity.qaClient
        compose.waitUntil(15_000) { client.state.value.unlocked }
        compose.runOnUiThread { client.lock() }
        compose.waitUntil(15_000) { client.state.value.session?.state == "locked" && !client.state.value.busy }
        val locked = client.state.value
        val busy = mutableStateOf(true)
        compose.runOnUiThread {
            client.showAccess(AccessPage.SIGN_IN_ANOTHER)
            compose.activity.setContent {
                DMsgTheme(false) { EntryScreen(client, locked.copy(busy = busy.value), {}, {}, remember { SnackbarHostState() }) }
            }
        }
        fun pressBack() = compose.runOnIdle { compose.activity.onBackPressedDispatcher.onBackPressed() }
        pressBack()
        assertEquals(AccessPage.SIGN_IN_ANOTHER, client.access.value)
        compose.runOnIdle { busy.value = false }
        pressBack()
        assertEquals(AccessPage.WELCOME, client.access.value)

        compose.runOnIdle { busy.value = true; client.showAccess(AccessPage.JOIN_ACCOUNT) }
        pressBack()
        assertEquals(AccessPage.WELCOME, client.access.value)
    }

    @Test fun newAndReturningUsersChooseAnIntentBeforeOpeningACredentialProvider() {
        val client = compose.activity.qaClient
        compose.waitUntil(15_000) { client.state.value.unlocked }
        val initial = client.state.value
        val session = initial.session!!.copy(state = "logged_out", account = null, fastUnlock = false, passkeyAvailable = true, setupPending = false)
        val sample = mutableStateOf(initial.copy(session = session))
        val requestsBefore = client.state.value.session!!.approvalsWaiting
        compose.runOnUiThread {
            compose.activity.setContent { DMsgTheme(false) { EntryScreen(client, sample.value, {}, {}, remember { SnackbarHostState() }) } }
        }
        compose.onNodeWithTag("open-demo-access").performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag("open-demo-access").performClick()
        compose.onNodeWithTag("demo-code-submit").assertIsNotEnabled()
        compose.onNodeWithTag("demo-code-input").performTextInput("SAMPLE-CODE")
        compose.onNodeWithTag("demo-code-submit").assertIsEnabled()
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle { sample.value = initial.copy(session = session.copy(state = "no_account", setupPending = true)) }
        compose.onNodeWithTag("open-demo-access").assertDoesNotExist()
        compose.runOnIdle { sample.value = initial.copy(session = session) }
        compose.onNodeWithText("Open an existing account").performScrollTo().performClick()
        compose.runOnIdle { assertEquals(true, client.joiningAccount.value); client.showAccountInvitation(false) }
        compose.onNodeWithText("Use an existing passkey").performScrollTo().performClick()
        compose.onNodeWithText("Use an existing passkey").assertIsDisplayed()
        compose.onNodeWithText("Create passkey").assertDoesNotExist()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Create a separate account").performClick()
        compose.onNodeWithText("Create passkey").assertIsDisplayed()
        compose.onNodeWithText("Already have a passkey? Sign in").performClick()
        compose.onNodeWithText("Use an existing passkey").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.runOnIdle { sample.value = initial.copy(session = session.copy(passkeyAvailable = false)) }
        compose.onNodeWithText("Sign in with Internet Identity").assertIsDisplayed()
        compose.onNodeWithText("Create passkey").assertDoesNotExist()
        compose.runOnIdle {
            sample.value = initial.copy(session = session.copy(state = "locked", account = initial.session?.account, fastUnlock = true))
        }
        compose.onNodeWithText("Unlock workspace").assertIsDisplayed()
        compose.onNodeWithText("Create a separate account").performScrollTo().performClick()
        compose.onNodeWithText("Create passkey").assertIsDisplayed()
        compose.runOnIdle { assertEquals(requestsBefore, client.state.value.session!!.approvalsWaiting) }
    }
}
