package net.dmsg.android.ui.kit

import androidx.compose.animation.Crossfade
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import net.dmsg.android.ui.Motion

/**
 * A round avatar: the [image] when there is one, else an [icon], else the first letter of [label]
 * on one of the container tones, chosen by [seed] so the same account keeps its tone.
 */
@Composable internal fun Avatar(label: String, seed: String, modifier: Modifier = Modifier, size: Dp = 40.dp, image: ImageBitmap? = null, icon: Painter? = null) {
    val colors = MaterialTheme.colorScheme
    val tones = listOf(colors.primaryContainer to colors.onPrimaryContainer, colors.secondaryContainer to colors.onSecondaryContainer,
        colors.tertiaryContainer to colors.onTertiaryContainer)
    val (background, content) = tones[Math.floorMod(seed.hashCode(), tones.size)]
    Box(modifier.size(size).clip(CircleShape).background(background), contentAlignment = Alignment.Center) {
        // A picture that arrives after the initial fades in over it; one already loaded shows at once.
        Crossfade(image, animationSpec = Motion.effects(), label = "avatar") { picture ->
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                when {
                    picture != null -> Image(picture, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
                    icon != null -> Icon(icon, null, Modifier.size(size * 0.55f), tint = content)
                    else -> Text(initial(label), color = content, maxLines = 1,
                        style = if (size >= 40.dp) MaterialTheme.typography.titleMedium else MaterialTheme.typography.labelLarge)
                }
            }
        }
    }
}

private fun initial(label: String): String = label.trim().let { if (it.isEmpty()) "?" else String(Character.toChars(it.codePointAt(0))).uppercase() }
