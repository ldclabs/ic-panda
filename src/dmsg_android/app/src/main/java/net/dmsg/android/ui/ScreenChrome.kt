package net.dmsg.android.ui

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
