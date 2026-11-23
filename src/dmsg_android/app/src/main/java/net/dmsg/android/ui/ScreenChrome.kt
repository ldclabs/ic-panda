package net.dmsg.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.runtime.*
import androidx.compose.ui.unit.dp

/**
 * What the active page adds to the shared scaffold.
 */
@Stable internal class ScreenChrome {
    var fab by mutableStateOf<(@Composable () -> Unit)?>(null)
}

/** Room below a list's last row for an extended FAB (56 dp) on its 16 dp margin, plus a 16 dp gap. */
internal val FabClearance = 88.dp

internal val LocalScreenChrome = staticCompositionLocalOf { ScreenChrome() }
internal val LocalPageActive = staticCompositionLocalOf { true }

/** The page's floating action button, shown by the scaffold above the navigation bar and below snackbars. */
@Composable internal fun ScreenFab(content: @Composable () -> Unit) {
    if (!LocalPageActive.current) return
    val chrome = LocalScreenChrome.current
    val latest by rememberUpdatedState(content)
    DisposableEffect(chrome) {
        val slot: @Composable () -> Unit = { latest() }
        chrome.fab = slot
        onDispose { if (chrome.fab === slot) chrome.fab = null }
    }
}

/**
 * The title and actions a page shows in the bar its frame draws: the page bar on phones, or the
 * detail pane's bar on wide windows. Without a declared title the frame uses the route's name.
 */
@Stable internal class PageBar {
    var title by mutableStateOf<String?>(null)
    var subtitle by mutableStateOf<String?>(null)
    var leading by mutableStateOf<(@Composable () -> Unit)?>(null)
    var actions by mutableStateOf<(@Composable RowScope.() -> Unit)?>(null)
    internal var owner: Any? = null
}

internal val LocalPageBar = staticCompositionLocalOf<PageBar?> { null }

/**
 * Declares the page's bar [title] and [actions] while the page is shown. Stable wrapper slots read
 * the latest lambdas through [rememberUpdatedState] so the bar follows page recompositions without
 * forcing the outer bar to recreate its slots on every tick.
 */
@Composable internal fun ScreenBar(title: String?, subtitle: String? = null, leading: (@Composable () -> Unit)? = null, actions: @Composable RowScope.() -> Unit = {}) {
    val bar = LocalPageBar.current ?: return
    val owner = remember { Any() }
    val latestLeading by rememberUpdatedState(leading)
    val latestActions by rememberUpdatedState(actions)
    val leadingSlot: @Composable () -> Unit = remember { { latestLeading?.invoke() } }
    val actionsSlot: @Composable RowScope.() -> Unit = remember { { latestActions() } }
    SideEffect {
        bar.owner = owner
        bar.title = title
        bar.subtitle = subtitle
        bar.leading = if (leading != null) leadingSlot else null
        bar.actions = actionsSlot
    }
    DisposableEffect(bar) {
        onDispose { if (bar.owner === owner) { bar.owner = null; bar.title = null; bar.subtitle = null; bar.leading = null; bar.actions = null } }
    }
}

/** A bar title with the page's optional [leading] avatar and [subtitle]. */
@Composable internal fun PageBarTitle(title: String, subtitle: String?, leading: (@Composable () -> Unit)?) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        leading?.invoke()
        Column {
            Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis)
            subtitle?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis) }
        }
    }
}
