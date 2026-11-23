package net.dmsg.android.ui.kit

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

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
