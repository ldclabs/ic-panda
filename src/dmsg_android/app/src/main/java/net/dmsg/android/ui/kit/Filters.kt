package net.dmsg.android.ui.kit

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Sort
import androidx.compose.material.icons.outlined.ArrowDropDown
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.ui.card

/** Filter chips scroll sideways and fade at an edge that hides more; [trailing] holds the sort chip. */
@Composable internal fun BrowseScopes(
    trailing: @Composable () -> Unit = {}, content: @Composable RowScope.() -> Unit,
) {
    val scroll = rememberScrollState()
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val actionWidth = maxWidth * 0.45f
        Row(verticalAlignment = Alignment.CenterVertically) {
            Row(Modifier.weight(1f).fadingEdges(scroll).horizontalScroll(scroll),
                horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically, content = content)
            Box(Modifier.padding(start = 8.dp).widthIn(max = actionWidth)) { trailing() }
        }
    }
}

/**
 * A Material filter chip: a tick and the secondary container when selected. Exclusive scopes
 * behave as tabs; independent filters such as Favorites are check boxes.
 */
@Composable internal fun BrowseChoice(label: String, selected: Boolean, toggle: Boolean = false, onClick: () -> Unit) {
    ChoiceChip(label, selected, if (toggle) Modifier.toggleable(selected, role = Role.Checkbox) { onClick() }
        else Modifier.selectable(selected, role = Role.Tab, onClick = onClick))
}

/** A filter chip that opens a menu of exclusive choices; it is selected while one of them applies. */
@Composable internal fun MenuChoice(label: String, selected: Boolean, modifier: Modifier = Modifier, content: @Composable ColumnScope.(close: () -> Unit) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        ChoiceChip(label, selected, modifier.clickable(role = Role.DropdownList) { expanded = true }, trailing = Icons.Outlined.ArrowDropDown)
        DropdownMenu(expanded, { expanded = false }) { content { expanded = false } }
    }
}

@Composable private fun ChoiceChip(label: String, selected: Boolean, modifier: Modifier, trailing: ImageVector? = null) {
    val colors = MaterialTheme.colorScheme
    val shape = MaterialTheme.shapes.small
    // The chip is 32 dp; the reserved 48 dp extends its touch target.
    Box(Modifier.minimumInteractiveComponentSize(), contentAlignment = Alignment.Center) {
        Surface(Modifier.clip(shape).then(modifier), shape = shape, color = if (selected) colors.secondaryContainer else Color.Transparent,
            contentColor = if (selected) colors.onSecondaryContainer else colors.onSurfaceVariant,
            border = if (selected) null else BorderStroke(1.dp, colors.outlineVariant)) {
            Row(Modifier.heightIn(min = 32.dp).padding(start = if (selected) 8.dp else 16.dp, end = if (trailing == null) 16.dp else 8.dp, top = 4.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (selected) Icon(Icons.Outlined.Check, null, Modifier.size(18.dp))
                Text(label, style = MaterialTheme.typography.labelLarge, maxLines = 1)
                trailing?.let { Icon(it, null, Modifier.size(18.dp)) }
            }
        }
    }
}

/** The current sort order as an assist chip; the menu ticks the selected order. */
@Composable internal fun SortMenu(selected: String, choices: List<Pair<String, Int>>, choose: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        AssistChip(onClick = { expanded = true }, label = { Text(str(choices.first { it.first == selected }.second), maxLines = 1) },
            leadingIcon = { Icon(Icons.AutoMirrored.Outlined.Sort, str(R.string.sort_by), Modifier.size(AssistChipDefaults.IconSize)) },
            trailingIcon = { Icon(Icons.Outlined.ArrowDropDown, null, Modifier.size(AssistChipDefaults.IconSize)) })
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

/** Two exclusive choices as a segmented button. */
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

/** Fades the start or end edge while the row can scroll further that way, in either direction. */
private fun Modifier.fadingEdges(scroll: ScrollState): Modifier =
    graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }.drawWithContent {
        drawContent()
        val edge = minOf(24.dp.toPx(), size.width / 4)
        fun fade(left: Boolean) = drawRect(
            Brush.horizontalGradient(if (left) listOf(Color.Transparent, Color.Black) else listOf(Color.Black, Color.Transparent),
                startX = if (left) 0f else size.width - edge, endX = if (left) edge else size.width),
            topLeft = Offset(if (left) 0f else size.width - edge, 0f), size = Size(edge, size.height), blendMode = BlendMode.DstIn)
        val ltr = layoutDirection == LayoutDirection.Ltr
        if (scroll.value > 0) fade(left = ltr)
        if (scroll.value < scroll.maxValue) fade(left = !ltr)
    }
