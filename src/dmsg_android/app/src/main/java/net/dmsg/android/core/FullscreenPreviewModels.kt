package net.dmsg.android.core

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import net.dmsg.core.MessageItem

/** Preview category mirroring `dmsg_client::preview::PreviewClass`. */
enum class PreviewClass {
    IMAGE,
    MARKDOWN,
    TEXT,
    UNSUPPORTED;

    companion object {
        fun fromWire(value: String?): PreviewClass = when (value) {
            "image" -> IMAGE
            "markdown" -> MARKDOWN
            "text" -> TEXT
            else -> UNSUPPORTED
        }
    }
}

/** Filter applied to the active fullscreen preview queue. */
enum class PreviewFilter {
    ALL,
    PREVIEWABLE,
    IMAGES,
    DOCUMENTS,
}

/** Reason a file rendered its metadata fallback card instead of content. */
enum class PreviewFallbackReason {
    UNSUPPORTED_TYPE,
    TOO_LARGE,
    CLOUD_ONLY,
    DECODE_FAILED,
    UNAVAILABLE;

    companion object {
        fun fromWire(value: String?): PreviewFallbackReason = when (value) {
            "too_large" -> TOO_LARGE
            "cloud_only" -> CLOUD_ONLY
            "decode_failed" -> DECODE_FAILED
            "unavailable" -> UNAVAILABLE
            else -> UNSUPPORTED_TYPE
        }
    }
}

/** Lightweight channel attachment entry returned by `MobileEngine.channelAttachments`. */
data class ChannelAttachmentRecord(
    val seq: ULong,
    val digest: String,
    val account: String,
    val createdAt: ULong,
    val storedAt: ULong,
    val name: String,
    val mime: String,
    val size: ULong,
    val previewClass: PreviewClass,
    val previewable: Boolean,
)

fun parseChannelAttachments(json: String): List<ChannelAttachmentRecord> =
    Json.parseToJsonElement(json).arrayRows().map { row ->
        ChannelAttachmentRecord(
            seq = row.requiredUnsigned("seq"),
            digest = row.required("digest"),
            account = row.required("account"),
            createdAt = row.requiredUnsigned("created_at"),
            storedAt = row.requiredUnsigned("stored_at"),
            name = row.required("name"),
            mime = row.required("mime"),
            size = row.requiredUnsigned("size"),
            previewClass = PreviewClass.fromWire(row.text("preview_class")),
            previewable = row.requiredFlag("previewable"),
        )
    }

// Safe Markdown AST produced by Rust `dmsg_client::preview::parse_markdown`.

enum class MarkdownTableAlignment {
    NONE,
    LEFT,
    CENTER,
    RIGHT;

    companion object {
        fun fromWire(value: String?): MarkdownTableAlignment = when (value) {
            "left" -> LEFT
            "center" -> CENTER
            "right" -> RIGHT
            else -> NONE
        }
    }
}

data class MarkdownDocument(
    val blocks: List<MarkdownBlock>,
    val truncated: Boolean = false,
)

data class MarkdownListItem(
    val checked: Boolean?,
    val blocks: List<MarkdownBlock>,
)

sealed interface MarkdownBlock {
    data class Heading(val level: Int, val inlines: List<MarkdownInline>) : MarkdownBlock
    data class Paragraph(val inlines: List<MarkdownInline>) : MarkdownBlock
    data class CodeBlock(val language: String?, val code: String) : MarkdownBlock
    data class BlockQuote(val blocks: List<MarkdownBlock>) : MarkdownBlock
    data class ListBlock(val ordered: Boolean, val start: ULong, val items: List<MarkdownListItem>) : MarkdownBlock
    data class Table(
        val alignments: List<MarkdownTableAlignment>,
        val headers: List<List<MarkdownInline>>,
        val rows: List<List<List<MarkdownInline>>>,
    ) : MarkdownBlock
    data object ThematicBreak : MarkdownBlock
}

sealed interface MarkdownInline {
    data class Text(val text: String) : MarkdownInline
    data class Strong(val children: List<MarkdownInline>) : MarkdownInline
    data class Emphasis(val children: List<MarkdownInline>) : MarkdownInline
    data class Strikethrough(val children: List<MarkdownInline>) : MarkdownInline
    data class Code(val code: String) : MarkdownInline
    data class Link(val url: String, val children: List<MarkdownInline>) : MarkdownInline
    data class BlockedImage(val alt: String) : MarkdownInline
    data object SoftBreak : MarkdownInline
    data object HardBreak : MarkdownInline
}

fun parseMarkdownDocument(json: String): MarkdownDocument? = runCatching {
    val root = Json.parseToJsonElement(json).objectValue()
    val blocks = parseMarkdownBlocks(root["blocks"] as? JsonArray ?: return null)
    MarkdownDocument(blocks = blocks, truncated = root.flag("truncated", default = false))
}.getOrNull()

private fun parseMarkdownBlocks(array: JsonArray): List<MarkdownBlock> = array.map { elem ->
    val obj = elem.objectValue()
    when (obj.required("type")) {
        "heading" -> MarkdownBlock.Heading(
            level = obj.requiredUnsigned("level").toInt().coerceIn(1, 6),
            inlines = parseMarkdownInlines(obj["inlines"] as? JsonArray ?: JsonArray(emptyList())),
        )
        "paragraph" -> MarkdownBlock.Paragraph(
            inlines = parseMarkdownInlines(obj["inlines"] as? JsonArray ?: JsonArray(emptyList())),
        )
        "code_block" -> MarkdownBlock.CodeBlock(
            language = obj.text("language"),
            code = obj.required("code"),
        )
        "block_quote" -> MarkdownBlock.BlockQuote(
            blocks = parseMarkdownBlocks(obj["blocks"] as? JsonArray ?: JsonArray(emptyList())),
        )
        "list" -> {
            val items = (obj["items"] as? JsonArray ?: JsonArray(emptyList())).map { itemElem ->
                val itemObj = itemElem.objectValue()
                val checked = when (val value = itemObj["checked"]) {
                    null, JsonNull -> null
                    is JsonPrimitive -> value.booleanOrNull
                    else -> null
                }
                MarkdownListItem(
                    checked = checked,
                    blocks = parseMarkdownBlocks(itemObj["blocks"] as? JsonArray ?: JsonArray(emptyList())),
                )
            }
            MarkdownBlock.ListBlock(
                ordered = obj.flag("ordered", default = false),
                start = obj.unsigned("start") ?: 1uL,
                items = items,
            )
        }
        "table" -> {
            val alignments = (obj["alignments"] as? JsonArray ?: JsonArray(emptyList())).map {
                MarkdownTableAlignment.fromWire((it as? JsonPrimitive)?.content)
            }
            val headers = (obj["headers"] as? JsonArray ?: JsonArray(emptyList())).map {
                parseMarkdownInlines(it as? JsonArray ?: JsonArray(emptyList()))
            }
            val rows = (obj["rows"] as? JsonArray ?: JsonArray(emptyList())).map { rowElem ->
                (rowElem as? JsonArray ?: JsonArray(emptyList())).map { cellElem ->
                    parseMarkdownInlines(cellElem as? JsonArray ?: JsonArray(emptyList()))
                }
            }
            MarkdownBlock.Table(alignments = alignments, headers = headers, rows = rows)
        }
        else -> MarkdownBlock.ThematicBreak
    }
}

private fun parseMarkdownInlines(array: JsonArray): List<MarkdownInline> = array.map { elem ->
    val obj = elem.objectValue()
    when (obj.required("type")) {
        "text" -> MarkdownInline.Text(obj.required("text"))
        "strong" -> MarkdownInline.Strong(parseMarkdownInlines(obj["children"] as? JsonArray ?: JsonArray(emptyList())))
        "emphasis" -> MarkdownInline.Emphasis(parseMarkdownInlines(obj["children"] as? JsonArray ?: JsonArray(emptyList())))
        "strikethrough" -> MarkdownInline.Strikethrough(parseMarkdownInlines(obj["children"] as? JsonArray ?: JsonArray(emptyList())))
        "code" -> MarkdownInline.Code(obj.required("code"))
        "link" -> MarkdownInline.Link(
            url = obj.required("url"),
            children = parseMarkdownInlines(obj["children"] as? JsonArray ?: JsonArray(emptyList())),
        )
        "blocked_image" -> MarkdownInline.BlockedImage(obj.text("alt").orEmpty())
        "hard_break" -> MarkdownInline.HardBreak
        else -> MarkdownInline.SoftBreak
    }
}

sealed interface PreviewSource {
    data class Library(val id: String, val version: String?) : PreviewSource
    data class Channel(val channel: String, val sequence: ULong) : PreviewSource
}

data class FullscreenPreviewItem(
    val id: String,
    val source: PreviewSource,
    val name: String,
    val mime: String,
    val size: ULong,
    val updatedAt: ULong?,
    val sha256: String?,
    val canOpen: Boolean,
    val libraryFile: LibraryFile? = null,
    val versionLabel: String? = null,
    val explicitClass: PreviewClass? = null,
    val explicitPreviewable: Boolean? = null,
) {
    companion object {
        fun fromLibrary(file: LibraryFile, version: String? = null, versionLabel: String? = null): FullscreenPreviewItem {
            val key = if (version != null) "${file.id}@$version" else file.id
            return FullscreenPreviewItem(
                id = key,
                source = PreviewSource.Library(file.id, version),
                name = file.name,
                mime = file.mime,
                size = file.size,
                updatedAt = file.updatedAt,
                sha256 = file.sha256,
                canOpen = file.local && file.trashedAt == null,
                libraryFile = file,
                versionLabel = versionLabel,
            )
        }

        fun fromMessage(channel: String, message: MessageItem): FullscreenPreviewItem? {
            if (message.retracted || message.hidden || message.unreadable || message.attachmentDeleted) return null
            val name = message.attachmentName ?: return null
            val mime = message.attachmentMime.orEmpty()
            if (mime == "application/vnd.dmsg.secret-package") return null
            return FullscreenPreviewItem(
                id = "channel:$channel:${message.sequence}",
                source = PreviewSource.Channel(channel, message.sequence),
                name = name,
                mime = mime,
                size = message.attachmentSize ?: 0uL,
                updatedAt = message.createdAtMs,
                sha256 = null,
                canOpen = true,
            )
        }

        fun fromAttachment(channel: String, record: ChannelAttachmentRecord): FullscreenPreviewItem =
            FullscreenPreviewItem(
                id = "channel:$channel:${record.seq}",
                source = PreviewSource.Channel(channel, record.seq),
                name = record.name,
                mime = record.mime,
                size = record.size,
                updatedAt = if (record.storedAt > 0uL) record.storedAt else record.createdAt,
                sha256 = null,
                canOpen = true,
                explicitClass = record.previewClass,
                explicitPreviewable = record.previewable,
            )
    }
}

object PreviewFilterLogic {
    val MAX_IMAGE_BYTES: ULong = 20uL * 1024uL * 1024uL
    val MAX_MARKDOWN_BYTES: ULong = 2uL * 1024uL * 1024uL
    val MAX_TEXT_BYTES: ULong = 1024uL * 1024uL

    private val imageMimes = setOf(
        "image/png", "image/jpeg", "image/jpg", "image/gif", "image/webp",
        "image/bmp", "image/x-icon", "image/vnd.microsoft.icon",
        "image/avif", "image/heic", "image/heif", "image/tiff",
    )
    private val imageExts = setOf(
        "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "avif", "heic", "heif", "tif", "tiff",
    )
    private val markdownExts = setOf("md", "markdown", "mdown", "mkd")
    private val textMimes = setOf(
        "application/json", "application/ld+json", "application/xml", "application/toml",
        "application/yaml", "application/x-yaml", "application/javascript",
        "application/x-javascript", "application/x-sh", "application/sql", "application/csv",
    )
    private val textExts = setOf(
        "txt", "log", "json", "jsonc", "yaml", "yml", "toml", "xml", "csv", "tsv",
        "ini", "cfg", "conf", "env", "sh", "bash", "zsh", "py", "rs", "js", "mjs",
        "cjs", "ts", "mts", "cts", "jsx", "tsx", "html", "htm", "css", "scss", "sql",
        "swift", "kt", "kts", "java", "go", "c", "h", "cpp", "cc", "hpp", "rb", "php",
        "svelte", "vue", "dockerfile", "makefile",
    )

    fun classify(mime: String, name: String): PreviewClass {
        val base = mime.substringBefore(';').trim().lowercase()
        val ext = name.substringAfterLast('.', "").lowercase().takeIf { '.' in name } ?: ""
        if (base == "application/vnd.dmsg.secret-package" || base == "image/svg+xml" || ext == "svg" || ext == "svgz") {
            return PreviewClass.UNSUPPORTED
        }
        if (base == "text/markdown" || base == "text/x-markdown" || ext in markdownExts) {
            return PreviewClass.MARKDOWN
        }
        if (base in imageMimes || ((base.isEmpty() || base == "application/octet-stream") && ext in imageExts)) {
            return PreviewClass.IMAGE
        }
        if (base.startsWith("text/") || base in textMimes || ((base.isEmpty() || base == "application/octet-stream") && ext in textExts)) {
            return PreviewClass.TEXT
        }
        return PreviewClass.UNSUPPORTED
    }

    fun previewClass(item: FullscreenPreviewItem): PreviewClass =
        item.explicitClass ?: classify(item.mime, item.name)

    fun sizeLimit(kind: PreviewClass): ULong = when (kind) {
        PreviewClass.IMAGE -> MAX_IMAGE_BYTES
        PreviewClass.MARKDOWN -> MAX_MARKDOWN_BYTES
        PreviewClass.TEXT -> MAX_TEXT_BYTES
        PreviewClass.UNSUPPORTED -> 0uL
    }

    fun isPreviewable(item: FullscreenPreviewItem): Boolean {
        if (!item.canOpen) return false
        item.explicitPreviewable?.let { return it }
        val limit = sizeLimit(previewClass(item))
        return limit > 0uL && item.size <= limit
    }

    fun matches(item: FullscreenPreviewItem, filter: PreviewFilter): Boolean = when (filter) {
        PreviewFilter.ALL -> true
        PreviewFilter.PREVIEWABLE -> isPreviewable(item)
        PreviewFilter.IMAGES -> previewClass(item) == PreviewClass.IMAGE
        PreviewFilter.DOCUMENTS -> previewClass(item).let { it == PreviewClass.MARKDOWN || it == PreviewClass.TEXT }
    }

    fun counts(items: List<FullscreenPreviewItem>): Map<PreviewFilter, Int> {
        var previewable = 0
        var images = 0
        var documents = 0
        for (item in items) {
            if (isPreviewable(item)) previewable += 1
            when (previewClass(item)) {
                PreviewClass.IMAGE -> images += 1
                PreviewClass.MARKDOWN, PreviewClass.TEXT -> documents += 1
                PreviewClass.UNSUPPORTED -> Unit
            }
        }
        return mapOf(
            PreviewFilter.ALL to items.size,
            PreviewFilter.PREVIEWABLE to previewable,
            PreviewFilter.IMAGES to images,
            PreviewFilter.DOCUMENTS to documents,
        )
    }

    data class FilterPick(val filtered: List<FullscreenPreviewItem>, val activeId: String?)

    fun pickNearest(items: List<FullscreenPreviewItem>, currentId: String?, filter: PreviewFilter): FilterPick {
        val filtered = items.filter { matches(it, filter) }
        if (filtered.isEmpty()) return FilterPick(emptyList(), null)
        if (currentId != null && filtered.any { it.id == currentId }) {
            return FilterPick(filtered, currentId)
        }
        val origin = if (currentId != null) items.indexOfFirst { it.id == currentId } else -1
        if (origin < 0) return FilterPick(filtered, filtered.first().id)
        val allowed = filtered.mapTo(mutableSetOf()) { it.id }
        for (delta in 1 until items.size) {
            val forward = origin + delta
            if (forward < items.size && items[forward].id in allowed) {
                return FilterPick(filtered, items[forward].id)
            }
            val backward = origin - delta
            if (backward >= 0 && items[backward].id in allowed) {
                return FilterPick(filtered, items[backward].id)
            }
        }
        return FilterPick(filtered, filtered.first().id)
    }
}
