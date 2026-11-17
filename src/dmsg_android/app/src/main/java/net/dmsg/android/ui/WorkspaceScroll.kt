package net.dmsg.android.ui

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.*
import net.dmsg.android.core.WorkspaceNavigation

/** Only integer positions are saved. Text, selections and secrets stay with their owners. */
@Composable internal fun workspaceListState(navigation: WorkspaceNavigation, key: String): LazyListState {
    val owner = navigation.scrollOwner()
    val state = remember(owner, key) { navigation.scrollPosition(key).let { LazyListState(it.first, it.offset) } }
    LaunchedEffect(state) { snapshotFlow { WorkspaceNavigation.ScrollPosition(state.firstVisibleItemIndex, state.firstVisibleItemScrollOffset) }
        .collect { navigation.rememberScroll(owner, key, it) } }
    return state
}

@Composable internal fun workspaceScrollState(navigation: WorkspaceNavigation, key: String): ScrollState {
    val owner = navigation.scrollOwner()
    val state = remember(owner, key) { ScrollState(navigation.scrollPosition(key).offset) }
    LaunchedEffect(state) { snapshotFlow { state.value }.collect { navigation.rememberScroll(owner, key, WorkspaceNavigation.ScrollPosition(offset = it)) } }
    return state
}
