package net.dmsg.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import org.junit.Rule
import org.junit.Test

class VaultDraftRecreationTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @Test fun activityRecreationKeepsTheCurrentEditorAndInput() {
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        compose.runOnIdle { compose.activity.qaClient.vaultDrafts.open(null) }
        compose.waitUntil(5_000) { compose.activity.qaClient.vaultDrafts.presented.value }
        compose.onNodeWithTag("vault-editor-title").performTextInput("Kept across recreation")
        compose.activityRule.scenario.recreate()
        compose.onNodeWithTag("vault-editor-title").assertTextContains("Kept across recreation")
    }
}
