package net.dmsg.android.ui

import android.view.accessibility.AccessibilityManager
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.annotation.DrawableRes
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.platform.str

internal data class WorkspaceDestination(val id: String, val label: Int, @param:DrawableRes val icon: Int)

internal val workspaceDestinations = listOf(
    WorkspaceDestination("vault", R.string.text_vault_ede2a06, R.drawable.workspace_vault),
    WorkspaceDestination("messages", R.string.text_messages_3a9ab2d, R.drawable.workspace_channels),
    WorkspaceDestination("files", R.string.text_files_c79ddc2, R.drawable.workspace_folder),
    WorkspaceDestination("account", R.string.text_account_4ab56e9, R.drawable.workspace_account),
)

internal val LocalWorkspaceBarInset = compositionLocalOf { 0.dp }

@Composable internal fun FloatingWorkspaceBar(destinations: List<WorkspaceDestination>, current: String, select: (String) -> Unit) {
    val colors = MaterialTheme.colorScheme
    val largeLabels = LocalDensity.current.fontScale >= 1.5f
    Box(Modifier.fillMaxWidth()
        .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
        .padding(horizontal = if (largeLabels) 4.dp else 16.dp, vertical = 6.dp), contentAlignment = Alignment.Center) {
        Surface(
            modifier = Modifier.widthIn(max = 448.dp).fillMaxWidth().testTag("workspace-navigation"),
            shape = CircleShape, color = colors.surface.copy(alpha = 0.88f),
            border = BorderStroke(1.dp, colors.outline.copy(alpha = 0.35f)), shadowElevation = 4.dp,
        ) {
            Row(Modifier.selectableGroup().padding(5.dp).height(IntrinsicSize.Min)) {
                destinations.forEach { destination ->
                    val selected = current == destination.id
                    val ink = if (selected) colors.primary else colors.onSurfaceVariant
                    Column(
                        Modifier.weight(1f).fillMaxHeight().heightIn(min = 52.dp).clip(CircleShape)
                            .background(if (selected) colors.primary.copy(alpha = 0.13f) else Color.Transparent)
                            .selectable(selected = selected, role = Role.Tab, onClick = { select(destination.id) })
                            .padding(horizontal = if (largeLabels) 0.dp else 3.dp, vertical = 4.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(2.dp, if (largeLabels) Alignment.Top else Alignment.CenterVertically),
                    ) {
                        Icon(painterResource(destination.icon), null, Modifier.size(22.dp), tint = ink)
                        Text(str(destination.label), style = MaterialTheme.typography.labelMedium,
                            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium, color = ink, textAlign = TextAlign.Center)
                    }
                }
            }
        }
    }
}

@Composable private fun touchExplorationEnabled(): Boolean {
    val manager = LocalContext.current.getSystemService(AccessibilityManager::class.java)
    var enabled by remember(manager) { mutableStateOf(manager.isTouchExplorationEnabled) }
    DisposableEffect(manager) {
        val listener = AccessibilityManager.TouchExplorationStateChangeListener { enabled = it }
        manager.addTouchExplorationStateChangeListener(listener)
        enabled = manager.isTouchExplorationEnabled
        onDispose { manager.removeTouchExplorationStateChangeListener(listener) }
    }
    return enabled
}

/** Scroll behind the floating bar. Its measured height is reserved at the end of each root list. */
@Composable internal fun WorkspaceBody(padding: PaddingValues, floatingBar: Boolean, content: @Composable ColumnScope.() -> Unit) {
    // A keyboard or screen reader brings the focused row into its measured viewport.
    val underBar = floatingBar && LocalInputModeManager.current.inputMode == InputMode.Touch && !touchExplorationEnabled()
    val direction = LocalLayoutDirection.current
    val systemBottom = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val viewport = if (underBar) PaddingValues(start = padding.calculateStartPadding(direction), top = padding.calculateTopPadding(),
        end = padding.calculateEndPadding(direction), bottom = systemBottom) else padding
    val scrollClearance = if (underBar) (padding.calculateBottomPadding() - systemBottom).coerceAtLeast(0.dp) else 0.dp
    CompositionLocalProvider(LocalWorkspaceBarInset provides scrollClearance) {
        Column(Modifier.fillMaxSize().padding(viewport).consumeWindowInsets(padding), content = content)
    }
}
