package net.dmsg.android.ui

import androidx.compose.foundation.layout.RowScope
import androidx.compose.runtime.*
import androidx.compose.ui.unit.dp

/**
 * What the displayed page adds to the shared scaffold. Pages declare it while they are shown;
 * during a transition the entering page's declaration wins.
 */
@Stable internal class ScreenChrome {
    var fab by mutableStateOf<(@Composable () -> Unit)?>(null)
}

/** Room below a list's last row for an extended FAB (56 dp) on its 16 dp margin, plus a 16 dp gap. */
internal val FabClearance = 88.dp

internal val LocalScreenChrome = staticCompositionLocalOf { ScreenChrome() }

/** The page's floating action button, shown by the scaffold above the navigation bar and below snackbars. */
@Composable internal fun ScreenFab(content: @Composable () -> Unit) {
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
    var actions by mutableStateOf<(@Composable RowScope.() -> Unit)?>(null)
    internal var owner: Any? = null
}

internal val LocalPageBar = staticCompositionLocalOf<PageBar?> { null }

/**
 * Declares the page's bar [title] and [actions] while the page is shown. Every recomposition hands
 * the bar a new wrapper, so the bar redraws the actions with the page's current state.
 */
@Composable internal fun ScreenBar(title: String?, actions: @Composable RowScope.() -> Unit = {}) {
    val bar = LocalPageBar.current ?: return
    val owner = remember { Any() }
    SideEffect {
        bar.owner = owner
        bar.title = title
        bar.actions = { actions() }
    }
    DisposableEffect(bar) {
        onDispose { if (bar.owner === owner) { bar.owner = null; bar.title = null; bar.actions = null } }
    }
}
