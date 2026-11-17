package net.dmsg.android

import android.graphics.Rect
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect as ComposeRect
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import net.dmsg.android.ui.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class AdaptiveLayoutTest {
    @get:Rule val compose = createComposeRule()

    @Test fun verticalHingeKeepsBothPanesClearAndMirrorsTheIndexInRtl() {
        var fold by mutableStateOf<WorkspaceFold?>(null)
        var direction by mutableStateOf(LayoutDirection.Ltr)
        var bounds = ComposeRect.Zero
        compose.setContent {
            CompositionLocalProvider(LocalDensity provides Density(1f), LocalLayoutDirection provides direction) {
                DMsgTheme(false) { Box(Modifier.requiredSize(1000.dp, 600.dp).onGloballyPositioned { bounds = it.boundsInWindow() }) {
                    WorkspacePanes(fold, list = { Text("Index") }, detail = { Text("Selected content") })
                } }
            }
        }
        compose.runOnIdle { fold = WorkspaceFold(Rect(bounds.left.toInt() + 480, bounds.top.toInt(), bounds.left.toInt() + 520, bounds.bottom.toInt()), true) }
        compose.onNodeWithTag("workspace-list").assertWidthIsEqualTo(480.dp)
        compose.onNodeWithTag("workspace-detail").assertWidthIsEqualTo(480.dp)
        compose.onNodeWithTag("workspace-hinge").assertWidthIsEqualTo(40.dp)
        val first = compose.onNodeWithTag("workspace-list").getUnclippedBoundsInRoot().left
        compose.runOnIdle { direction = LayoutDirection.Rtl }
        val mirrored = compose.onNodeWithTag("workspace-list").getUnclippedBoundsInRoot().left
        assertTrue(mirrored > first)
        compose.onNodeWithTag("workspace-list").assertWidthIsEqualTo(480.dp)
    }

    @Test fun tabletopUsesOneUnobstructedAreaAndLargeTextStacksLongValues() {
        var fold by mutableStateOf<WorkspaceFold?>(null)
        var bounds = ComposeRect.Zero
        compose.setContent {
            CompositionLocalProvider(LocalDensity provides Density(1f, 2f)) {
                DMsgTheme(false) { Box(Modifier.requiredSize(600.dp, 600.dp).onGloballyPositioned { bounds = it.boundsInWindow() }) {
                    FoldSafeWorkspace(fold, false) { Column(Modifier.fillMaxSize().testTag("safe-content")) {
                        LabelValue("Complete account identifier", "aaaaa-bbbbb-ccccc-ddddd-eeeee-fffff-ggggg")
                    } }
                } }
            }
        }
        compose.runOnIdle { fold = WorkspaceFold(Rect(bounds.left.toInt(), bounds.top.toInt() + 250, bounds.right.toInt(), bounds.top.toInt() + 300), false) }
        compose.onNodeWithTag("safe-content").assertHeightIsEqualTo(300.dp)
        val label = compose.onNodeWithText("Complete account identifier").getUnclippedBoundsInRoot()
        val value = compose.onNodeWithText("aaaaa-bbbbb-ccccc-ddddd-eeeee-fffff-ggggg").getUnclippedBoundsInRoot()
        assertTrue(value.top >= label.bottom)
    }
}
