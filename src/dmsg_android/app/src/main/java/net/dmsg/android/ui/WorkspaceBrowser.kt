package net.dmsg.android.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.platform.str

/** The shell's part of a root header: Back when the page was pushed, and the activity bell. */
internal class BrowseChrome(val back: (@Composable () -> Unit)?, val activity: @Composable () -> Unit)

internal val LocalBrowseChrome = staticCompositionLocalOf { BrowseChrome(null) {} }

internal enum class BrowseSearchMode { Browse, Editing, Results }

/**
 * A workspace root. Its search bar is the top bar, as in Gmail, Drive and Keep: while browsing
 * it also carries the activity bell and the page [actions]; searching adds Cancel.
 */
@Composable internal fun WorkspaceBrowser(
    title: String, query: String, change: (String) -> Unit, prompt: String,
    actions: @Composable RowScope.() -> Unit, content: @Composable (searching: Boolean) -> Unit,
) {
    var mode by remember { mutableStateOf(if (query.isEmpty()) BrowseSearchMode.Browse else BrowseSearchMode.Results) }
    var previousQuery by remember { mutableStateOf(query) }
    LaunchedEffect(query) {
        if (previousQuery.isNotEmpty() && query.isEmpty() && mode == BrowseSearchMode.Results) mode = BrowseSearchMode.Browse
        previousQuery = query
    }
    Column(Modifier.fillMaxSize().semantics { paneTitle = title }) {
        BrowseSearchRow(query, change, prompt, mode, actions) { mode = it }
        content(mode != BrowseSearchMode.Browse)
    }
}

/** Only editing owns an input connection. Returning to results never reopens the IME. */
@Composable internal fun BrowseSearchRow(query: String, change: (String) -> Unit, prompt: String,
    mode: BrowseSearchMode, actions: @Composable RowScope.() -> Unit = {}, setMode: (BrowseSearchMode) -> Unit) {
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
    val active = LocalPageActive.current
    val editor = remember { FocusRequester() }
    val cancelFocus = remember { FocusRequester() }
    fun dismiss() {
        if (query.isNotEmpty()) change("")
        setMode(BrowseSearchMode.Browse)
        focus.clearFocus()
        keyboard?.hide()
    }
    BackHandler(enabled = active && mode != BrowseSearchMode.Browse && !imeVisible, onBack = ::dismiss)
    Row(Modifier.fillMaxWidth().padding(horizontal = pageMargin(), vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        if (mode == BrowseSearchMode.Editing) {
            WorkspaceSearchField(query, change, prompt, Modifier.weight(1f).focusRequester(editor).testTag("browse-search-editor"), onSearch = {
                cancelFocus.requestFocus()
                setMode(BrowseSearchMode.Results)
                keyboard?.hide()
            })
            if (active) LaunchedEffect(Unit) {
                editor.requestFocus()
                keyboard?.show()
            }
        } else {
            val colors = MaterialTheme.colorScheme
            val chrome = LocalBrowseChrome.current
            if (mode == BrowseSearchMode.Browse) chrome.back?.invoke()
            Surface(onClick = { setMode(BrowseSearchMode.Editing) },
                modifier = Modifier.weight(1f).testTag("browse-search-entry").semantics { contentDescription = prompt },
                shape = CircleShape, color = colors.surfaceContainerHigh) {
                Row(Modifier.heightIn(min = 56.dp).padding(end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.width(52.dp), contentAlignment = Alignment.Center) { Icon(Icons.Outlined.Search, null, tint = colors.onSurfaceVariant) }
                    val labelModifier = if (query.isEmpty()) Modifier.clearAndSetSemantics {} else Modifier
                    Text(query.ifEmpty { prompt }, Modifier.weight(1f).then(labelModifier).padding(start = 4.dp, end = 8.dp, top = 12.dp, bottom = 12.dp),
                        maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyLarge,
                        color = if (query.isEmpty()) colors.onSurfaceVariant else colors.onSurface)
                    if (query.isNotEmpty()) IconButton(onClick = { change(""); setMode(BrowseSearchMode.Editing) }) {
                        Icon(Icons.Outlined.Close, str(R.string.text_clear_search_3f4dec6))
                    } else if (mode == BrowseSearchMode.Browse) {
                        chrome.activity()
                        actions()
                    }
                }
            }
        }
        if (mode != BrowseSearchMode.Browse) {
            val label = str(R.string.text_cancel_998b9c4)
            val modifier = Modifier.focusRequester(cancelFocus).testTag("browse-cancel")
            if (LocalDensity.current.fontScale >= 1.5f) IconButton(onClick = ::dismiss, modifier = modifier) {
                Icon(Icons.Outlined.Close, label)
            } else TextButton(onClick = ::dismiss, modifier = modifier) { Text(label) }
        }
    }
}

/** Keep the scope controls reachable while browsing a long set of search results. */
internal fun LazyListScope.browseScopesItem(searching: Boolean, controls: @Composable () -> Unit) {
    if (searching) stickyHeader(key = "browse-scopes") {
        Box(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.page)) { controls() }
    } else item(key = "browse-scopes") { controls() }
}
