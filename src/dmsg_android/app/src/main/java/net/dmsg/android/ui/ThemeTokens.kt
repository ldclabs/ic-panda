package net.dmsg.android.ui

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
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

/** Padding around a whole page; [top] is 0 under a browse header and [bottomInset] keeps the last row clear of the floating bar. */
@Composable internal fun pagePadding(top: Dp = pageMargin(), bottomInset: Dp = 0.dp): PaddingValues {
    val margin = pageMargin()
    return PaddingValues(start = margin, top = top, end = margin, bottom = margin + bottomInset)
}
