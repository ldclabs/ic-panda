package net.dmsg.android

import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import net.dmsg.android.platform.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PasswordChoicesTest {
    @get:Rule val compose = createComposeRule()

    @Test fun authenticatedPickerRanksBothAccountsAndFindsOtherLogins() {
        val entries = listOf("personal", "work", "other").map { ExportCredential(it, "Example", it, "", it) }
        var chosen: String? = null
        compose.setContent {
            PasswordPicker(Modifier, entries, listOf("work", "personal"), null, false, { chosen = it.username }) {}
        }
        val work = compose.onNodeWithText("work")
        assertTrue(work.fetchSemanticsNode().boundsInRoot.top < compose.onNodeWithText("personal").fetchSemanticsNode().boundsInRoot.top)
        work.performClick()
        assertEquals("work", chosen)
        compose.onNode(hasSetTextAction()).performTextInput("OTHER")
        compose.onNodeWithText("work").assertDoesNotExist()
        compose.onNodeWithText("other").performClick()
        assertEquals("other", chosen)
    }

    @Test fun confirmationOffersRememberOnlyForIdentifiedTargets() {
        var identifiable by mutableStateOf(false)
        var remember: Boolean? = null
        compose.setContent {
            ConfirmLogin(ExportCredential("id", "Example", "me", "", "item"), "app.example", null, identifiable, {}) { remember = it }
        }
        compose.onNodeWithText("Fill and remember").assertDoesNotExist()
        compose.onNodeWithText("Fill once").performClick()
        assertEquals(false, remember)
        compose.runOnIdle { identifiable = true }
        compose.onNodeWithText("Fill and remember").performClick()
        assertEquals(true, remember)
    }
}
