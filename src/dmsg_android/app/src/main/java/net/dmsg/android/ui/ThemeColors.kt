package net.dmsg.android.ui

import android.content.Context
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color

// Colour roles come from HCT tonal palettes; src/dmsg_android/DESIGN.md lists the seeds, chroma and tones.
// Pages sit on `surface`, cards on `surfaceContainerLow`; the other containers keep the M3 order.

internal val LightColors = lightColorScheme(
    primary = Color(0xFF145C45), onPrimary = Color.White, inversePrimary = Color(0xFF91D4B7),
    primaryContainer = Color(0xFFC0ECD7), onPrimaryContainer = Color(0xFF264E3E),
    secondary = Color(0xFF4F6359), onSecondary = Color.White,
    secondaryContainer = Color(0xFFD2E8DB), onSecondaryContainer = Color(0xFF384B42),
    tertiary = Color(0xFF9B4500), onTertiary = Color.White,
    tertiaryContainer = Color(0xFFFFDBCA), onTertiaryContainer = Color(0xFF763300),
    error = Color(0xFFBA1A1A), onError = Color.White,
    errorContainer = Color(0xFFFFDAD6), onErrorContainer = Color(0xFF93000A),
    background = Color(0xFFEFF1ED), onBackground = Color(0xFF191C1A),
    surface = Color(0xFFEFF1ED), onSurface = Color(0xFF191C1A), surfaceTint = Color(0xFF145C45),
    surfaceVariant = Color(0xFFDCE5DC), onSurfaceVariant = Color(0xFF404943),
    inverseSurface = Color(0xFF2E312E), inverseOnSurface = Color(0xFFEFF1ED),
    outline = Color(0xFF707972), outlineVariant = Color(0xFFC0C9C1), scrim = Color.Black,
    surfaceBright = Color(0xFFF8FAF5), surfaceDim = Color(0xFFD6D8D3),
    surfaceContainerLowest = Color.White, surfaceContainerLow = Color(0xFFFBFDF8), surfaceContainer = Color(0xFFEAECE7),
    surfaceContainerHigh = Color(0xFFE4E6E1), surfaceContainerHighest = Color(0xFFDEE0DC),
    primaryFixed = Color(0xFFC0ECD7), primaryFixedDim = Color(0xFFA5D0BB),
    onPrimaryFixed = Color(0xFF002116), onPrimaryFixedVariant = Color(0xFF264E3E),
    secondaryFixed = Color(0xFFD2E8DB), secondaryFixedDim = Color(0xFFB6CBC0),
    onSecondaryFixed = Color(0xFF0D1F18), onSecondaryFixedVariant = Color(0xFF384B42),
    tertiaryFixed = Color(0xFFFFDBCA), tertiaryFixedDim = Color(0xFFFFB68E),
    onTertiaryFixed = Color(0xFF331200), onTertiaryFixedVariant = Color(0xFF763300),
)

internal val DarkColors = darkColorScheme(
    primary = Color(0xFF91D4B7), onPrimary = Color(0xFF003828), inversePrimary = Color(0xFF145C45),
    primaryContainer = Color(0xFF264E3E), onPrimaryContainer = Color(0xFFC0ECD7),
    secondary = Color(0xFFB6CBC0), onSecondary = Color(0xFF22342C),
    secondaryContainer = Color(0xFF384B42), onSecondaryContainer = Color(0xFFD2E8DB),
    tertiary = Color(0xFFFFB68E), onTertiary = Color(0xFF532200),
    tertiaryContainer = Color(0xFF763300), onTertiaryContainer = Color(0xFFFFDBCA),
    error = Color(0xFFFFB4AB), onError = Color(0xFF690005),
    errorContainer = Color(0xFF93000A), onErrorContainer = Color(0xFFFFDAD6),
    background = Color(0xFF0E1512), onBackground = Color(0xFFDDE4DE),
    surface = Color(0xFF0E1512), onSurface = Color(0xFFDDE4DE), surfaceTint = Color(0xFF91D4B7),
    surfaceVariant = Color(0xFF3C4A43), onSurfaceVariant = Color(0xFFBBCAC1),
    inverseSurface = Color(0xFFDDE4DE), inverseOnSurface = Color(0xFF2B322E),
    outline = Color(0xFF85948C), outlineVariant = Color(0xFF3C4A43), scrim = Color.Black,
    surfaceBright = Color(0xFF333B37), surfaceDim = Color(0xFF0E1512),
    surfaceContainerLowest = Color(0xFF09100D), surfaceContainerLow = Color(0xFF161D1A), surfaceContainer = Color(0xFF1A211E),
    surfaceContainerHigh = Color(0xFF242C28), surfaceContainerHighest = Color(0xFF2F3633),
    primaryFixed = Color(0xFFC0ECD7), primaryFixedDim = Color(0xFFA5D0BB),
    onPrimaryFixed = Color(0xFF002116), onPrimaryFixedVariant = Color(0xFF264E3E),
    secondaryFixed = Color(0xFFD2E8DB), secondaryFixedDim = Color(0xFFB6CBC0),
    onSecondaryFixed = Color(0xFF0D1F18), onSecondaryFixedVariant = Color(0xFF384B42),
    tertiaryFixed = Color(0xFFFFDBCA), tertiaryFixedDim = Color(0xFFFFB68E),
    onTertiaryFixed = Color(0xFF331200), onTertiaryFixedVariant = Color(0xFF763300),
)

/** The app's scheme, or the wallpaper's when dynamic colour is on. */
internal fun appColors(context: Context, dark: Boolean, dynamic: Boolean): ColorScheme {
    val fixed = if (dark) DarkColors else LightColors
    if (!dynamic) return fixed
    return (if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)).asAppColors(fixed, dark)
}

/**
 * A wallpaper scheme in this app's roles. In light mode the page moves one container step down so
 * cards stay lighter than the page; error roles stay the app's own so a warning reads the same on
 * any wallpaper.
 */
internal fun ColorScheme.asAppColors(fixed: ColorScheme, dark: Boolean): ColorScheme {
    val paper = if (dark) this else copy(background = surfaceContainerLow, surface = surfaceContainerLow, surfaceContainerLow = surfaceContainerLowest)
    return paper.copy(error = fixed.error, onError = fixed.onError, errorContainer = fixed.errorContainer, onErrorContainer = fixed.onErrorContainer)
}

/** The page behind content: scaffold, top bar and sticky headers. */
internal val ColorScheme.page: Color get() = background

/** Card paper, one step lighter than the page in both modes: grouped rows, detail cards, sheets and search fields. */
internal val ColorScheme.card: Color get() = surfaceContainerLow

/** Fullscreen image lightbox backdrop: deep dark tone in both light and dark themes. */
internal val ColorScheme.viewerBackdrop: Color get() = Color(0xFF0A0F0D)

/** Translucent floating bar over the dark image backdrop. */
internal val ColorScheme.viewerChrome: Color get() = Color(0xCC161E1B)

/** High-contrast foreground on [viewerBackdrop] and [viewerChrome]. */
internal val ColorScheme.onViewerBackdrop: Color get() = Color(0xFFEFF3F0)

/** Secondary foreground on [viewerBackdrop] and [viewerChrome]. */
internal val ColorScheme.onViewerBackdropMuted: Color get() = Color(0xFFB3C0B9)
