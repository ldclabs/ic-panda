package net.dmsg.android.ui

import android.graphics.Bitmap
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Article
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str
import net.dmsg.android.ui.kit.DetailCard
import net.dmsg.android.ui.kit.LabelValue
import net.dmsg.android.ui.kit.Notice
import net.dmsg.android.ui.kit.Progress
import net.dmsg.android.ui.kit.RowIcon
import java.io.File
import kotlinx.coroutines.flow.first

/**
 * Fullscreen file preview mounted as a root-level overlay inside [DMsgApp],
 * inheriting [RecentsPrivacy] protection and closing immediately on lock/conceal.
 */
@Composable internal fun FullscreenFilePreviewOverlay(
    client: ClientController,
    session: FullscreenPreviewSession,
    browser: (String) -> Unit,
    share: (File, String) -> Unit,
    snackbar: SnackbarHostState,
) {
    BackHandler { client.fullscreenPreview.close() }
    val isImagePage = session.isImagePage

    val filtered = session.filteredItems
    val pagerState = rememberPagerState(
        initialPage = session.activeIndex.coerceIn(0, maxOf(0, filtered.lastIndex)),
        pageCount = { maxOf(1, filtered.size) },
    )
    var showChrome by remember(session.activeId) { mutableStateOf(true) }
    var isZoomed by remember(session.activeId) { mutableStateOf(false) }
    var markdownSource by remember(session.activeId) { mutableStateOf(false) }
    var filterMenu by remember { mutableStateOf(false) }
    var actionsMenu by remember { mutableStateOf(false) }
    var confirmShare by remember { mutableStateOf(false) }
    var topBarHeightPx by remember { mutableIntStateOf(0) }
    val topBarHeight = with(LocalDensity.current) { topBarHeightPx.toDp() }

    LaunchedEffect(session.activeId, filtered) {
        if (filtered.isEmpty()) return@LaunchedEffect
        val target = session.activeIndex
        if (pagerState.settledPage != target) {
            pagerState.scrollToPage(target)
        }
        val next = snapshotFlow { pagerState.settledPage }.first { it != target }
        filtered.getOrNull(next)?.let { client.fullscreenPreview.select(it.id) }
    }

    fun openDetails(file: LibraryFile) {
        client.fullscreenPreview.close()
        client.navigation.open("files")
        val route = client.navigation.state.value.current
        if (route.page != "files" || route.id != file.id) {
            client.navigation.push(Route("files", id = file.id, folder = route.folder, revision = file.version))
        }
    }

    val backdrop = if (isImagePage) MaterialTheme.colorScheme.viewerBackdrop else MaterialTheme.colorScheme.page
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(backdrop)
            .testTag("fullscreen-preview"),
    ) {
        if (filtered.isNotEmpty()) {
            HorizontalPager(
                state = pagerState,
                userScrollEnabled = !isZoomed,
                modifier = Modifier.fillMaxSize(),
            ) { pageIndex ->
                val item = filtered.getOrNull(pageIndex) ?: return@HorizontalPager
                PreviewPageContent(
                    client = client,
                    session = session,
                    item = item,
                    markdownSource = markdownSource,
                    topBarHeight = topBarHeight,
                    browser = browser,
                    onToggleChrome = { showChrome = !showChrome },
                    onZoomChanged = { zoomed -> isZoomed = zoomed },
                    onShareRequest = { confirmShare = true },
                    onOpenDetails = ::openDetails,
                )
            }
        }

        AnimatedVisibility(
            visible = showChrome || !isImagePage,
            enter = fadeIn(Motion.effects()),
            exit = fadeOut(Motion.effectsFast()),
            modifier = Modifier.align(Alignment.TopCenter).fillMaxWidth(),
        ) {
            val barBg = if (isImagePage) MaterialTheme.colorScheme.viewerChrome else MaterialTheme.colorScheme.page
            val barFg = if (isImagePage) MaterialTheme.colorScheme.onViewerBackdrop else MaterialTheme.colorScheme.onSurface
            val barMuted = if (isImagePage) MaterialTheme.colorScheme.onViewerBackdropMuted else MaterialTheme.colorScheme.onSurfaceVariant
            Surface(
                color = barBg,
                contentColor = barFg,
                modifier = Modifier.fillMaxWidth().onSizeChanged { topBarHeightPx = it.height },
            ) {
                Column(Modifier.fillMaxWidth().statusBarsPadding()) {
                    Row(
                        modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 8.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        IconButton(
                            onClick = client.fullscreenPreview::close,
                            modifier = Modifier.testTag("preview-close"),
                        ) {
                            Icon(Icons.Outlined.Close, str(R.string.text_close_6cf4a77))
                        }

                        Column(Modifier.weight(1f)) {
                            val active = session.activeItem
                            Text(
                                text = active?.name.orEmpty(),
                                style = MaterialTheme.typography.titleSmall.copy(textDirection = TextDirection.ContentOrLtr),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            val subtitle = when {
                                filtered.size > 1 -> "${localizedNumber(session.activeIndex + 1)} / ${localizedNumber(filtered.size)}"
                                !active?.versionLabel.isNullOrEmpty() -> active.versionLabel.orEmpty()
                                else -> ""
                            }
                            if (subtitle.isNotEmpty()) {
                                CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
                                    Text(
                                        text = subtitle,
                                        style = MaterialTheme.typography.labelSmall.tabular(),
                                        color = barMuted,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                }
                            }
                        }

                        if (session.showFilter && session.allItems.size > 1) {
                            Box {
                                val counts = session.counts
                                if (session.filter == PreviewFilter.ALL) {
                                    IconButton(
                                        onClick = { filterMenu = true },
                                        modifier = Modifier.testTag("preview-filter"),
                                    ) {
                                        Icon(Icons.Outlined.FilterList, str(R.string.preview_filter))
                                    }
                                } else {
                                    FilledTonalButton(
                                        onClick = { filterMenu = true },
                                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp),
                                        modifier = Modifier.heightIn(min = 36.dp).testTag("preview-filter"),
                                    ) {
                                        Icon(Icons.Outlined.FilterList, null, Modifier.size(16.dp))
                                        Spacer(Modifier.width(4.dp))
                                        Text(
                                            text = "${filterLabel(session.filter)} · ${localizedNumber(counts[session.filter] ?: 0)}",
                                            style = MaterialTheme.typography.labelMedium.tabular(),
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                    }
                                }
                                DropdownMenu(expanded = filterMenu, onDismissRequest = { filterMenu = false }) {
                                    PreviewFilter.entries.forEach { option ->
                                        val count = counts[option] ?: 0
                                        DropdownMenuItem(
                                            text = {
                                                Text(
                                                    text = "${filterLabel(option)} (${localizedNumber(count)})",
                                                    style = MaterialTheme.typography.bodyMedium.tabular(),
                                                )
                                            },
                                            trailingIcon = if (session.filter == option) {
                                                { Icon(Icons.Outlined.Check, null) }
                                            } else null,
                                            enabled = count > 0,
                                            onClick = {
                                                filterMenu = false
                                                client.fullscreenPreview.setFilter(option)
                                            },
                                        )
                                    }
                                }
                            }
                        }

                        if (session.content is LoadedPreviewContent.Markdown) {
                            IconButton(
                                onClick = { markdownSource = !markdownSource },
                                modifier = Modifier.testTag("preview-markdown-mode"),
                            ) {
                                Icon(
                                    imageVector = if (markdownSource) Icons.AutoMirrored.Outlined.Article else Icons.Outlined.Code,
                                    contentDescription = str(if (markdownSource) R.string.preview_reading else R.string.preview_source),
                                )
                            }
                        }

                        session.activeItem?.libraryFile?.let { file ->
                            IconButton(
                                onClick = { openDetails(file) },
                                modifier = Modifier.testTag("preview-file-details"),
                            ) {
                                Icon(Icons.Outlined.Info, str(R.string.text_file_details_27e9099))
                            }
                        }

                        val isCloud = client.state.value.session?.accountKind != "local"
                        val activeLibraryFile = session.activeItem?.libraryFile
                        val canLinkCurrent = isCloud && activeLibraryFile != null &&
                            activeLibraryFile.size <= 100uL * 1024u * 1024u &&
                            (activeLibraryFile.local || activeLibraryFile.backup != null)
                        if (session.canShareCurrent || canLinkCurrent || session.activeItem?.source is PreviewSource.Channel) Box {
                            IconButton(
                                onClick = { actionsMenu = true },
                                modifier = Modifier.testTag("preview-actions"),
                            ) {
                                Icon(Icons.Outlined.MoreVert, str(R.string.message_actions))
                            }
                            DropdownMenu(expanded = actionsMenu, onDismissRequest = { actionsMenu = false }) {
                                when (val loaded = session.content) {
                                    is LoadedPreviewContent.Markdown -> DropdownMenuItem(
                                        text = { Text(str(R.string.text_copy_d6e9651)) },
                                        leadingIcon = { Icon(Icons.Outlined.ContentCopy, null) },
                                        onClick = {
                                            actionsMenu = false
                                            client.clipboard.copy(loaded.raw)
                                        },
                                    )
                                    is LoadedPreviewContent.Text -> DropdownMenuItem(
                                        text = { Text(str(R.string.text_copy_d6e9651)) },
                                        leadingIcon = { Icon(Icons.Outlined.ContentCopy, null) },
                                        onClick = {
                                            actionsMenu = false
                                            client.clipboard.copy(loaded.text)
                                        },
                                    )
                                    else -> Unit
                                }
                                session.activeItem?.takeIf { it.source is PreviewSource.Channel }?.let { item ->
                                    DropdownMenuItem(
                                        text = { Text(str(R.string.preview_save_to_files)) },
                                        leadingIcon = { Icon(Icons.Outlined.Download, null) },
                                        enabled = !session.savingToFiles,
                                        onClick = {
                                            actionsMenu = false
                                            client.fullscreenPreview.saveChannelAttachment(item)
                                        },
                                    )
                                }
                                if (activeLibraryFile != null) {
                                    if (activeLibraryFile.local) {
                                        DropdownMenuItem(
                                            text = { Text(str(R.string.nearby_share)) },
                                            leadingIcon = { Icon(Icons.Outlined.WifiTethering, null) },
                                            onClick = {
                                                actionsMenu = false
                                                client.fullscreenPreview.close()
                                                client.nearby.prepareSendFiles(
                                                    listOf(
                                                        NearbySelectedFile(
                                                            activeLibraryFile.id,
                                                            activeLibraryFile.version,
                                                            activeLibraryFile.name,
                                                            activeLibraryFile.size,
                                                        )
                                                    )
                                                )
                                                client.navigation.push(Route("nearby-send-files"))
                                            },
                                        )
                                    }
                                    if (canLinkCurrent) {
                                        DropdownMenuItem(
                                            text = { Text(str(R.string.text_share_by_link_05aafef)) },
                                            leadingIcon = { Icon(painterResource(R.drawable.glyph_link), null) },
                                            onClick = {
                                                actionsMenu = false
                                                client.fullscreenPreview.close()
                                                client.navigation.push(
                                                    Route("file-link-compose", id = activeLibraryFile.id, revision = activeLibraryFile.version)
                                                )
                                            },
                                        )
                                    }
                                }
                                if (session.canShareCurrent) {
                                    DropdownMenuItem(
                                        text = { Text(str(R.string.share_to_another_app)) },
                                        leadingIcon = { Icon(Icons.Outlined.Share, null) },
                                        onClick = {
                                            actionsMenu = false
                                            confirmShare = true
                                        },
                                    )
                                }
                            }
                        }
                    }
                    if (!isImagePage) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                }
            }
        }
        // The overlay covers the workspace's hosts, so notices and errors raised here show here.
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).navigationBarsPadding())
    }

    if (confirmShare) {
        AlertDialog(
            onDismissRequest = { confirmShare = false },
            title = { Text(str(R.string.share_to_another_app)) },
            text = { Text(str(R.string.text_shared_copies_leave_dmsg_s_protection_and_cannot_be_recalled_84ddf5e)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmShare = false
                    client.fullscreenPreview.shareCurrent(share)
                }) {
                    Text(str(R.string.share_to_another_app))
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmShare = false }) {
                    Text(str(R.string.text_cancel_998b9c4))
                }
            },
        )
    }
}

@Composable private fun PreviewPageContent(
    client: ClientController,
    session: FullscreenPreviewSession,
    item: FullscreenPreviewItem,
    markdownSource: Boolean,
    topBarHeight: Dp,
    browser: (String) -> Unit,
    onToggleChrome: () -> Unit,
    onZoomChanged: (Boolean) -> Unit,
    onShareRequest: () -> Unit,
    onOpenDetails: (LibraryFile) -> Unit,
) {
    val topBarInsetModifier = if (topBarHeight > 0.dp) {
        Modifier.padding(top = topBarHeight)
    } else {
        Modifier.statusBarsPadding().padding(top = 56.dp)
    }
    val colors = MaterialTheme.colorScheme
    val spinnerColor = if (session.isImagePage) colors.onViewerBackdrop else colors.primary
    if (item.id != session.activeId || session.loadedForId != item.id) {
        Box(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding(), contentAlignment = Alignment.Center) {
            CircularProgressIndicator(color = spinnerColor)
        }
        return
    }
    when (val loaded = session.content) {
        is LoadedPreviewContent.Loading -> {
            Box(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding(), contentAlignment = Alignment.Center) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier.widthIn(max = 260.dp),
                ) {
                    if (loaded.total > 0uL) {
                        Progress((loaded.completed.toDouble() / loaded.total.toDouble()).toFloat().coerceIn(0f, 1f))
                    } else {
                        CircularProgressIndicator(color = spinnerColor)
                    }
                    Text(str(R.string.vault_loading), style = MaterialTheme.typography.bodySmall,
                        color = if (session.isImagePage) colors.onViewerBackdropMuted else colors.onSurfaceVariant)
                }
            }
        }

        is LoadedPreviewContent.Image -> {
            ZoomableBitmapStage(
                bitmap = loaded.bitmap,
                contentDescription = loaded.name,
                onTap = onToggleChrome,
                onZoomChanged = onZoomChanged,
            )
        }

        is LoadedPreviewContent.Markdown -> {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .then(topBarInsetModifier)
                    .navigationBarsPadding()
                    .verticalScroll(rememberScrollState())
                    .padding(pagePadding()),
            ) {
                if (markdownSource) {
                    SelectionContainer {
                        Text(loaded.raw, style = MaterialTheme.typography.bodySmall.code())
                    }
                } else {
                    MarkdownDocumentView(loaded.document, client, browser)
                }
            }
        }

        is LoadedPreviewContent.Text -> {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .then(topBarInsetModifier)
                    .navigationBarsPadding()
                    .verticalScroll(rememberScrollState())
                    .padding(pagePadding()),
            ) {
                SelectionContainer {
                    Text(loaded.text, style = MaterialTheme.typography.bodySmall.code())
                }
            }
        }

        is LoadedPreviewContent.Fallback -> {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .then(topBarInsetModifier)
                    .navigationBarsPadding()
                    .verticalScroll(rememberScrollState())
                    .padding(pagePadding()),
                contentAlignment = Alignment.TopCenter,
            ) {
                FallbackMetadataCard(
                    item = item,
                    reason = loaded.reason,
                    mime = loaded.mime,
                    name = loaded.name.ifBlank { item.name },
                    size = loaded.size,
                    sha256 = loaded.sha256 ?: item.sha256,
                    savingToFiles = session.savingToFiles,
                    canShare = session.canShareCurrent,
                    onSaveToFiles = { client.fullscreenPreview.saveChannelAttachment(item) },
                    onShare = onShareRequest,
                    onDetails = { item.libraryFile?.let(onOpenDetails) },
                )
            }
        }

        is LoadedPreviewContent.Failed -> {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .navigationBarsPadding()
                    .padding(pagePadding()),
                contentAlignment = Alignment.Center,
            ) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier.widthIn(max = 360.dp),
                ) {
                    Notice(errorText(loaded.code), true)
                    Button(onClick = client.fullscreenPreview::retry) {
                        Text(str(R.string.text_retry_563171c))
                    }
                }
            }
        }
    }
}

@Composable private fun ZoomableBitmapStage(
    bitmap: Bitmap,
    contentDescription: String,
    onTap: () -> Unit,
    onZoomChanged: (Boolean) -> Unit,
) {
    var scale by remember(bitmap) { mutableFloatStateOf(1f) }
    var offset by remember(bitmap) { mutableStateOf(Offset.Zero) }
    var stageSize by remember { mutableStateOf(IntSize.Zero) }

    LaunchedEffect(scale) { onZoomChanged(scale > 1.01f) }

    fun clampOffset(raw: Offset, targetScale: Float): Offset {
        if (targetScale <= 1.01f || stageSize.width == 0 || stageSize.height == 0) return Offset.Zero
        val maxX = stageSize.width * (targetScale - 1f) / 2f
        val maxY = stageSize.height * (targetScale - 1f) / 2f
        return Offset(raw.x.coerceIn(-maxX, maxX), raw.y.coerceIn(-maxY, maxY))
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .onSizeChanged { stageSize = it }
            .pointerInput(bitmap) {
                detectTapGestures(
                    onTap = { onTap() },
                    onDoubleTap = {
                        if (scale > 1.05f) {
                            scale = 1f
                            offset = Offset.Zero
                        } else {
                            scale = 2.5f
                            offset = Offset.Zero
                        }
                    },
                )
            }
            .pointerInput(bitmap) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    do {
                        val event = awaitPointerEvent()
                        val pointers = event.changes.count { it.pressed }
                        if (pointers >= 2 || scale > 1.01f) {
                            val zoomChange = event.calculateZoom()
                            val panChange = event.calculatePan()
                            val nextScale = (scale * zoomChange).coerceIn(1f, 5f)
                            scale = nextScale
                            offset = clampOffset(offset + panChange, nextScale)
                            event.changes.forEach { change ->
                                if (change.positionChanged()) change.consume()
                            }
                        }
                    } while (event.changes.any { it.pressed })
                    if (scale <= 1.01f) {
                        scale = 1f
                        offset = Offset.Zero
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Image(
            bitmap = bitmap.asImageBitmap(),
            contentDescription = contentDescription,
            contentScale = ContentScale.Fit,
            modifier = Modifier
                .fillMaxSize()
                .graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                    translationX = offset.x
                    translationY = offset.y
                },
        )
    }
}

@Composable private fun FallbackMetadataCard(
    item: FullscreenPreviewItem,
    reason: PreviewFallbackReason,
    mime: String,
    name: String,
    size: ULong,
    sha256: String?,
    savingToFiles: Boolean,
    canShare: Boolean,
    onSaveToFiles: () -> Unit,
    onShare: () -> Unit,
    onDetails: () -> Unit,
) {
    DetailCard(Modifier.widthIn(max = 440.dp).testTag("preview-fallback")) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.Top,
        ) {
            RowIcon(painterResource(if (mime.startsWith("image/")) R.drawable.glyph_image else R.drawable.glyph_file), MaterialTheme.colorScheme.secondaryContainer)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(name, style = MaterialTheme.typography.titleMedium.copy(textDirection = TextDirection.ContentOrLtr))
                Text(fallbackReasonMessage(reason), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        HorizontalDivider(Modifier.padding(vertical = 8.dp), color = MaterialTheme.colorScheme.outlineVariant)
        LabelValue(str(R.string.text_type_6172196), mime.ifBlank { "—" }, code = true)
        LabelValue(str(R.string.preview_size), fileSize(size))
        item.updatedAt?.takeIf { it > 0uL }?.let { updated ->
            LabelValue(str(R.string.preview_modified), messageTime(updated))
        }
        sha256?.takeIf { it.isNotBlank() }?.let { digest ->
            LabelValue("SHA-256", digest.take(16) + "…", code = true)
        }
        Spacer(Modifier.height(8.dp))
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            when (item.source) {
                is PreviewSource.Channel -> {
                    Button(
                        onClick = onSaveToFiles,
                        enabled = !savingToFiles,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(Icons.Outlined.Download, null)
                        Spacer(Modifier.width(8.dp))
                        Text(str(R.string.preview_save_to_files))
                    }
                }
                is PreviewSource.Library -> {
                    if (canShare) {
                        Button(onClick = onShare, modifier = Modifier.fillMaxWidth()) {
                            Icon(Icons.Outlined.Share, null)
                            Spacer(Modifier.width(8.dp))
                            Text(str(R.string.share_to_another_app))
                        }
                    }
                    if (item.libraryFile != null) {
                        OutlinedButton(onClick = onDetails, modifier = Modifier.fillMaxWidth()) {
                            Icon(Icons.Outlined.Info, null)
                            Spacer(Modifier.width(8.dp))
                            Text(str(R.string.text_file_details_27e9099))
                        }
                    }
                }
            }
        }
    }
}

private fun filterLabel(filter: PreviewFilter): String = when (filter) {
    PreviewFilter.ALL -> str(R.string.text_all_eae0ae1)
    PreviewFilter.PREVIEWABLE -> str(R.string.preview_filter_previewable)
    PreviewFilter.IMAGES -> str(R.string.preview_filter_images)
    PreviewFilter.DOCUMENTS -> str(R.string.preview_filter_documents)
}

private fun fallbackReasonMessage(reason: PreviewFallbackReason): String = when (reason) {
    PreviewFallbackReason.UNSUPPORTED_TYPE -> str(R.string.preview_unsupported)
    PreviewFallbackReason.TOO_LARGE -> str(R.string.preview_too_large)
    PreviewFallbackReason.CLOUD_ONLY -> str(R.string.preview_cloud_only)
    PreviewFallbackReason.DECODE_FAILED -> str(R.string.preview_decode_failed)
    PreviewFallbackReason.UNAVAILABLE -> str(R.string.preview_unavailable)
}
