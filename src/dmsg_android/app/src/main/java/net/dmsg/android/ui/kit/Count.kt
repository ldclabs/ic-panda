package net.dmsg.android.ui.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Badge
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import net.dmsg.android.ui.Motion

/**
 * A count whose changed digits roll in: from below when it grew, from above when it shrank.
 * Unchanged digits stay still and every frame shows the new value, never a made-up one in between.
 * [text] is the formatted count (localized digits, "99+"); [value] gives the direction.
 */
@Composable internal fun Count(text: String, value: Long, modifier: Modifier = Modifier) {
    val last = remember { longArrayOf(value) }
    val up = value >= last[0]
    SideEffect { last[0] = value }
    // Digits already there when the count first shows stay still; the badge itself pops in.
    var settled by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { settled = true }
    // Numbers read left to right in every language; the digits roll inside the count's own bounds.
    CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
        Row(modifier.clipToBounds().clearAndSetSemantics { contentDescription = text }) {
            text.forEachIndexed { index, digit ->
                // Keyed from the end, so 9 → 10 keeps the ones place and brings in the tens.
                key(text.length - index, digit) { Digit(digit, up, roll = settled) }
            }
        }
    }
}

@Composable private fun Digit(digit: Char, up: Boolean, roll: Boolean) {
    val direction = remember { if (up) 1f else -1f }
    val shown = remember { Animatable(if (roll) 0f else 1f) }
    LaunchedEffect(Unit) { shown.animateTo(1f, Motion.effects()) }
    Text(digit.toString(), Modifier.graphicsLayer {
        translationY = (1f - shown.value) * size.height * 0.5f * direction
        alpha = shown.value
    })
}

/**
 * A count badge: it pops in when the count leaves zero, rolls its digits, and shrinks away
 * at zero. [format] writes the count as the badge shows it.
 */
@Composable internal fun CountBadge(count: Long, format: (Long) -> String) {
    val shown = retained(count.takeIf { it > 0 })
    AnimatedVisibility(count > 0, enter = scaleIn(Motion.spatialFast(), initialScale = 0.6f) + fadeIn(Motion.effects()),
        exit = scaleOut(Motion.effectsFast(), targetScale = 0.6f) + fadeOut(Motion.effectsFast())) {
        shown?.let { value -> Badge { Count(format(value), value) } }
    }
}

/** A dot that pops in when something waits and shrinks away when nothing does. */
@Composable internal fun DotBadge(visible: Boolean) {
    AnimatedVisibility(visible, enter = scaleIn(Motion.spatialFast(), initialScale = 0.6f) + fadeIn(Motion.effects()),
        exit = scaleOut(Motion.effectsFast(), targetScale = 0.6f) + fadeOut(Motion.effectsFast())) { Badge() }
}
