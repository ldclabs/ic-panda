package net.dmsg.android.ui.kit

import android.os.SystemClock
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.ui.Motion

/** The copy icon that turns into a tick for a moment after copying: the tick springs in and the icon fades back. */
@Composable internal fun CopyIcon(copied: Boolean, contentDescription: String?) {
    AnimatedContent(copied, transitionSpec = {
        (if (targetState) scaleIn(Motion.spatialFast(), initialScale = 0.6f) + fadeIn(Motion.effects()) else fadeIn(Motion.effects())) togetherWith
            fadeOut(Motion.effectsFast())
    }, label = "copied") { tick ->
        Icon(if (tick) Icons.Outlined.Check else Icons.Outlined.ContentCopy, contentDescription,
            tint = if (tick) MaterialTheme.colorScheme.primary else LocalContentColor.current)
    }
}

/**
 * A button that copies and confirms in place: its icon turns into a tick and its label into
 * "Copied" for two seconds, with the Confirm haptic, so no notice is needed. The button keeps
 * the width of the longer label. [outlined] is a page's full-width secondary action; otherwise
 * it is a text button. A dialog's action leaves out the [icon].
 */
@Composable internal fun CopyButton(
    label: String,
    modifier: Modifier = Modifier,
    outlined: Boolean = false,
    enabled: Boolean = true,
    icon: Boolean = true,
    colors: ButtonColors? = null,
    copy: () -> Unit,
) {
    val haptics = LocalHapticFeedback.current
    var copiedAt by remember { mutableLongStateOf(0L) }
    LaunchedEffect(copiedAt) { if (copiedAt != 0L) { delay(2000); copiedAt = 0L } }
    val copied = copiedAt != 0L
    val shown by animateFloatAsState(if (copied) 1f else 0f, Motion.effects(), label = "copied")
    val click = {
        copy()
        copiedAt = SystemClock.elapsedRealtime()
        haptics.performHapticFeedback(HapticFeedbackType.Confirm)
    }
    val content: @Composable () -> Unit = {
        if (icon) { CopyIcon(copied, null); Spacer(Modifier.width(8.dp)) }
        Box {
            // Both labels take space, so the button keeps its width; only the shown one is read out.
            Text(label, Modifier.graphicsLayer { alpha = 1f - shown }.then(if (copied) Modifier.clearAndSetSemantics {} else Modifier))
            Text(str(R.string.vault_copied), Modifier.graphicsLayer { alpha = shown }.then(if (copied) Modifier else Modifier.clearAndSetSemantics {}))
        }
    }
    if (outlined) OutlinedButton(click, modifier.fillMaxWidth(), enabled, colors = colors ?: ButtonDefaults.outlinedButtonColors()) { content() }
    else TextButton(click, modifier, enabled, colors = colors ?: ButtonDefaults.textButtonColors()) { content() }
}

/** An icon button that copies: its icon turns into a tick for two seconds, with the Confirm haptic. */
@Composable internal fun CopyIconButton(contentDescription: String, copy: () -> Unit) {
    val haptics = LocalHapticFeedback.current
    var copiedAt by remember { mutableLongStateOf(0L) }
    LaunchedEffect(copiedAt) { if (copiedAt != 0L) { delay(2000); copiedAt = 0L } }
    IconButton(onClick = {
        copy()
        copiedAt = SystemClock.elapsedRealtime()
        haptics.performHapticFeedback(HapticFeedbackType.Confirm)
    }, Modifier.semantics { if (copiedAt != 0L) stateDescription = str(R.string.vault_copied) }) { CopyIcon(copiedAt != 0L, contentDescription) }
}
