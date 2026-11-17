package net.dmsg.android.ui

import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ListItem
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

/** The label, value and control are one focusable, keyboard-operable target. */
@Composable internal fun SelectionRow(title: String, checked: Boolean, enabled: Boolean = true, subtitle: String? = null, change: (Boolean) -> Unit) {
    val haptics = LocalHapticFeedback.current
    ListItem(
        modifier = Modifier.heightIn(min = 56.dp).semantics(mergeDescendants = true) {}
            .toggleable(checked, enabled = enabled, role = Role.Checkbox) {
                change(it)
                haptics.performHapticFeedback(if (it) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff)
            },
        headlineContent = { Text(title) }, supportingContent = subtitle?.let { { Text(it) } },
        trailingContent = { Checkbox(checked, null, enabled = enabled) },
    )
}

@Composable internal fun RadioChoiceRow(title: String, selected: Boolean, enabled: Boolean = true, subtitle: String? = null, choose: () -> Unit) {
    val haptics = LocalHapticFeedback.current
    ListItem(
        modifier = Modifier.heightIn(min = 56.dp).semantics(mergeDescendants = true) {}
            .selectable(selected, enabled = enabled, role = Role.RadioButton) {
                choose()
                haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
            },
        headlineContent = { Text(title) }, supportingContent = subtitle?.let { { Text(it) } },
        leadingContent = { RadioButton(selected, onClick = null, enabled = enabled) },
    )
}
