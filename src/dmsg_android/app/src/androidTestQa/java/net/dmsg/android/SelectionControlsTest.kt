package net.dmsg.android

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import net.dmsg.android.ui.*
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class SelectionControlsTest {
    @get:Rule val compose = createComposeRule()

    @Test fun selectionLabelIsOneKeyboardTargetAtDoubleTextSize() {
        var chosen by mutableStateOf(false)
        var changes = 0
        lateinit var inputMode: androidx.compose.ui.input.InputModeManager
        compose.setContent {
            val density = LocalDensity.current
            val input = LocalInputModeManager.current
            SideEffect { inputMode = input; input.requestInputMode(InputMode.Keyboard) }
            CompositionLocalProvider(LocalDensity provides Density(density.density, 2f)) {
                DMsgTheme(false) { Surface {
                    SelectionRow("Independent file copy", chosen, subtitle = "Complete file identity") { chosen = it; changes++ }
                } }
            }
        }
        compose.onAllNodes(hasClickAction()).assertCountEquals(1)
        val row = compose.onNodeWithText("Independent file copy")
        row.assertHeightIsAtLeast(48.dp).assertIsOff().performClick().assertIsOn()
        compose.runOnIdle { inputMode.requestInputMode(InputMode.Keyboard) }
        row.performSemanticsAction(SemanticsActions.RequestFocus) { it() }
        row.assertIsFocused()
        row.performKeyInput { pressKey(Key.Spacebar) }
        row.assertIsOff()
        assertEquals(2, changes)
    }

    @Test fun radioChoicesExposeSelectionWithoutDuplicateActions() {
        var selected by mutableStateOf("a")
        compose.setContent { DMsgTheme(false) { Surface { Column {
            RadioChoiceRow("First address", selected == "a", subtitle = "aaaaa-bbbbb-ccccc-ddddd") { selected = "a" }
            RadioChoiceRow("Second address", selected == "b", enabled = false) { error("Disabled address changed") }
        } } } }
        compose.onAllNodes(hasClickAction()).assertCountEquals(2)
        compose.onNodeWithText("First address").assertIsSelected().assertHeightIsAtLeast(48.dp)
        compose.onNodeWithText("Second address").assertIsNotSelected().assertIsNotEnabled()
    }
}
