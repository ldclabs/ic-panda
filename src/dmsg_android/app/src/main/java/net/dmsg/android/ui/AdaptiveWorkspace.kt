package net.dmsg.android.ui

import android.app.Activity
import android.content.ContextWrapper
import android.graphics.Rect
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowInfoTracker

internal data class WorkspaceFold(val bounds: Rect, val vertical: Boolean)

@Composable internal fun rememberWorkspaceFold(): WorkspaceFold? {
    val context = LocalContext.current
    val activity = remember(context) {
        var owner = context
        while (owner is ContextWrapper && owner !is Activity) owner = owner.baseContext
        owner as? Activity
    }
    val fold by produceState<WorkspaceFold?>(null, activity) {
        activity?.let { owner -> WindowInfoTracker.getOrCreate(context).windowLayoutInfo(owner).collect { layout ->
            value = layout.displayFeatures.filterIsInstance<FoldingFeature>().firstOrNull { it.isSeparating || it.occlusionType == FoldingFeature.OcclusionType.FULL }
                ?.let { WorkspaceFold(Rect(it.bounds), it.orientation == FoldingFeature.Orientation.VERTICAL) }
        } }
    }
    return fold
}

/** Large text and tabletop mode use the larger unobstructed part of the window. */
@Composable internal fun FoldSafeWorkspace(fold: WorkspaceFold?, dualPane: Boolean, content: @Composable () -> Unit) {
    var origin by remember { mutableStateOf(Offset.Zero) }
    val density = LocalDensity.current
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    BoxWithConstraints(Modifier.fillMaxSize().onGloballyPositioned { origin = it.positionInWindow() }) {
        val inset = if (fold == null || dualPane && fold.vertical) Modifier else with(density) {
            val width = maxWidth.toPx(); val height = maxHeight.toPx()
            if (fold.vertical) {
                val left = (fold.bounds.left - origin.x).coerceIn(0f, width)
                val right = (fold.bounds.right - origin.x).coerceIn(0f, width)
                if (left <= 0 || right >= width) Modifier
                else if (left > width - right || left == width - right && !rtl) Modifier.absolutePadding(right = (width - left).toDp())
                else Modifier.absolutePadding(left = right.toDp())
            } else {
                val top = (fold.bounds.top - origin.y).coerceIn(0f, height)
                val bottom = (fold.bounds.bottom - origin.y).coerceIn(0f, height)
                if (top <= 0 || bottom >= height) Modifier
                else if (top >= height - bottom) Modifier.padding(bottom = (height - top).toDp())
                else Modifier.padding(top = bottom.toDp())
            }
        }
        Box(Modifier.fillMaxSize().then(inset)) { content() }
    }
}

/** Both panes share the same hinge-aware layout, including the account index. */
@Composable internal fun WorkspacePanes(fold: WorkspaceFold?, list: @Composable () -> Unit, detail: @Composable () -> Unit) {
    var origin by remember { mutableStateOf(Offset.Zero) }
    val density = LocalDensity.current
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    BoxWithConstraints(Modifier.fillMaxSize().onGloballyPositioned { origin = it.positionInWindow() }.testTag("workspace-panes")) {
        val split = fold?.takeIf { it.vertical }?.let { feature -> with(density) {
            val left = (feature.bounds.left - origin.x).toDp()
            val right = (feature.bounds.right - origin.x).toDp()
            if (left >= 240.dp && maxWidth - right >= 240.dp) (if (rtl) maxWidth - right else left) to (right - left).coerceAtLeast(1.dp) else null
        } }
        val listWidth = split?.first ?: (maxWidth * .4f).coerceIn(280.dp, 420.dp)
        Row(Modifier.fillMaxSize()) {
            Box(Modifier.width(listWidth).fillMaxHeight().testTag("workspace-list")) { list() }
            if (split == null) VerticalDivider() else Spacer(Modifier.width(split.second).fillMaxHeight().testTag("workspace-hinge"))
            Box(Modifier.weight(1f).fillMaxHeight().testTag("workspace-detail")) { detail() }
        }
    }
}

/** The detail pane of a wide window has its own compact bar; the page in it declares the title and actions with [ScreenBar]. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable internal fun DetailPane(content: @Composable () -> Unit) {
    val bar = remember { PageBar() }
    Column(Modifier.fillMaxSize()) {
        TopAppBar(title = { Text(bar.title.orEmpty(), maxLines = 1, overflow = TextOverflow.Ellipsis) }, actions = { bar.actions?.invoke(this) },
            windowInsets = WindowInsets(0), colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.page))
        CompositionLocalProvider(LocalPageBar provides bar) { Box(Modifier.weight(1f).fillMaxWidth()) { content() } }
    }
}
