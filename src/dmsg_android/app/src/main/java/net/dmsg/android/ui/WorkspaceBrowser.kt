package net.dmsg.android.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.platform.str

internal val LocalBrowseNavigation = staticCompositionLocalOf<@Composable () -> Unit> { {} }

internal enum class BrowseSearchMode { Browse, Editing, Results }

@OptIn(ExperimentalMaterial3Api::class)
@Composable internal fun WorkspaceBrowser(
    title: String, query: String, change: (String) -> Unit, prompt: String,
    actions: @Composable RowScope.() -> Unit, content: @Composable (searching: Boolean) -> Unit,
) {
    var mode by remember { mutableStateOf(if (query.isEmpty()) BrowseSearchMode.Browse else BrowseSearchMode.Results) }
    val browsing = mode == BrowseSearchMode.Browse
    val bar = TopAppBarDefaults.enterAlwaysScrollBehavior(canScroll = { browsing })

    Column(Modifier.fillMaxSize().semantics { paneTitle = title }.nestedScroll(bar.nestedScrollConnection)) {
        if (browsing) TopAppBar(
            title = {}, navigationIcon = LocalBrowseNavigation.current, actions = actions,
            expandedHeight = 48.dp, windowInsets = WindowInsets(0),
            colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent, scrolledContainerColor = Color.Transparent),
            scrollBehavior = bar,
        )
        BrowseSearchRow(query, change, prompt, mode) { next ->
            if (next == BrowseSearchMode.Browse) {
                bar.state.heightOffset = 0f
                bar.state.contentOffset = 0f
            }
            mode = next
        }
        content(!browsing)
    }
}

/** Only editing owns an input connection. Returning to results never reopens the IME. */
@Composable internal fun BrowseSearchRow(query: String, change: (String) -> Unit, prompt: String,
    mode: BrowseSearchMode, setMode: (BrowseSearchMode) -> Unit) {
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
    val editor = remember { FocusRequester() }
    val cancelFocus = remember { FocusRequester() }
    fun dismiss() {
        if (query.isNotEmpty()) change("")
        setMode(BrowseSearchMode.Browse)
        focus.clearFocus()
        keyboard?.hide()
    }
    BackHandler(enabled = mode != BrowseSearchMode.Browse && !imeVisible, onBack = ::dismiss)
    Row(Modifier.fillMaxWidth().padding(horizontal = pageMargin(), vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        if (mode == BrowseSearchMode.Editing) {
            WorkspaceSearchField(query, change, prompt, Modifier.weight(1f).focusRequester(editor).testTag("browse-search-editor"), onSearch = {
                cancelFocus.requestFocus()
                setMode(BrowseSearchMode.Results)
                keyboard?.hide()
            })
            LaunchedEffect(Unit) {
                editor.requestFocus()
                keyboard?.show()
            }
        } else {
            val colors = MaterialTheme.colorScheme
            Surface(onClick = { setMode(BrowseSearchMode.Editing) },
                modifier = Modifier.weight(1f).testTag("browse-search-entry").semantics { contentDescription = prompt },
                shape = CircleShape, color = colors.card, border = BorderStroke(1.dp, colors.outlineVariant)) {
                Row(Modifier.heightIn(min = 56.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.width(48.dp), contentAlignment = Alignment.Center) { Icon(Icons.Outlined.Search, null, tint = colors.onSurfaceVariant) }
                    val labelModifier = if (query.isEmpty()) Modifier.clearAndSetSemantics {} else Modifier
                    Text(query.ifEmpty { prompt }, Modifier.weight(1f).then(labelModifier).padding(start = 4.dp, end = 8.dp, top = 12.dp, bottom = 12.dp),
                        maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyLarge,
                        color = if (query.isEmpty()) colors.onSurfaceVariant else colors.onSurface)
                    if (query.isNotEmpty()) IconButton(onClick = { change(""); setMode(BrowseSearchMode.Editing) }) {
                        Icon(Icons.Outlined.Close, str(R.string.text_clear_search_3f4dec6))
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
