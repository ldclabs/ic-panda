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

    @Test fun colourSchemesAreDefinedOnce() = assertNone("colour scheme outside the theme", Regex("""\b(light|dark|dynamicLight|dynamicDark)ColorScheme\("""), "ThemeColors.kt")

    @Test fun themeIsProvidedOnce() = assertNone("nested MaterialTheme; use DMsgTheme or OnCard", Regex("""\bMaterialTheme\("""), "Theme.kt")

    @Test fun cornersComeFromTheShapeScale() = assertNone("literal corner; use MaterialTheme.shapes", Regex("""\b(RoundedCornerShape|CutCornerShape)\(|\bShapes\("""), "ThemeTokens.kt")

    @Test fun typeComesFromTheTypeScale() = assertNone("literal type; use MaterialTheme.typography, code() or tabular()",
        Regex("""\b\d+(\.\d+)?\.sp\b|\bTypography\(|FontFamily\.Monospace|fontFeatureSettings"""), "ThemeTokens.kt")

    @Test fun pagesUseTheSharedMargin() = assertNone("literal page padding; use pagePadding() or pageMargin()", Regex(
        """contentPadding\s*=\s*PaddingValues\(\d+\.dp""" +
            """|verticalScroll\((?:[^()]|\([^()]*\))*\)(?:\.(?:imePadding|systemBarsPadding)\(\))*\.padding\(\d+\.dp\)""" +
            """|fillMaxSize\(\)(?:\.(?:imePadding|systemBarsPadding)\(\))*\.padding\(\d+\.dp\)"""))

    @Test fun motionComesFromTheTokens() = assertNone("literal animation spec; use Motion in ThemeTokens.kt",
        Regex("""\b(tween|spring|keyframes|infiniteRepeatable)\("""), "ThemeTokens.kt")

    @Test fun rowsHaveNoChevrons() = assertNone("row chevron; a row's trailing side shows its value or badge",
        Regex("""\b(KeyboardArrowRight|ChevronRight|NavigateNext)\b"""))

    @Test fun overflowMenusUseTheVerticalIcon() = assertNone("horizontal ellipsis; overflow menus use Icons.Outlined.MoreVert",
        Regex("""\bMoreHoriz\b"""))

    @Test fun segmentedChoicesUseTheKit() = assertNone("segmented button outside the kit; use SegmentedChoice",
        Regex("""\b(SingleChoiceSegmentedButtonRow|MultiChoiceSegmentedButtonRow)\("""), "Filters.kt")

    @Test fun cardsUseTheKit() = assertNone("raw Card; use DetailCard, Section or OutlinedCard",
        Regex("""(^|[^a-zA-Z0-9_])Card\s*[({]"""))

    @Test fun buttonIconsKeepSpacing() = assertNone("Icon immediately followed by Text without a Spacer",
        Regex("""Icon\([^)]*\)\s*;\s*Text\("""))

    @Test fun sharedComponentsLiveInTheKit() {
        val kit = File(root, "ui/kit")
        assertTrue(listOf("Card.kt", "Controls.kt", "Section.kt", "Rows.kt", "Notice.kt", "EmptyState.kt", "Skeleton.kt", "Filters.kt").all { File(kit, it).isFile })
        assertNone("shared component defined outside ui/kit", Regex(
            """fun (Section|ListRow|InfoRow|CodeRow|SheetAction|RowIcon|LabelValue|SettingToggle|SelectionRow|RadioChoiceRow|BrowseRow|BrowseIcon|BrowseChoice|SortMenu|MenuChoice|SegmentedChoice|BottomActionBar|SecretTextField|Avatar|Notice|EmptyState|DetailCard)\("""),
            *kit.list().orEmpty())
    }

    private fun assertNone(reason: String, pattern: Regex, vararg allowed: String) {
        val found = sources.filter { it.name !in allowed }.flatMap { file ->
            file.readLines().mapIndexedNotNull { index, line ->
                if (pattern.containsMatchIn(line)) "${file.relativeTo(root)}:${index + 1}: ${line.trim().take(140)}" else null
            }
        }
        assertTrue("$reason:\n" + found.joinToString("\n"), found.isEmpty())
    }
}
