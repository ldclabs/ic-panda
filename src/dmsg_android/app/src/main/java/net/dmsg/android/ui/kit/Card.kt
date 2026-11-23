package net.dmsg.android.ui.kit

import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import net.dmsg.android.ui.card

/** A block of related content on the card colour with the large corners. */
@Composable internal fun DetailCard(
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.card,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, color = color) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp), content = content)
    }
}
