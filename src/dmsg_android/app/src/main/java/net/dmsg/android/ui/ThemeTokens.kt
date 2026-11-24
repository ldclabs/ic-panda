package net.dmsg.android.ui

import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.InfiniteRepeatableSpec
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ProgressIndicatorDefaults
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.util.Locale

/** Corners: 8 text fields, menus and snackbars; 12 chips and thumbnails; 16 cards and notices; 24 grouped panels; 28 sheets and dialogs. */
internal val DMsgShapes = Shapes(
    extraSmall = RoundedCornerShape(8.dp), small = RoundedCornerShape(12.dp), medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(24.dp), extraLarge = RoundedCornerShape(28.dp),
)

/** Inner corners of rows inside a section; the section keeps the large outer corners. */
internal val segmentShape = RoundedCornerShape(4.dp)

private val ideographic = setOf("zh", "ja", "ko")

/** M3 type scale on the system sans; Chinese, Japanese and Korean drop the Latin tracking, which only spaces ideographs apart. */
internal fun dmsgTypography(locale: Locale): Typography {
    val base = Typography()
    if (locale.language !in ideographic) return base
    fun TextStyle.untracked() = copy(letterSpacing = 0.sp)
    return Typography(
        displayLarge = base.displayLarge.untracked(), displayMedium = base.displayMedium.untracked(),
        displaySmall = base.displaySmall.untracked(), headlineLarge = base.headlineLarge.untracked(),
        headlineMedium = base.headlineMedium.untracked(), headlineSmall = base.headlineSmall.untracked(),
        titleLarge = base.titleLarge.untracked(), titleMedium = base.titleMedium.untracked(), titleSmall = base.titleSmall.untracked(),
        bodyLarge = base.bodyLarge.untracked(), bodyMedium = base.bodyMedium.untracked(), bodySmall = base.bodySmall.untracked(),
        labelLarge = base.labelLarge.untracked(), labelMedium = base.labelMedium.untracked(), labelSmall = base.labelSmall.untracked(),
    )
}

/** Identifiers, fingerprints and digests: monospace keeps groups aligned and 0 apart from O; always left to right. */
internal fun TextStyle.code(): TextStyle = copy(fontFamily = FontFamily.Monospace, textDirection = TextDirection.Ltr)

/** One-time codes and countdowns: tabular figures stop digits from shifting as they change. */
internal fun TextStyle.tabular(): TextStyle = copy(fontFeatureSettings = "tnum")

/** The only page margin: 16 dp on phones, 24 dp once the window is 600 dp wide. */
@Composable internal fun pageMargin(): Dp = if (LocalWindowInfo.current.containerDpSize.width >= 600.dp) 24.dp else 16.dp

/** Padding around a whole page; [top] is 0 under a browse header and [bottom] grows to [FabClearance] under a FAB. */
@Composable internal fun pagePadding(top: Dp = pageMargin(), bottom: Dp = pageMargin()): PaddingValues {
    val margin = pageMargin()
    return PaddingValues(start = margin, top = top, end = margin, bottom = bottom)
}

/**
 * Material 3 expressive springs and the skeleton pulse. Movement may overshoot a little; fades
 * never do. With Android's "remove animations" setting on, Compose finishes the springs at once.
 * The verbs they serve are the desktop's (docs/motion.zh.md); see DESIGN.md, Motion.
 */
internal object Motion {
    /** Pages and blocks that unfold: about 0.26 s, 1.5% overshoot. */
    fun <T> spatial(): FiniteAnimationSpec<T> = spring(dampingRatio = 0.8f, stiffness = 380f)
    /** Small things settling into a new state (a tick, a badge, a star): fast spatial, about 0.24 s, 9% overshoot. */
    fun <T> spatialFast(): FiniteAnimationSpec<T> = spring(dampingRatio = 0.6f, stiffness = 800f)
    fun <T> effects(): FiniteAnimationSpec<T> = spring(dampingRatio = 1f, stiffness = 1600f)
    fun <T> effectsFast(): FiniteAnimationSpec<T> = spring(dampingRatio = 1f, stiffness = 3800f)

    /** A countdown ring sweeping evenly through one second. */
    fun <T> tick(): FiniteAnimationSpec<T> = tween(1000, easing = LinearEasing)

    /** A determinate progress bar gliding to its new value. */
    fun progress(): AnimationSpec<Float> = ProgressIndicatorDefaults.ProgressAnimationSpec

    /** The skeleton's breathing; a loop needs a fixed duration rather than a spring. */
    fun <T> pulse(): InfiniteRepeatableSpec<T> = infiniteRepeatable(tween(900), RepeatMode.Reverse)
}

/** A message bubble: large corners, tucked in on the sender's side next to the same sender's neighbouring messages. */
internal fun bubbleShape(mine: Boolean, joinsPrevious: Boolean, joinsNext: Boolean): Shape {
    val large = 20.dp
    val small = 6.dp
    return RoundedCornerShape(
        topStart = if (!mine && joinsPrevious) small else large, topEnd = if (mine && joinsPrevious) small else large,
        bottomEnd = if (mine && joinsNext) small else large, bottomStart = if (!mine && joinsNext) small else large,
    )
}
