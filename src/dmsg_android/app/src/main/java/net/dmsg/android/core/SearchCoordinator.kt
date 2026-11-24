package net.dmsg.android.core

import kotlinx.coroutines.delay
import kotlinx.serialization.json.*
import net.dmsg.core.ChannelItem
import net.dmsg.core.MessageItem

data class HighlightSpan(val text: String, val matched: Boolean)

fun JsonObject.spans(key: String): List<HighlightSpan> {
    val arr = this[key] as? JsonArray ?: return emptyList()
    return arr.mapNotNull { el ->
        val obj = el as? JsonObject ?: return@mapNotNull null
        val text = obj.text("text") ?: return@mapNotNull null
        HighlightSpan(text, obj.flag("matched"))
    }
}

fun JsonObject.optionalStrings(key: String): List<String> {
    val arr = this[key] as? JsonArray ?: return emptyList()
    return arr.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content }
}

data class ChannelSearchHit(
    val id: String,
    val name: String,
    val nameSpans: List<HighlightSpan>,
    val matchedMember: String?,
    val memberSpans: List<HighlightSpan>,
    val unread: ULong,
    val muted: Boolean,
    val lastAt: ULong?,
    val score: UInt,
)

data class RichMessageHit(
    val channelId: String,
    val channelName: String,
    val sequence: ULong,
    val id: String,
    val account: String,
    val senderName: String?,
    val text: String,
    val createdAtMs: ULong,
    val attachmentName: String?,
    val attachmentDeleted: Boolean,
    val snippetSpans: List<HighlightSpan>,
    val score: UInt,
    val fileSpans: List<HighlightSpan> = emptyList(),
) {
    private val showFile get() = attachmentName != null && !attachmentDeleted &&
        (text.isBlank() || (fileSpans.any { it.matched } && snippetSpans.none { it.matched }))
    val previewText get() = if (showFile) attachmentName.orEmpty() else text
    val previewSpans get() = if (showFile) fileSpans else snippetSpans
}

data class RichChannelGroup(
    val channelId: String,
    val channelName: String,
    val hits: List<RichMessageHit>,
    val nextBefore: ULong?,
)

data class RichChannelSearchPage(
    val channelId: String,
    val query: String,
    val hasFile: Boolean,
    val hits: List<RichMessageHit>,
    val nextBefore: ULong?,
)

data class LetterSearchHit(
    val id: String,
    val boxKind: String,
    val account: String,
    val displayName: String?,
    val nameSpans: List<HighlightSpan>,
    val snippetSpans: List<HighlightSpan>,
    val createdAt: ULong,
    val read: Boolean,
    val archived: Boolean,
    val state: String,
    val score: UInt,
)

data class GlobalSearchResult(
    val query: String,
    val vault: List<VaultSummary>,
    val channels: List<ChannelSearchHit>,
    val messages: List<RichMessageHit>,
    val letters: List<LetterSearchHit>,
    val folders: List<LibraryFolder>,
    val files: List<LibraryFile>,
) {
    val isEmpty: Boolean
        get() = vault.isEmpty() && channels.isEmpty() && messages.isEmpty() && letters.isEmpty() && folders.isEmpty() && files.isEmpty()
}

fun parseChannelSearchHit(row: JsonObject): ChannelSearchHit {
    val last = row["last"]?.takeIf { it != JsonNull }?.objectValue()
    val matchField = row.text("match_field")
    val matchValue = row.text("match_value")
    return ChannelSearchHit(
        id = row.required("id"),
        name = row.required("name"),
        nameSpans = row.spans("name_spans"),
        matchedMember = if (matchField == "member") matchValue else null,
        memberSpans = emptyList(),
        unread = row.unsigned("unread") ?: 0uL,
        muted = row.flag("muted"),
        lastAt = last?.unsigned("stored_at") ?: row.unsigned("active_at"),
        score = row.unsigned("score")?.toUInt() ?: 0u,
    )
}

fun parseRichMessageHit(row: JsonObject, fallbackChannelId: String = "", fallbackChannelName: String = ""): RichMessageHit {
    val msg = row["message"]!!.objectValue()
    val file = msg["file"]?.takeIf { it != JsonNull }?.objectValue()
    return RichMessageHit(
        channelId = row.text("channel") ?: row.text("channel_id") ?: fallbackChannelId,
        channelName = row.text("channel_name") ?: fallbackChannelName,
        sequence = msg.requiredUnsigned("seq"),
        id = msg.text("message_id") ?: msg.requiredUnsigned("seq").toString(),
        account = msg.required("account"),
        senderName = row.text("sender_name"),
        text = msg.text("text").orEmpty(),
        createdAtMs = msg.unsigned("stored_at") ?: msg.unsigned("created_at") ?: 0uL,
        attachmentName = file?.text("name"),
        attachmentDeleted = msg.flag("file_deleted"),
        snippetSpans = row.spans("snippet_spans"),
        fileSpans = row.spans("file_spans"),
        score = row.unsigned("score")?.toUInt() ?: 0u,
    )
}

fun parseRichChannelGroups(value: JsonElement): List<RichChannelGroup> = value.arrayRows().map { row ->
    val chId = row.required("channel_id")
    val chName = row.required("channel_name")
    RichChannelGroup(
        channelId = chId,
        channelName = chName,
        hits = row.rows("hits").map { parseRichMessageHit(it, chId, chName) },
        nextBefore = row.unsigned("next_before"),
    )
}

fun parseLetterSearchHits(value: JsonElement): List<LetterSearchHit> = value.arrayRows().map { row ->
    LetterSearchHit(
        id = row.required("id"),
        boxKind = row.required("box_kind"),
        account = row.required("account"),
        displayName = row.text("display_name"),
        nameSpans = row.spans("name_spans"),
        snippetSpans = row.spans("snippet_spans"),
        createdAt = row.unsigned("created_at") ?: 0uL,
        read = row.flag("read", true),
        archived = row.flag("archived"),
        state = row.text("state") ?: "visible",
        score = row.unsigned("score")?.toUInt() ?: 0u,
    )
}

fun parseFilesSearchListing(value: JsonElement): Pair<List<LibraryFolder>, List<LibraryFile>> {
    val root = value.objectValue()
    val folders = root.rows("folders").map { hit ->
        parseFolder(hit).copy(
            nameSpans = hit.spans("name_spans"),
            pathSpans = hit.spans("path_spans"),
        )
    }
    val files = root.rows("files").map { hit ->
        parseFile(hit).copy(
            nameSpans = hit.spans("name_spans"),
            pathSpans = hit.spans("path_spans"),
            matchField = hit.text("match_field"),
            matchValue = hit.text("match_value"),
        )
    }
    return folders to files
}

fun parseVaultSummaryFromGlobal(row: JsonObject): VaultSummary = VaultSummary(
    id = row.required("id"),
    revision = row.required("revision"),
    kind = row.text("kind") ?: row.text("type") ?: "note",
    title = row.required("title"),
    subtitle = row.text("subtitle").orEmpty(),
    favorite = row.flag("favorite"),
    tags = row.optionalStrings("tags"),
    synced = row.flag("synced", true),
    conflict = row.flag("conflict"),
    reserved = row.flag("reserved"),
    deleted = row.flag("deleted"),
    createdAt = row.unsigned("createdAt") ?: row.unsigned("created_at") ?: 0uL,
    updatedAt = row.unsigned("updatedAt") ?: row.unsigned("updated_at") ?: 0uL,
    saveState = row.text("saveState") ?: row.text("save_state"),
    titleSpans = row.spans("titleSpans").ifEmpty { row.spans("title_spans") },
    subtitleSpans = row.spans("subtitleSpans").ifEmpty { row.spans("subtitle_spans") },
    matchField = row.text("matchField") ?: row.text("match_field"),
    matchValue = row.text("matchValue") ?: row.text("match_value"),
)

fun parseGlobalSearchResult(query: String, value: JsonElement): GlobalSearchResult {
    val root = value.objectValue()
    val vault = (root["vault"] as? JsonArray).orEmpty().map { parseVaultSummaryFromGlobal(it.objectValue()) }
    val channels = (root["channels"] as? JsonArray).orEmpty().map { parseChannelSearchHit(it.objectValue()) }
    val messages = (root["messages"] as? JsonArray).orEmpty().map { parseRichMessageHit(it.objectValue()) }
    val letters = (root["letters"] as? JsonArray)?.let { parseLetterSearchHits(it) }.orEmpty()
    val folders = mutableListOf<LibraryFolder>()
    val files = mutableListOf<LibraryFile>()
    (root["files"] as? JsonArray).orEmpty().forEach { el ->
        val obj = el.objectValue()
        val kind = obj.text("kind") ?: "file"
        val id = obj.required("id")
        val name = obj.required("name")
        val folder = obj.text("folder") ?: "root"
        val path = obj.optionalStrings("path")
        val nameSpans = obj.spans("name_spans")
        val pathSpans = obj.spans("path_spans")
        if (kind == "folder") {
            folders += LibraryFolder(id = id, name = name, parent = folder, path = path, nameSpans = nameSpans, pathSpans = pathSpans)
        } else {
            val status = obj.text("status") ?: "local"
            files += LibraryFile(
                id = id,
                name = name,
                size = obj.unsigned("size") ?: 0uL,
                status = status,
                sha256 = "",
                mime = obj.text("mime").orEmpty(),
                version = "",
                folder = folder,
                local = status != "cloud_only",
                starred = false,
                tags = emptyList(),
                note = "",
                updatedAt = 0uL,
                backup = null,
                trashedAt = null,
                path = path,
                nameSpans = nameSpans,
                pathSpans = pathSpans,
                matchField = obj.text("match_field"),
                matchValue = obj.text("match_value"),
            )
        }
    }
    return GlobalSearchResult(query, vault, channels, messages, letters, folders, files)
}

/** Unified cross-workspace search coordinator backed by `dmsg_client::search`. */
class SearchCoordinator(private val client: ClientController) {
    val global = ReadStore<GlobalSearchResult> { it.isEmpty }
    private var lastQuery = ""

    fun searchAll(raw: String) {
        val term = raw.trim()
        if (term.isEmpty() || term.codePointCount(0, term.length) > 200) {
            client.cancelOperation("search:global")
            global.reset()
            lastQuery = ""
            return
        }
        val same = term == lastQuery
        lastQuery = term
        val request = global.begin(keep = same)
        client.perform({ core ->
            if (!same) delay(150)
            val snap = core.globalSearch(term, emptyList(), 12u)
            check(snap.generation == core.controlGeneration()) { "LOCKED" }
            parseGlobalSearchResult(term, snap.decode())
        }, key = "search:global", replace = true, quiet = true, onError = { global.fail(request, it) }) {
            global.accept(request, it)
        }
    }

    fun clear() {
        client.cancelOperation("search:global")
        global.reset()
        lastQuery = ""
    }
}
