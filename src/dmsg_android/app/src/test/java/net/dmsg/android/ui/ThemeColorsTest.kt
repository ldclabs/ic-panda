package net.dmsg.android.ui

import androidx.compose.ui.graphics.Color
import java.io.File
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ThemeColorsTest {
    private val schemes = mapOf("light" to LightColors, "dark" to DarkColors)

    @Test fun textAndControlsMeetContrast() {
        val failures = schemes.flatMap { (mode, s) ->
            val pairs = listOf(
                // Body text and text on containers: WCAG AAA.
                Triple("onSurface on page", s.onSurface to s.page, 7.0),
                Triple("onSurface on card", s.onSurface to s.card, 7.0),
                Triple("onSurfaceVariant on page", s.onSurfaceVariant to s.page, 7.0),
                Triple("onSurfaceVariant on card", s.onSurfaceVariant to s.card, 7.0),
                Triple("onSurfaceVariant on dialog", s.onSurfaceVariant to s.surfaceContainerHigh, 7.0),
                Triple("onSurface on bubble", s.onSurface to s.surfaceContainerHighest, 7.0),
                Triple("onPrimaryContainer", s.onPrimaryContainer to s.primaryContainer, 7.0),
                Triple("onSecondaryContainer", s.onSecondaryContainer to s.secondaryContainer, 7.0),
                Triple("onTertiaryContainer", s.onTertiaryContainer to s.tertiaryContainer, 7.0),
                Triple("onErrorContainer", s.onErrorContainer to s.errorContainer, 7.0),
                // Filled accents, links and actions: WCAG AA.
                Triple("onPrimary", s.onPrimary to s.primary, 4.5),
                Triple("onSecondary", s.onSecondary to s.secondary, 4.5),
                Triple("onTertiary", s.onTertiary to s.tertiary, 4.5),
                Triple("onError", s.onError to s.error, 4.5),
                Triple("primary on page", s.primary to s.page, 4.5),
                Triple("primary on card", s.primary to s.card, 4.5),
                Triple("primary on notice", s.primary to s.secondaryContainer, 4.5),
                Triple("primary on dialog", s.primary to s.surfaceContainerHigh, 4.5),
                Triple("error on page", s.error to s.page, 4.5),
                Triple("error on card", s.error to s.card, 4.5),
                Triple("snackbar text", s.inverseOnSurface to s.inverseSurface, 4.5),
                Triple("snackbar action", s.inversePrimary to s.inverseSurface, 4.5),
                // Field borders and other control boundaries: WCAG 1.4.11.
                Triple("outline on page", s.outline to s.page, 3.0),
                Triple("outline on card", s.outline to s.card, 3.0),
            )
            pairs.mapNotNull { (name, colors, minimum) ->
                val ratio = contrast(colors.first, colors.second)
                if (ratio < minimum) "$mode $name: ${"%.2f".format(ratio)} < $minimum" else null
            }
        }
        assertTrue(failures.joinToString("\n"), failures.isEmpty())
    }

    @Test fun wallpaperColoursKeepTheCardAndErrorRoles() {
        // The Material baseline stands in for a wallpaper scheme; on a device the system supplies it.
        for ((mode, wallpaper, fixed) in listOf(Triple("light", androidx.compose.material3.lightColorScheme(), LightColors),
                Triple("dark", androidx.compose.material3.darkColorScheme(), DarkColors))) {
            val s = wallpaper.asAppColors(fixed, dark = mode == "dark")
            assertTrue("$mode: card is lighter than the page", luminance(s.card) > luminance(s.page))
            assertEquals("$mode: page", s.background, s.surface)
            assertEquals("$mode: error", listOf(fixed.error, fixed.onError, fixed.errorContainer, fixed.onErrorContainer),
                listOf(s.error, s.onError, s.errorContainer, s.onErrorContainer))
            assertEquals("$mode: primary follows the wallpaper", wallpaper.primary, s.primary)
        }
    }

    @Test fun pagesAndCardsKeepTheirRoles() {
        for (s in schemes.values) {
            assertEquals(s.background, s.surface)
            assertEquals(s.page, s.background)
            assertEquals(s.card, s.surfaceContainerLow)
        }
    }

    @Test fun surfacesStepAwayFromTheCard() {
        // Light: cards and bright surfaces above the page, containers darken below it.
        with(LightColors) {
            assertOrdered("light", listOf(surfaceContainerLowest, surfaceContainerLow, surfaceBright, surface,
                surfaceContainer, surfaceContainerHigh, surfaceContainerHighest, surfaceDim).map(::luminance).reversed())
        }
        // Dark: the page is the dimmest surface; every container is lighter than the card below it.
        with(DarkColors) {
            assertEquals(surfaceDim, surface)
            assertOrdered("dark", listOf(surfaceContainerLowest, surface, surfaceContainerLow, surfaceContainer,
                surfaceContainerHigh, surfaceContainerHighest, surfaceBright).map(::luminance))
        }
    }

    @Test fun windowColoursMatchTheTheme() {
        val light = resourceColors("values/theme_colors.xml") + resourceColors("values/launcher_colors.xml")
        val dark = resourceColors("values-night/theme_colors.xml")
        assertEquals(LightColors.page, light["theme_page"])
        assertEquals(LightColors.primary, light["theme_primary"])
        assertEquals(LightColors.primary, light["launcher_green"])
        assertEquals(DarkColors.page, dark["theme_page"])
        assertEquals(DarkColors.primary, dark["theme_primary"])
    }

    private fun assertOrdered(mode: String, values: List<Double>) {
        values.zipWithNext().forEachIndexed { index, (lower, higher) ->
            assertTrue("$mode surface step $index: $lower >= $higher", lower < higher)
        }
    }

    private fun resourceColors(path: String): Map<String, Color> =
        Regex("""<color name="(\w+)">#([0-9A-Fa-f]{6})</color>""").findAll(File("src/main/res/$path").readText())
            .associate { it.groupValues[1] to Color(0xFF000000 or it.groupValues[2].toLong(16)) }

    private fun luminance(color: Color): Double {
        fun channel(value: Float): Double = if (value <= 0.04045f) value / 12.92 else ((value + 0.055) / 1.055).pow(2.4)
        return 0.2126 * channel(color.red) + 0.7152 * channel(color.green) + 0.0722 * channel(color.blue)
    }

    private fun contrast(a: Color, b: Color): Double {
        val x = luminance(a)
        val y = luminance(b)
        return (max(x, y) + 0.05) / (min(x, y) + 0.05)
    }
}
