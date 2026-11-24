package net.dmsg.android.core

import android.graphics.Bitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import net.dmsg.android.platform.BoundedImages
import net.dmsg.android.platform.PrivateFiles
import net.dmsg.core.MessageItem
import java.io.File

sealed interface LoadedPreviewContent {
    val originalBytes: ByteArray? get() = null

    data class Loading(val completed: ULong = 0uL, val total: ULong = 0uL) : LoadedPreviewContent
    data class Image(
        val bitmap: Bitmap,
        override val originalBytes: ByteArray,
        val mime: String,
        val name: String,
        val size: ULong,
        val sha256: String?,
    ) : LoadedPreviewContent
    data class Markdown(
        val document: MarkdownDocument,
        val raw: String,
        override val originalBytes: ByteArray?,
        val mime: String,
        val name: String,
        val size: ULong,
        val sha256: String?,
    ) : LoadedPreviewContent
    data class Text(
        val text: String,
        override val originalBytes: ByteArray?,
        val mime: String,
        val name: String,
        val size: ULong,
        val sha256: String?,
    ) : LoadedPreviewContent
    data class Fallback(
        val reason: PreviewFallbackReason,
        val mime: String,
        val name: String,
        val size: ULong,
        val sha256: String?,
    ) : LoadedPreviewContent
    data class Failed(val code: String) : LoadedPreviewContent
}

data class FullscreenPreviewSession(
    val allItems: List<FullscreenPreviewItem>,
    val filter: PreviewFilter = PreviewFilter.ALL,
    val activeId: String,
    val showFilter: Boolean = true,
    val channelId: String? = null,
    val loadedForId: String? = null,
    val content: LoadedPreviewContent = LoadedPreviewContent.Loading(),
    val savingToFiles: Boolean = false,
) {
    val counts: Map<PreviewFilter, Int> get() = PreviewFilterLogic.counts(allItems)
    val filteredItems: List<FullscreenPreviewItem> get() = allItems.filter { PreviewFilterLogic.matches(it, filter) }
    val activeIndex: Int get() = filteredItems.indexOfFirst { it.id == activeId }.takeIf { it >= 0 } ?: 0
    val activeItem: FullscreenPreviewItem? get() = filteredItems.getOrNull(activeIndex)
    val isImagePage: Boolean get() = content is LoadedPreviewContent.Image
    val canShareCurrent: Boolean
        get() {
            val item = activeItem ?: return false
            return when (item.source) {
                is PreviewSource.Library -> item.canOpen && !(content is LoadedPreviewContent.Fallback &&
                    (content.reason == PreviewFallbackReason.CLOUD_ONLY || content.reason == PreviewFallbackReason.UNAVAILABLE))
                is PreviewSource.Channel -> content is LoadedPreviewContent.Image ||
                    content is LoadedPreviewContent.Markdown ||
                    content is LoadedPreviewContent.Text
            }
        }
}

/** Coordinates fullscreen preview across Files and Messages without writing plaintext temp files. */
class FullscreenPreviewCoordinator(
    private val client: ClientController,
    private val paths: () -> PrivateFiles?,
) {
    private val mutable = MutableStateFlow<FullscreenPreviewSession?>(null)
    val session = mutable.asStateFlow()
    private var loadSequence = 0L

    fun openLibrary(
        files: List<LibraryFile>,
        initialId: String,
        version: String? = null,
        versionLabel: String? = null,
        showFilter: Boolean = true,
    ) {
        val items = files.map { file ->
            if (file.id == initialId && version != null) FullscreenPreviewItem.fromLibrary(file, version, versionLabel)
            else FullscreenPreviewItem.fromLibrary(file)
        }
        val targetId = if (version != null) "$initialId@$version" else initialId
        if (items.none { it.id == targetId }) return
        openSession(FullscreenPreviewSession(
            allItems = items,
            filter = PreviewFilter.ALL,
            activeId = targetId,
            showFilter = showFilter && items.size > 1,
            channelId = null,
        ))
    }

    fun openChannel(channel: String, messages: List<MessageItem>, initialSequence: ULong) {
        val items = messages.mapNotNull { FullscreenPreviewItem.fromMessage(channel, it) }
        val targetId = "channel:$channel:$initialSequence"
        if (items.none { it.id == targetId }) return
        openSession(FullscreenPreviewSession(
            allItems = items,
            filter = PreviewFilter.ALL,
            activeId = targetId,
            showFilter = true,
            channelId = channel,
        ))
        loadAllChannelAttachments(channel)
    }

    private fun openSession(next: FullscreenPreviewSession) {
        client.activity()
        zeroLoadedBytes()
        client.cancelOperation("fullscreen:preview")
        client.cancelOperation("fullscreen:attachments")
        mutable.value = next
        loadCurrent()
    }

    fun select(id: String) {
        val current = mutable.value ?: return
        if (current.activeId == id && current.loadedForId == id) return
        if (current.filteredItems.none { it.id == id }) return
        client.activity()
        zeroLoadedBytes()
        mutable.value = current.copy(activeId = id, loadedForId = id, content = LoadedPreviewContent.Loading())
        loadCurrent()
    }

    fun setFilter(next: PreviewFilter) {
        val current = mutable.value ?: return
        if (current.filter == next) return
        if ((current.counts[next] ?: 0) == 0) return
        val picked = PreviewFilterLogic.pickNearest(current.allItems, current.activeId, next)
        val nextId = picked.activeId ?: return
        val changed = nextId != current.activeId
        if (changed) zeroLoadedBytes()
        mutable.value = current.copy(
            filter = next,
            activeId = nextId,
            loadedForId = if (changed) nextId else current.loadedForId,
            content = if (changed) LoadedPreviewContent.Loading() else current.content,
        )
        if (changed) loadCurrent()
    }

    fun retry() {
        loadCurrent()
    }

    fun saveChannelAttachment(item: FullscreenPreviewItem) {
        val source = item.source as? PreviewSource.Channel ?: return
        val current = mutable.value ?: return
        if (current.savingToFiles) return
        mutable.value = current.copy(savingToFiles = true)
        client.perform({ core ->
            client.operation { token ->
                core.saveAttachment(source.channel, source.sequence, token)
            }
        }, key = "fullscreen:save:${source.channel}:${source.sequence}", onError = {
            mutable.value = mutable.value?.copy(savingToFiles = false)
        }) {
            mutable.value = mutable.value?.copy(savingToFiles = false)
            client.library.browse()
            client.messageAttachments.invalidate()
            client.notice("FILE_SAVED_LOCALLY")
        }
    }

    fun shareCurrent(open: (File, String) -> Unit) {
        val current = mutable.value ?: return
        val item = current.activeItem ?: return
        val selection = loadSequence
        fun showCopy(file: File, mime: String) {
            if (selection == loadSequence) open(file, mime) else file.parentFile?.deleteRecursively()
        }
        when (item.source) {
            is PreviewSource.Library -> {
                val file = item.libraryFile ?: return
                client.transfers.share(file, ::showCopy)
            }
            is PreviewSource.Channel -> {
                val files = paths() ?: return
                val stamp = client.currentStamp() ?: return
                val owner = stamp.account ?: return
                val pair: Pair<ByteArray, String> = when (val loaded = current.content) {
                    is LoadedPreviewContent.Image -> loaded.originalBytes.copyOf() to loaded.mime
                    is LoadedPreviewContent.Markdown -> (loaded.originalBytes?.copyOf() ?: loaded.raw.toByteArray(Charsets.UTF_8)) to loaded.mime
                    is LoadedPreviewContent.Text -> (loaded.originalBytes?.copyOf() ?: loaded.text.toByteArray(Charsets.UTF_8)) to loaded.mime
                    else -> return
                }
                client.perform({
                    try {
                        files.shareBytes(item.name, pair.first, owner) { client.sessionCurrent(stamp) }
                    } finally {
                        pair.first.fill(0)
                    }
                }, key = "fullscreen:share:${item.id}", replace = true) { sharedFile ->
                    showCopy(sharedFile, pair.second)
                }
            }
        }
    }

    fun close() = clear()

    fun clear() {
        loadSequence += 1
        client.cancelOperation("fullscreen:preview")
        client.cancelOperation("fullscreen:attachments")
        zeroLoadedBytes()
        mutable.value = null
    }

    private fun zeroLoadedBytes() {
        mutable.value?.content?.originalBytes?.fill(0)
    }

    private fun loadCurrent() {
        val current = mutable.value ?: return
        val item = current.activeItem ?: return
        val targetId = item.id
        val seq = ++loadSequence
        val stamp = client.currentStamp() ?: return
        mutable.value = current.copy(loadedForId = targetId, content = LoadedPreviewContent.Loading())
        client.perform({ core ->
            val payload = client.operation { token ->
                coroutineScope {
                    val progressJob = if (item.source is PreviewSource.Channel) launch {
                        while (isActive) {
                            val progress = token.progress()
                            if (progress.total > 0uL) {
                                withContext(Dispatchers.Main.immediate) {
                                    val live = mutable.value
                                    if (client.accepts(stamp) && loadSequence == seq && live?.activeId == targetId && live.content is LoadedPreviewContent.Loading) {
                                        mutable.value = live.copy(content = LoadedPreviewContent.Loading(progress.completed, progress.total))
                                    }
                                }
                            }
                            delay(200)
                        }
                    } else null
                    try {
                        when (val source = item.source) {
                            is PreviewSource.Library -> core.previewLibraryFile(source.id, source.version, token)
                            is PreviewSource.Channel -> core.previewChannelAttachment(source.channel, source.sequence, token)
                        }
                    } finally {
                        progressJob?.cancel()
                    }
                }
            }
            if (payload.fallbackReason != null) {
                return@perform LoadedPreviewContent.Fallback(
                    reason = PreviewFallbackReason.fromWire(payload.fallbackReason),
                    mime = payload.mime,
                    name = payload.name,
                    size = payload.size,
                    sha256 = payload.sha256,
                )
            }
            when (payload.kind) {
                "image" -> {
                    val bytes = payload.imageBytes
                    val bitmap = bytes?.let { BoundedImages.fullscreen(it) }
                    if (bytes == null || bitmap == null) {
                        bytes?.fill(0)
                        LoadedPreviewContent.Fallback(
                            reason = PreviewFallbackReason.DECODE_FAILED,
                            mime = payload.mime,
                            name = payload.name,
                            size = payload.size,
                            sha256 = payload.sha256,
                        )
                    } else {
                        LoadedPreviewContent.Image(
                            bitmap = bitmap,
                            originalBytes = bytes,
                            mime = payload.mime,
                            name = payload.name,
                            size = payload.size,
                            sha256 = payload.sha256,
                        )
                    }
                }
                "markdown" -> {
                    val doc = payload.markdownJson?.let(::parseMarkdownDocument)
                    if (doc == null) {
                        LoadedPreviewContent.Fallback(
                            reason = PreviewFallbackReason.DECODE_FAILED,
                            mime = payload.mime,
                            name = payload.name,
                            size = payload.size,
                            sha256 = payload.sha256,
                        )
                    } else {
                        LoadedPreviewContent.Markdown(
                            document = doc,
                            raw = payload.text.orEmpty(),
                            originalBytes = payload.originalTextBytes,
                            mime = payload.mime,
                            name = payload.name,
                            size = payload.size,
                            sha256 = payload.sha256,
                        )
                    }
                }
                "text" -> LoadedPreviewContent.Text(
                    text = payload.text.orEmpty(),
                    originalBytes = payload.originalTextBytes,
                    mime = payload.mime,
                    name = payload.name,
                    size = payload.size,
                    sha256 = payload.sha256,
                )
                else -> LoadedPreviewContent.Fallback(
                    reason = PreviewFallbackReason.UNSUPPORTED_TYPE,
                    mime = payload.mime,
                    name = payload.name,
                    size = payload.size,
                    sha256 = payload.sha256,
                )
            }
        }, key = "fullscreen:preview", replace = true, quiet = true, onError = { code ->
            val live = mutable.value
            if (loadSequence == seq && live?.activeId == targetId) {
                mutable.value = live.copy(loadedForId = targetId, content = LoadedPreviewContent.Failed(code))
            }
        }) { loaded ->
            val live = mutable.value
            if (loadSequence == seq && live?.activeId == targetId) {
                mutable.value = live.copy(loadedForId = targetId, content = loaded)
            } else {
                loaded.originalBytes?.fill(0)
            }
        }
    }

    private fun loadAllChannelAttachments(channel: String) {
        client.perform({ core ->
            parseChannelAttachments(core.channelAttachments(channel).json)
        }, key = "fullscreen:attachments", replace = true, quiet = true) { records ->
            val live = mutable.value ?: return@perform
            if (live.channelId != channel) return@perform
            val items = records.map { FullscreenPreviewItem.fromAttachment(channel, it) }
            if (items.isEmpty()) { close(); return@perform }
            val filter = live.filter.takeIf { f -> items.any { PreviewFilterLogic.matches(it, f) } } ?: PreviewFilter.ALL
            val selected = PreviewFilterLogic.pickNearest(items, live.activeId, filter).activeId ?: return@perform
            if (selected != live.activeId) zeroLoadedBytes()
            mutable.value = live.copy(allItems = items, activeId = selected, filter = filter)
            if (selected != live.activeId) loadCurrent()
        }
    }
}
