package net.dmsg.android.ui.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import net.dmsg.android.ui.Motion

/**
 * An inline banner: information on the secondary container, problems on the error container.
 * Actions sit at its end. Show at most one per screen; put explanations in helper text instead.
 */
@Composable fun Notice(text: String, warning: Boolean = false, action: (@Composable () -> Unit)? = null) {
    val colors = MaterialTheme.colorScheme
    Surface(Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.medium,
        color = if (warning) colors.errorContainer else colors.secondaryContainer,
        contentColor = if (warning) colors.onErrorContainer else colors.onSecondaryContainer) {
        Column(Modifier.padding(start = 16.dp, top = 12.dp, end = 16.dp, bottom = if (action == null) 12.dp else 4.dp)) {
            Row(Modifier.semantics(mergeDescendants = true) { if (warning) liveRegion = LiveRegionMode.Polite },
                horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Icon(if (warning) Icons.Outlined.ErrorOutline else Icons.Outlined.Info, null, Modifier.size(20.dp))
                Text(text, style = MaterialTheme.typography.bodyMedium)
            }
            action?.let {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) { it() }
            }
        }
    }
}

/**
 * A banner or block that comes and goes while its page is open: it unfolds and the content
 * below makes room, and folds away when it leaves. Visible when the page opens, it shows at once.
 */
@Composable internal fun Reveal(visible: Boolean, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    AnimatedVisibility(visible, modifier, enter = expandVertically(Motion.spatial()) + fadeIn(Motion.effects()),
        exit = shrinkVertically(Motion.spatial()) + fadeOut(Motion.effectsFast())) { content() }
}

/** The last non-null [value], so content that is leaving can still draw what it showed. */
@Composable internal fun <T : Any> retained(value: T?): T? {
    val last = remember { arrayOfNulls<Any>(1) }
    if (value != null) last[0] = value
    @Suppress("UNCHECKED_CAST") return last[0] as T?
}
