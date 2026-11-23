package net.dmsg.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Sort
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.KeyboardArrowDown
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.platform.str

/** Adjacent native list rows share an outer shape and keep their own ripple and focus target. */
private val LocalGroupIconColor = staticCompositionLocalOf { Color.Unspecified }

@Composable internal fun SettingsGroup(
    iconColor: Color = MaterialTheme.colorScheme.primaryContainer,
    content: @Composable ColumnScope.() -> Unit,
) {
    CompositionLocalProvider(LocalGroupIconColor provides iconColor) {
        OnCard {
            Column(Modifier.fillMaxWidth().clip(MaterialTheme.shapes.large),
                verticalArrangement = Arrangement.spacedBy(2.dp), content = content)
        }
    }
}

@Composable internal fun TonalIcon(icon: ImageVector, color: Color = LocalGroupIconColor.current) {
    val background = if (color == Color.Unspecified) MaterialTheme.colorScheme.primaryContainer else color
    Box(Modifier.size(40.dp).background(background, CircleShape), contentAlignment = Alignment.Center) {
        Icon(icon, null, Modifier.size(22.dp), tint = contentColorFor(background))
    }
}

@Composable internal fun DetailCard(
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.card,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, color = color) {
        Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(6.dp), content = content)
    }
}

@Composable internal fun WorkspaceScope(first: String, second: String, secondSelected: Boolean, enabled: Boolean = true, switch: () -> Unit) {
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().height(IntrinsicSize.Min)) {
        listOf(first, second).forEachIndexed { index, label ->
            val selected = (index == 1) == secondSelected
            SegmentedButton(
                selected = selected, enabled = enabled, onClick = { if (!selected) switch() },
                modifier = Modifier.fillMaxHeight(),
                shape = SegmentedButtonDefaults.itemShape(index, 2),
                colors = SegmentedButtonDefaults.colors(
                    activeContainerColor = MaterialTheme.colorScheme.primaryContainer,
                    activeContentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                    inactiveContainerColor = MaterialTheme.colorScheme.card,
                    inactiveBorderColor = MaterialTheme.colorScheme.outlineVariant,
                    activeBorderColor = MaterialTheme.colorScheme.outlineVariant,
                ),
            ) { Text(label, textAlign = TextAlign.Center) }
        }
    }
}

@Composable internal fun SortMenu(selected: String, choices: List<Pair<String, Int>>, choose: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        TextButton(onClick = { expanded = true }) {
            Icon(Icons.AutoMirrored.Outlined.Sort, str(R.string.sort_by), Modifier.size(18.dp))
            Spacer(Modifier.width(6.dp))
            Text(str(choices.first { it.first == selected }.second))
            Icon(Icons.Outlined.KeyboardArrowDown, null, Modifier.size(16.dp))
        }
        DropdownMenu(expanded, { expanded = false }) {
            choices.forEach { (key, label) ->
                DropdownMenuItem(text = { Text(str(label)) },
                    trailingIcon = if (selected == key) ({ Icon(Icons.Outlined.Check, null) }) else null,
                    modifier = Modifier.semantics { this.selected = key == selected },
                    onClick = { expanded = false; choose(key) })
            }
        }
    }
}

@Composable internal fun EmptyPanel(
    text: String,
    icon: ImageVector? = null,
    detail: String? = null,
    action: (@Composable () -> Unit)? = null,
) {
    Surface(Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.card) {
        Column(Modifier.padding(horizontal = 20.dp, vertical = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            icon?.let { TonalIcon(it) }
            Text(text, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
            detail?.let { Text(it, style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center) }
            action?.invoke()
        }
    }
}
