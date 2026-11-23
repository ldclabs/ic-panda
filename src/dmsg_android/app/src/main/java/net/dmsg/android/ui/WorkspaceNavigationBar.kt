package net.dmsg.android.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import net.dmsg.android.R
import net.dmsg.android.platform.str

internal data class WorkspaceDestination(val id: String, val label: Int, @param:DrawableRes val icon: Int)

internal val workspaceDestinations = listOf(
    WorkspaceDestination("vault", R.string.text_vault_ede2a06, R.drawable.glyph_vault),
    WorkspaceDestination("messages", R.string.text_messages_3a9ab2d, R.drawable.glyph_channels),
    WorkspaceDestination("files", R.string.text_files_c79ddc2, R.drawable.glyph_folder),
    WorkspaceDestination("account", R.string.text_account_4ab56e9, R.drawable.glyph_account),
)

/** Phones: the Material 3 short navigation bar, flat on the container colour. */
@Composable internal fun WorkspaceNavigationBar(destinations: List<WorkspaceDestination>, current: String, select: (String) -> Unit) {
    ShortNavigationBar(Modifier.testTag("workspace-navigation"), containerColor = MaterialTheme.colorScheme.surfaceContainer) {
        destinations.forEach { destination ->
            ShortNavigationBarItem(selected = current == destination.id, onClick = { select(destination.id) },
                icon = { Icon(painterResource(destination.icon), null) },
                label = { Text(str(destination.label), maxLines = 1, overflow = TextOverflow.Ellipsis) })
        }
    }
}

/** Wide windows: the collapsed Material 3 navigation rail beside the panes. */
@Composable internal fun WorkspaceNavigationRail(destinations: List<WorkspaceDestination>, current: String, select: (String) -> Unit) {
    WideNavigationRail(Modifier.fillMaxHeight().testTag("workspace-navigation"),
        colors = WideNavigationRailDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainer)) {
        destinations.forEach { destination ->
            WideNavigationRailItem(railExpanded = false, selected = current == destination.id, onClick = { select(destination.id) },
                icon = { Icon(painterResource(destination.icon), null) }, label = { Text(str(destination.label)) })
        }
    }
}

/** The page area between the bars; the navigation bar takes its own space, so nothing scrolls beneath it. */
@Composable internal fun WorkspaceBody(padding: PaddingValues, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding), content = content)
}
