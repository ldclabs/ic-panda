package net.dmsg.android.ui.kit

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

/**
 * Nothing to show yet: an optional mark, one line saying so, one line of detail and at most one
 * action, centred in the space it is given.
 */
@Composable internal fun EmptyState(
    text: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    detail: String? = null,
    @DrawableRes glyph: Int? = null,
    action: (@Composable () -> Unit)? = null,
) {
    val colors = MaterialTheme.colorScheme
    Column(modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 40.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically)) {
        val painter = glyph?.let { painterResource(it) } ?: icon?.let { rememberVectorPainter(it) }
        painter?.let {
            Box(Modifier.padding(bottom = 8.dp).size(72.dp).background(colors.secondaryContainer, CircleShape),
                contentAlignment = Alignment.Center) {
                Icon(it, null, Modifier.size(32.dp), tint = colors.onSecondaryContainer)
            }
        }
        Text(text, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        detail?.let {
            Text(it, style = MaterialTheme.typography.bodyMedium, color = colors.onSurfaceVariant, textAlign = TextAlign.Center)
        }
        action?.let { Box(Modifier.padding(top = 8.dp)) { it() } }
    }
}
