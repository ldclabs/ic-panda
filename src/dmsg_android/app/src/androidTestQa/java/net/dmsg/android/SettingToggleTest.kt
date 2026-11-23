package net.dmsg.android

import net.dmsg.android.ui.kit.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import net.dmsg.android.ui.DMsgTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class SettingToggleTest {
    @get:Rule val compose = createComposeRule()

    @Test fun labelledSettingHasOneActionAndWorksWithKeyboard() {
        var checked by mutableStateOf(false)
        var changes = 0
        compose.setContent { DMsgTheme(false) {
            val inputMode = LocalInputModeManager.current
            SideEffect { inputMode.requestInputMode(InputMode.Keyboard) }
            Surface { SettingToggle("Background synchronization", checked, { checked = it; changes++ },
                description = "Only while this account is unlocked", modifier = Modifier.testTag("setting")) }
        } }
        compose.onAllNodes(hasClickAction()).assertCountEquals(1)
        compose.onNodeWithText("Background synchronization").assertHasClickAction()
        compose.onNodeWithTag("setting").assertIsOff().performClick().assertIsOn()
        compose.onNodeWithTag("setting").performSemanticsAction(SemanticsActions.RequestFocus) { it() }
        compose.onNodeWithTag("setting").performKeyInput { pressKey(Key.Spacebar) }
        compose.onNodeWithTag("setting").assertIsOff()
        compose.runOnIdle { assertEquals(2, changes) }
    }

    @Test fun disabledSettingExposesItsLabelAndCurrentState() {
        compose.setContent { DMsgTheme(false) { Surface {
            SettingToggle("Device-bound workspace", true, { error("Disabled control changed") }, enabled = false, modifier = Modifier.testTag("setting"))
        } } }
        compose.onNodeWithText("Device-bound workspace").assertIsNotEnabled().assertIsOn()
    }
}
