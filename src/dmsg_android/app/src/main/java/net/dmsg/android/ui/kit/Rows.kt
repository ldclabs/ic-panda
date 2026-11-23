package net.dmsg.android.ui.kit

import android.os.SystemClock
import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
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
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.ui.code

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
 * or a [trailing] badge. A [destructive] row, such as signing out, takes the error colour.
 */
@Composable internal fun ListRow(
    headline: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    value: String? = null,
    supporting: String? = null,
    @DrawableRes glyph: Int? = null,
    trailing: (@Composable () -> Unit)? = null,
    destructive: Boolean = false,
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
) {
    val sectionIconColor = LocalSectionIconColor.current
    val iconColor = when {
        sectionIconColor != Color.Unspecified -> sectionIconColor
        destructive -> MaterialTheme.colorScheme.errorContainer
        else -> Color.Unspecified
    }
    val leading: (@Composable () -> Unit)? = when {
        glyph != null -> { { RowIcon(painterResource(glyph), iconColor) } }
        icon != null -> { { RowIcon(icon, iconColor) } }
        else -> null
    }
    val end: (@Composable () -> Unit)? = if (value == null && trailing == null) null else { {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            value?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
            trailing?.invoke()
        }
    } }
    ListItem(
        headlineContent = { Text(headline, color = if (destructive) MaterialTheme.colorScheme.error else Color.Unspecified) },
        supportingContent = supporting?.let { { Text(it) } },
        leadingContent = leading, trailingContent = end,
        modifier = modifier.clipTo(rowShape())
            .then(if (onClick == null) Modifier else Modifier.clickable(enabled = enabled, role = Role.Button, onClick = onClick))
            .then(if (enabled) Modifier else Modifier.alpha(0.45f))
            .heightIn(min = 56.dp),
    )
}

/** A label and its value; a long value wraps under the label. Set [code] for technical identifiers. */
@Composable internal fun LabelValue(label: String, value: String, code: Boolean = false) {
    FlowRow(Modifier.fillMaxWidth().padding(vertical = 4.dp).semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.SpaceBetween, verticalArrangement = Arrangement.spacedBy(2.dp),
        itemVerticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.padding(end = 12.dp), style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, style = if (code) MaterialTheme.typography.bodyMedium.code() else MaterialTheme.typography.titleSmall)
    }
}

/** A labelled value inside a [Section] or [DetailCard], with optional monospace formatting and one-tap copy. */
@Composable internal fun InfoRow(
    label: String,
    value: String,
    modifier: Modifier = Modifier,
    code: Boolean = false,
    selectable: Boolean = code,
    onCopy: (() -> Unit)? = null,
) {
    val haptics = LocalHapticFeedback.current
    var copiedAt by remember(value) { mutableLongStateOf(0L) }
    LaunchedEffect(copiedAt) { if (copiedAt != 0L) { delay(2000); copiedAt = 0L } }
    ListItem(
        headlineContent = {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                val style = if (code) MaterialTheme.typography.bodyMedium.code() else MaterialTheme.typography.bodyLarge
                if (selectable) SelectionContainer { Text(value, style = style) }
                else Text(value, style = style)
            }
        },
        trailingContent = onCopy?.let { copy -> {
            IconButton(onClick = {
                copy()
                copiedAt = SystemClock.elapsedRealtime()
                haptics.performHapticFeedback(HapticFeedbackType.Confirm)
            }) {
                Icon(if (copiedAt != 0L) Icons.Outlined.Check else Icons.Outlined.ContentCopy,
                    str(R.string.text_copy_d6e9651) + " · " + label,
                    tint = if (copiedAt != 0L) MaterialTheme.colorScheme.primary else LocalContentColor.current)
            }
        } },
        modifier = modifier.clipTo(rowShape())
            .semantics { if (copiedAt != 0L) stateDescription = str(R.string.vault_copied) }
            .heightIn(min = 56.dp),
    )
}

/** A technical identifier, digest or fingerprint inside a [Section] or [DetailCard], always LTR monospace. */
@Composable internal fun CodeRow(
    label: String,
    value: String,
    modifier: Modifier = Modifier,
    onCopy: (() -> Unit)? = null,
) = InfoRow(label = label, value = value, modifier = modifier, code = true, selectable = true, onCopy = onCopy)

/** A transparent action row inside a [ModalBottomSheet]. */
@Composable internal fun SheetAction(
    label: String,
    icon: ImageVector? = null,
    modifier: Modifier = Modifier,
    @DrawableRes glyph: Int? = null,
    supporting: String? = null,
    enabled: Boolean = true,
    destructive: Boolean = false,
    onClick: () -> Unit,
) {
    val color = if (destructive) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface
    val leading: (@Composable () -> Unit)? = when {
        glyph != null -> { { Icon(painterResource(glyph), null, tint = color) } }
        icon != null -> { { Icon(icon, null, tint = color) } }
        else -> null
    }
    ListItem(
        headlineContent = { Text(label, color = color) },
        supportingContent = supporting?.let { { Text(it) } },
        leadingContent = leading,
        colors = ListItemDefaults.colors(containerColor = Color.Transparent),
        modifier = modifier.clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .then(if (enabled) Modifier else Modifier.alpha(0.45f))
            .heightIn(min = 56.dp),
    )
}

/** One labelled, keyboard-operable accessibility target for the whole setting. */
@Composable internal fun SettingToggle(
    title: String,
    checked: Boolean,
    change: (Boolean) -> Unit,
    enabled: Boolean = true,
    description: String? = null,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
) {
    val haptics = LocalHapticFeedback.current
    ListItem(
        leadingContent = icon?.let { { RowIcon(it) } },
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

/** A compact row of a browse list: no chevron and no divider, as in Google's list apps; [emphasized] marks unread rows. */
@Composable internal fun BrowseRow(
    title: String, modifier: Modifier, leading: @Composable () -> Unit,
    trailing: @Composable RowScope.() -> Unit = {}, emphasized: Boolean = false, supporting: @Composable () -> Unit = {},
) {
    val largeText = LocalDensity.current.fontScale >= 1.5f
    // Clip before the caller's click handling so the pressed state keeps rounded corners.
    Row(Modifier.clip(MaterialTheme.shapes.small).then(modifier).fillMaxWidth().heightIn(min = 56.dp).padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.sizeIn(minWidth = 36.dp, minHeight = 36.dp), contentAlignment = Alignment.Center) { leading() }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = if (emphasized) FontWeight.Bold else FontWeight.Medium,
                maxLines = if (largeText) Int.MAX_VALUE else 1, overflow = TextOverflow.Ellipsis)
            CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.onSurfaceVariant) {
                ProvideTextStyle(MaterialTheme.typography.bodySmall, supporting)
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp), content = trailing)
    }
}
