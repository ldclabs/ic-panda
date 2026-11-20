package net.dmsg.android

import android.graphics.Bitmap
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.platform.*
import net.dmsg.android.ui.DMsgTheme
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

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

    @Test fun tappingAFilteredAccountAndConfirmingOnceReturnsOnlyThatAccount() {
        val entries = listOf("work", "personal").map { ExportCredential(it, "Example", it, "https://example.com", it) }
        val filled = mutableListOf<String>()
        compose.setContent {
            var selected by remember { mutableStateOf<ExportCredential?>(null) }
            DMsgTheme(null) {
                Scaffold { padding ->
                    PasswordPicker(Modifier.fillMaxSize().padding(padding), entries, emptyList(), "example.com", filled.isNotEmpty(), { selected = it }) {
                        PasswordPickerHeading("Example App", "example.com") {}
                    }
                }
                selected?.let { entry ->
                    ConfirmLogin(entry, "Example App", "example.com", true, { selected = null }) {
                        selected = null
                        filled += entry.username
                    }
                }
            }
        }
        compose.onNodeWithText("personal · example.com").assertIsDisplayed()
        captureFixture("picker")
        compose.onNode(hasSetTextAction()).performTextInput("work")
        compose.onNodeWithText("work · example.com").performTouchInput { click() }
        captureFixture("confirmation")
        val once = str(R.string.autofill_once)
        compose.onNodeWithText(once).performTouchInput { click() }
        compose.onNodeWithText(once).assertDoesNotExist()
        compose.runOnIdle { assertEquals(listOf("work"), filled) }
    }

    private fun captureFixture(name: String) {
        if (InstrumentationRegistry.getArguments().getString("captureAutofillUi") != "true") return
        compose.waitForIdle()
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.uiAutomation.waitForIdle(500, 5_000)
        val image = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
        try {
            File(instrumentation.targetContext.cacheDir, "autofill-$name.png").outputStream().use {
                assertTrue(image.compress(Bitmap.CompressFormat.PNG, 100, it))
            }
        } finally { image.recycle() }
    }
}
