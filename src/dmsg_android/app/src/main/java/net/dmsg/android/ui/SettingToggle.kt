package net.dmsg.android.ui

import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.ListItem
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

/** One labelled, keyboard-operable accessibility target for the whole setting. */
@Composable internal fun SettingToggle(
    title: String,
    checked: Boolean,
    change: (Boolean) -> Unit,
    enabled: Boolean = true,
    description: String? = null,
    modifier: Modifier = Modifier,
) {
    val haptics = LocalHapticFeedback.current
    val supporting: (@Composable () -> Unit)? = description?.let { text -> { Text(text) } }
    ListItem(
        headlineContent = { Text(title) },
        supportingContent = supporting,
        trailingContent = { Switch(checked, onCheckedChange = null, enabled = enabled) },
        modifier = modifier.heightIn(min = 56.dp).semantics(mergeDescendants = true) {}
            .toggleable(checked, enabled = enabled, role = Role.Switch) { next ->
                change(next)
                haptics.performHapticFeedback(if (next) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff)
            },
    )
}
