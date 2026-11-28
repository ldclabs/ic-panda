package net.dmsg.android.ui

import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.ClientController
import net.dmsg.android.core.NoticeTarget
import java.util.UUID

/** A real clipped viewport, current window focus and resumed lifecycle are required. */
internal fun Modifier.reminderVisible(client: ClientController, target: NoticeTarget, through: ULong? = null, revision: Any? = null, onVisible: () -> Unit = {}): Modifier = composed {
    val owner = remember(target) { UUID.randomUUID().toString() }
    val view = LocalView.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val lifecycleState by lifecycle.currentStateFlow.collectAsState()
    val focused = LocalWindowInfo.current.isWindowFocused
    var visible by remember { mutableStateOf(false) }
    val active by rememberUpdatedState(LocalPageActive.current)
    val currentThrough by rememberUpdatedState(through)
    val whenVisible by rememberUpdatedState(onVisible)
    DisposableEffect(client, owner) { onDispose { visible = false; client.notifications.hide(owner) } }
    LaunchedEffect(active, visible, focused, lifecycleState, currentThrough, revision) {
        client.notifications.hide(owner)
        if (active && visible && focused && view.hasWindowFocus() && lifecycleState.isAtLeast(Lifecycle.State.RESUMED)) {
            whenVisible()
            client.notifications.observed(target, currentThrough, owner) { active && visible && view.hasWindowFocus() && lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        }
    }
    onGloballyPositioned {
        val bounds = it.boundsInWindow()
        visible = it.isAttached && bounds.width > 0 && bounds.height > 0
    }
}


/** Service/task pages use the original reminder episode, after their own read. */
@Composable internal fun Modifier.openedReminderVisible(client: ClientController, ready: Boolean, matches: (NoticeTarget) -> Boolean): Modifier {
    val item by client.notifications.opened.collectAsStateWithLifecycle()
    val value = item
    return if (ready && value != null && matches(value.target)) reminderVisible(client, value.target, value.version) else this
}
