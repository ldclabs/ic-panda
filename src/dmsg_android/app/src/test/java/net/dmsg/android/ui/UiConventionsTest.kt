package net.dmsg.android.ui

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pages take colour, corners, type and margins from the theme (src/dmsg_android/DESIGN.md), not from literals. */
class UiConventionsTest {
    private val root = File("src/main/java/net/dmsg/android")
    private val sources = root.walk().filter { it.isFile && it.extension == "kt" }.toList()

    @Test fun scansTheAppSources() = assertTrue(sources.map { it.name }.containsAll(listOf("DMsgApp.kt", "Theme.kt", "ThemeTokens.kt")))

    @Test fun coloursComeFromRoles() = assertNone("raw colour; use a colour role", Regex("""\bColor\(0x"""), "ThemeColors.kt")

    @Test fun colourSchemesAreDefinedOnce() = assertNone("colour scheme outside the theme", Regex("""\b(light|dark)ColorScheme\("""), "ThemeColors.kt")

    @Test fun themeIsProvidedOnce() = assertNone("nested MaterialTheme; use DMsgTheme or OnCard", Regex("""\bMaterialTheme\("""), "Theme.kt")

    @Test fun cornersComeFromTheShapeScale() = assertNone("literal corner; use MaterialTheme.shapes", Regex("""\b(RoundedCornerShape|CutCornerShape)\(|\bShapes\("""), "ThemeTokens.kt")

    @Test fun typeComesFromTheTypeScale() = assertNone("literal type; use MaterialTheme.typography, code() or tabular()",
        Regex("""\b\d+(\.\d+)?\.sp\b|\bTypography\(|FontFamily\.Monospace|fontFeatureSettings"""), "ThemeTokens.kt")

    @Test fun pagesUseTheSharedMargin() = assertNone("literal page padding; use pagePadding() or pageMargin()", Regex(
        """contentPadding\s*=\s*PaddingValues\(\d+\.dp""" +
            """|verticalScroll\((?:[^()]|\([^()]*\))*\)(?:\.(?:imePadding|systemBarsPadding)\(\))*\.padding\(\d+\.dp\)""" +
            """|fillMaxSize\(\)(?:\.(?:imePadding|systemBarsPadding)\(\))*\.padding\(\d+\.dp\)"""))

    private fun assertNone(reason: String, pattern: Regex, vararg allowed: String) {
        val found = sources.filter { it.name !in allowed }.flatMap { file ->
            file.readLines().mapIndexedNotNull { index, line ->
                if (pattern.containsMatchIn(line)) "${file.relativeTo(root)}:${index + 1}: ${line.trim().take(140)}" else null
            }
        }
        assertTrue("$reason:\n" + found.joinToString("\n"), found.isEmpty())
    }
}
