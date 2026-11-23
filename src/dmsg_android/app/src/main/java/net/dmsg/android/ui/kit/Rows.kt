package net.dmsg.android.ui.kit

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/** A row's leading icon: a tonal circle, coloured per section like Android settings. */
@Composable internal fun RowIcon(painter: Painter, color: Color = LocalSectionIconColor.current) {
    val background = if (color == Color.Unspecified) MaterialTheme.colorScheme.primaryContainer else color
    Box(Modifier.size(40.dp).background(background, CircleShape), contentAlignment = Alignment.Center) {
        Icon(painter, null, Modifier.size(22.dp), tint = contentColorFor(background))
    }
}

@Composable internal fun RowIcon(icon: ImageVector, color: Color = LocalSectionIconColor.current) = RowIcon(rememberVectorPainter(icon), color)

/**
 * A row that opens a page. There is no chevron: the trailing side shows the current [value]
 * or a [trailing] badge.
 */
@Composable internal fun ListRow(
    headline: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    value: String? = null,
    supporting: String? = null,
    @DrawableRes glyph: Int? = null,
    trailing: (@Composable () -> Unit)? = null,
    onClick: (() -> Unit)? = null,
) {
    val leading: (@Composable () -> Unit)? = when {
        glyph != null -> { { RowIcon(painterResource(glyph)) } }
        icon != null -> { { RowIcon(icon) } }
        else -> null
    }
    val end: (@Composable () -> Unit)? = if (value == null && trailing == null) null else { {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            value?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
            trailing?.invoke()
        }
    } }
    ListItem(
        headlineContent = { Text(headline) },
        supportingContent = supporting?.let { { Text(it) } },
        leadingContent = leading, trailingContent = end,
        modifier = modifier.clipTo(rowShape())
            .then(if (onClick == null) Modifier else Modifier.clickable(role = Role.Button, onClick = onClick))
            .heightIn(min = 56.dp),
    )
}

/** A label and its value; a long value wraps under the label. */
@Composable internal fun LabelValue(label: String, value: String) {
    FlowRow(Modifier.fillMaxWidth().padding(vertical = 4.dp).semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.SpaceBetween, verticalArrangement = Arrangement.spacedBy(2.dp),
        itemVerticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.padding(end = 12.dp), style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, style = MaterialTheme.typography.titleSmall)
    }
}

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
    ListItem(
        headlineContent = { Text(title) },
        supportingContent = description?.let { { Text(it) } },
        trailingContent = { Switch(checked, onCheckedChange = null, enabled = enabled) },
        modifier = modifier.clipTo(rowShape()).heightIn(min = 56.dp).semantics(mergeDescendants = true) {}
            .toggleable(checked, enabled = enabled, role = Role.Switch) { next ->
                change(next)
                haptics.performHapticFeedback(if (next) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff)
            },
    )
}

/** The label, value and control are one focusable, keyboard-operable target. */
@Composable internal fun SelectionRow(title: String, checked: Boolean, enabled: Boolean = true, subtitle: String? = null, modifier: Modifier = Modifier, accessibilityLabel: String? = null, change: (Boolean) -> Unit) {
    val haptics = LocalHapticFeedback.current
    ListItem(
        modifier = modifier.clipTo(rowShape()).heightIn(min = 56.dp)
            .semantics(mergeDescendants = true) { accessibilityLabel?.let { contentDescription = it } }
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
        modifier = Modifier.clipTo(rowShape()).heightIn(min = 56.dp).semantics(mergeDescendants = true) {}
            .selectable(selected, enabled = enabled, role = Role.RadioButton) {
                choose()
                haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
            },
        headlineContent = { Text(title) }, supportingContent = subtitle?.let { { Text(it) } },
        leadingContent = { RadioButton(selected, onClick = null, enabled = enabled) },
    )
}

/** The leading tile of a browse row: a type glyph on a tonal square. */
@Composable internal fun BrowseIcon(painter: Painter) {
    val colors = MaterialTheme.colorScheme
    Box(Modifier.size(36.dp).background(colors.secondaryContainer, MaterialTheme.shapes.small), contentAlignment = Alignment.Center) {
        Icon(painter, null, Modifier.size(20.dp), tint = colors.onSecondaryContainer)
    }
}

@Composable internal fun BrowseIcon(icon: ImageVector) = BrowseIcon(rememberVectorPainter(icon))

@Composable internal fun BrowseIcon(@DrawableRes glyph: Int) = BrowseIcon(painterResource(glyph))

/** A compact row of a browse list: no chevron and no divider, as in Google's list apps. */
@Composable internal fun BrowseRow(
    title: String, modifier: Modifier, leading: @Composable () -> Unit,
    trailing: @Composable RowScope.() -> Unit = {}, supporting: @Composable () -> Unit = {},
) {
    val largeText = LocalDensity.current.fontScale >= 1.5f
    // Clip before the caller's click handling so the pressed state keeps rounded corners.
    Row(Modifier.clip(MaterialTheme.shapes.small).then(modifier).fillMaxWidth().heightIn(min = 56.dp).padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { leading() }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium,
                maxLines = if (largeText) Int.MAX_VALUE else 1, overflow = TextOverflow.Ellipsis)
            CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.onSurfaceVariant) {
                ProvideTextStyle(MaterialTheme.typography.bodySmall, supporting)
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp), content = trailing)
    }
}
