package net.dmsg.android.ui.kit

import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.progressSemantics
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import net.dmsg.android.ui.Motion

/** First-load placeholder: list-shaped rows that pulse gently until the content arrives. */
@Composable internal fun SkeletonRows(modifier: Modifier = Modifier, count: Int = 3) {
    val pulse by rememberInfiniteTransition(label = "skeleton")
        .animateFloat(0.45f, 0.9f, Motion.pulse(), label = "pulse")
    val fill = MaterialTheme.colorScheme.surfaceContainerHighest
    Column(modifier.fillMaxWidth().progressSemantics().graphicsLayer { alpha = pulse },
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        repeat(count) { row ->
            Row(Modifier.heightIn(min = 56.dp), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Box(Modifier.size(36.dp).background(fill, MaterialTheme.shapes.small))
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Box(Modifier.fillMaxWidth(if (row % 2 == 0) 0.62f else 0.48f).height(14.dp).background(fill, CircleShape))
                    Box(Modifier.fillMaxWidth(0.3f).height(10.dp).background(fill, CircleShape))
                }
            }
        }
    }
}
