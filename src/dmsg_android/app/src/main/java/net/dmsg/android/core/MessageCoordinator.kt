package net.dmsg.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import net.dmsg.core.*
import kotlinx.serialization.json.*

data class MessageTimeline(
    val channel: String,
    val items: List<MessageItem>,
    val hasMore: Boolean,
    val before: ULong?,
    val hasAfter: Boolean = false,
    val after: ULong? = null,
    val anchor: ULong? = null,
    val highlightSpans: List<HighlightSpan> = emptyList(),
)
internal fun MessageTimeline.withPage(page: MessagePage, older: Boolean): MessageTimeline {
    if (older) return copy(items = (page.items + items).distinctBy { it.id }.sortedBy { it.sequence }, hasMore = page.hasMore, before = page.nextBefore)
    val first = page.items.firstOrNull()?.sequence
    val previous = if (page.hasMore && first != null && items.lastOrNull()?.let { it.sequence >= first } == true)
        items.takeWhile { it.sequence < first } else emptyList()
    return if (previous.isEmpty()) copy(items = page.items, hasMore = page.hasMore, before = page.nextBefore, hasAfter = false, after = null)
    else copy(items = previous + page.items, hasAfter = false, after = null)
}
data class MessageOutbox(val channel: String, val items: List<OutgoingMessage>)
data class ComposerReply(val sequence: ULong, val digest: String, val preview: String? = null)
data class MessageComposer(val text: String = "", val attachments: List<ComposerAttachment> = emptyList(), val reply: ComposerReply? = null) {
    companion object { const val MAX_TEXT_BYTES = 16_000 }
    val bytes get() = text.toByteArray(Charsets.UTF_8).size
    val tooLong get() = bytes > MAX_TEXT_BYTES
    val sendable get() = !tooLong && attachments.size <= 10 && (text.isNotBlank() || attachments.isNotEmpty())
}

fun parseMessageComposers(value: JsonElement): Map<String, MessageComposer> = value.arrayRows().associate { row ->
    val draft = row["draft"]!!.objectValue()
    check(draft["attachments"] is JsonArray && "reply_to" in draft) { "INTEGRITY_FAILED" }
    val reply = draft["reply_to"]?.takeIf { it != JsonNull }?.objectValue()?.let { ComposerReply(it.requiredUnsigned("seq"), it.required("digest")) }
    row.required("channel") to MessageComposer(draft.required("text"), draft.rows("attachments").map { ComposerAttachment(it.required("file"), it.required("version"), it.required("name")) }, reply)
}

internal fun remainingComposer(queued: MessageComposer, current: MessageComposer) = current.copy(
    text = if (current.text == queued.text) "" else current.text,
    attachments = current.attachments.filterNot { file -> queued.attachments.any { it.file == file.file && it.version == file.version } },
    reply = current.reply.takeUnless { it?.sequence == queued.reply?.sequence && it?.digest == queued.reply?.digest },
)

/** Account-owned message reads, paging and authored drafts. Native outbox owns delivery. */
class MessageCoordinator(private val client: ClientController, private val scope: CoroutineScope) {
    val channels = ReadStore<List<ChannelItem>> { it.isEmpty() }
    val timeline = ReadStore<MessageTimeline> { it.items.isEmpty() }
    val outbox = ReadStore<MessageOutbox> { it.items.isEmpty() }
    private val syncError = MutableStateFlow<Pair<String, String>?>(null)
    val channelSyncError = syncError.asStateFlow()
    val attachmentChoices = ReadStore<FileCoordinator.Listing> { it.files.isEmpty() && it.folders.isEmpty() }
    data class ReferencedMessage(val channel: String, val sequence: ULong, val message: MessageItem?)
    val reference = ReadStore<ReferencedMessage> { it.message == null }
    private val mutableComposers = MutableStateFlow<Map<String, MessageComposer>>(emptyMap())
    val composers = mutableComposers.asStateFlow()
    private val readyValue = MutableStateFlow(false)
    val draftsReady = readyValue.asStateFlow()
    private val ledger = DraftLedger()
    private val writes = mutableMapOf<String, Job>()
    private val turns = mutableMapOf<String, Mutex>()
    private val deliveryTurns = mutableMapOf<String, Mutex>()
    private data class PendingQueue(val value: MessageComposer, val authored: DraftLedger.Draft)
    private val pendingQueues = mutableMapOf<String, PendingQueue>()
    private val pendingReads = mutableMapOf<String, ULong>()
    private var selected: String? = null
    private var refreshAgain = false

    fun browse() {
        val request = channels.begin()
        client.perform({ it.channels().items to parseMessageComposers(it.messageComposers().decode()) }, key = "message:channels", replace = true,
            onError = { channels.fail(request, it) }) { (values, saved) ->
            channels.accept(request, values)
            mutableComposers.value = values.associate { it.id to MessageComposer(it.draft) } + saved + mutableComposers.value
            readyValue.value = true
        }
    }

    private val directoryText = MutableStateFlow("")
    val directoryQuery = directoryText.asStateFlow()
    private val directoryFilesOnly = MutableStateFlow(false)
    val directoryHasFile = directoryFilesOnly.asStateFlow()
    val channelHits = ReadStore<List<ChannelSearchHit>> { it.isEmpty() }
    val richMatches = ReadStore<List<RichChannelGroup>> { it.isEmpty() }
    val conversationSearch = ReadStore<RichChannelSearchPage> { it.hits.isEmpty() }
    private val searchRevision = MutableStateFlow(0L)
    val directoryRevision = searchRevision.asStateFlow()

    private fun invalidateDirectory() {
        if (directoryText.value.isBlank() && !directoryFilesOnly.value) return
        client.cancelOperation("message:directory")
        channelHits.reset()
        richMatches.reset()
        searchRevision.value++
    }

    fun editDirectoryQuery(text: String) {
        if (text == directoryText.value) return
        client.cancelOperation("message:directory")
        channelHits.reset()
        richMatches.reset()
        directoryText.value = text
    }

    fun toggleDirectoryHasFile() {
        client.cancelOperation("message:directory")
        richMatches.reset()
        directoryFilesOnly.value = !directoryFilesOnly.value
    }

    fun searchDirectory(text: String, sources: List<String>, blocked: Set<String>, hasFile: Boolean = directoryFilesOnly.value) {
        val query = text.trim()
        if (text != directoryText.value || (query.isEmpty() && !hasFile) || query.codePointCount(0, query.length) > 200) return
        val allowed = sources.toSet()
        val chReq = channelHits.begin(false)
        val msgReq = richMatches.begin(false)
        client.perform({ core ->
            delay(150)
            val chList = if (query.isEmpty()) emptyList() else core.searchChannels(query).decode().arrayRows().map(::parseChannelSearchHit)
            val groups = parseRichChannelGroups(core.searchMessagesAll(query, 6u, 30u, hasFile).decode())
                .filter { it.channelId in allowed }
                .mapNotNull { group ->
                    val visible = group.hits.filterNot { it.account in blocked }
                    if (visible.isEmpty()) null else group.copy(hits = visible)
                }
            chList to groups
        }, key = "message:directory", replace = true, quiet = true, onError = { channelHits.fail(chReq, it); richMatches.fail(msgReq, it) }) { (chList, groups) ->
            channelHits.accept(chReq, chList)
            richMatches.accept(msgReq, groups)
        }
    }

    fun moreRichMatches(group: RichChannelGroup, blocked: Set<String>, hasFile: Boolean = directoryFilesOnly.value) {
        val before = group.nextBefore ?: return
        val query = directoryText.value.trim()
        val previous = richMatches.value() ?: return
        val request = richMatches.begin()
        client.perform({ core ->
            val obj = core.searchChannelMessages(group.channelId, query, before, 20u, hasFile, null).decode().objectValue()
            val hits = obj.rows("hits").map { parseRichMessageHit(it, group.channelId, group.channelName) }.filterNot { it.account in blocked }
            hits to obj.unsigned("next_before")
        }, key = "message:directory", replace = true, quiet = true, onError = { richMatches.fail(request, it) }) { (hits, nextBefore) ->
            richMatches.accept(request, previous.map {
                if (it.channelId == group.channelId) it.copy(hits = (it.hits + hits).distinctBy { h -> h.sequence }, nextBefore = nextBefore) else it
            })
        }
    }

    fun searchConversation(channel: String, text: String, hasFile: Boolean = false, more: Boolean = false) {
        val term = text.trim()
        if (term.isEmpty() && !hasFile) {
            client.cancelOperation("message:conv-search")
            conversationSearch.reset()
            return
        }
        if (term.codePointCount(0, term.length) > 200) return
        val previous = if (more) conversationSearch.value()?.takeIf { it.channelId == channel && it.query == term && it.hasFile == hasFile } else null
        val before = previous?.nextBefore
        if (more && before == null) return
        val request = conversationSearch.begin(keep = previous != null)
        client.perform({ core ->
            if (!more) delay(140)
            val obj = core.searchChannelMessages(channel, term, before, 40u, hasFile, null).decode().objectValue()
            val chName = channels.value()?.firstOrNull { it.id == channel }?.name.orEmpty()
            val hits = obj.rows("hits").map { parseRichMessageHit(it, channel, chName) }
            val merged = if (previous != null) (previous.hits + hits).distinctBy { it.sequence } else hits
            RichChannelSearchPage(channel, term, hasFile, merged, obj.unsigned("next_before"))
        }, key = "message:conv-search", replace = true, quiet = true, onError = { conversationSearch.fail(request, it) }) {
            conversationSearch.accept(request, it)
        }
    }

    fun clearConversationSearch() {
        client.cancelOperation("message:conv-search")
        conversationSearch.reset()
    }

    fun loadAround(channel: String, sequence: ULong, highlightSpans: List<HighlightSpan> = emptyList()) {
        if (selected != channel) syncError.value = null
        selected = channel
        val request = timeline.begin(keep = false)
        client.perform({ core ->
            core.messagesAround(channel, sequence, 30u, 30u)
        }, key = "message:timeline", replace = true, quiet = true, onError = { timeline.fail(request, it) }) { win ->
            timeline.accept(request, MessageTimeline(
                channel = channel,
                items = win.items,
                hasMore = win.hasBefore,
                before = win.nextBefore,
                hasAfter = win.hasAfter,
                after = win.nextAfter,
                anchor = win.anchor,
                highlightSpans = highlightSpans,
            ))
            // Catch the channel up as `load` does once the window is in place.
            if (selected == channel) refresh(channel)
        }
        val outgoing = outbox.begin(keep = false)
        client.perform({ MessageOutbox(channel, it.outbox(channel).items) }, key = "message:outbox", replace = true, quiet = true,
            onError = { outbox.fail(outgoing, it) }) { outbox.accept(outgoing, it) }
    }

    fun newer() {
        val current = timeline.value() ?: return
        val after = current.after ?: return
        if (!current.hasAfter || timeline.state.value is ReadState.Loading) return
        val request = timeline.begin(keep = true)
        client.perform({ core ->
            core.messagesAfter(current.channel, after, 100u)
        }, key = "message:timeline", replace = true, quiet = true, onError = { timeline.fail(request, it) }) { page ->
            val merged = (current.items + page.items).distinctBy { it.id }.sortedBy { it.sequence }
            timeline.accept(request, current.copy(
                items = merged,
                hasAfter = page.hasMore,
                after = page.nextBefore,
            ))
        }
    }

    fun composer(channel: String): MessageComposer = mutableComposers.value[channel]
        ?: MessageComposer(channels.value()?.firstOrNull { it.id == channel }?.draft.orEmpty())

    fun edit(channel: String, value: MessageComposer) {
        if (!readyValue.value) return
        if (value.tooLong) { client.showError("MESSAGE_TOO_LONG"); return }
        client.activity()
        val account = client.state.value.session?.account ?: return
        mutableComposers.value += channel to value
        val authored = ledger.edit(account, channel, value)
        val handle = client.scopedHandle() ?: return
        val stamp = client.currentStamp() ?: run { handle.destroy(); return }
        val turn = turns.getOrPut(channel) { Mutex() }
        writes[channel]?.cancel()
        writes[channel] = scope.launch {
            try {
                turn.withLock {
                    if (ledger.current(authored)) {
                        withContext(Dispatchers.IO) { handle.saveAuthoredDraft(authored.native()) }
                        ledger.saved(authored)
                    }
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Throwable) { if (client.accepts(stamp)) client.showError(client.errorCode(error)) }
            finally { handle.destroy() }
        }
    }

    fun attach(channel: String, file: LibraryFile) {
        if (!readyValue.value) { client.showError("DRAFT_NOT_READY"); return }
        val previous = composer(channel)
        if (previous.attachments.any { it.file == file.id && it.version == file.version }) return
        if (!file.local || file.size > 100uL * 1024u * 1024u || previous.attachments.size >= 10) { client.showError("ATTACHMENT_LIMIT"); return }
        edit(channel, previous.copy(attachments = previous.attachments + ComposerAttachment(file.id, file.version, file.name)))
    }

    fun chooseFolder(folder: String) {
        val request = attachmentChoices.begin(false)
        client.perform({ parseFiles(it.files(folder).json).let { (folders, files) -> FileCoordinator.Listing(folder, "", false, folders, files) } },
            key = "message:attachment-choices", replace = true, quiet = true, onError = { attachmentChoices.fail(request, it) }) { attachmentChoices.accept(request, it) }
    }

    fun locate(channel: String, sequence: ULong) {
        val request = reference.begin(false)
        client.perform({ core ->
            val before = if (sequence == ULong.MAX_VALUE) sequence else sequence + 1u
            val cached = core.messages(channel, before, 1u).items.singleOrNull { it.sequence == sequence }
            val message = cached ?: client.operation { core.earlierMessages(channel, before, 1u, it) }.items.singleOrNull { it.sequence == sequence }
            ReferencedMessage(channel, sequence, message)
        }, key = "message:reference", replace = true, quiet = true, onError = { reference.fail(request, it) }) { reference.accept(request, it) }
    }
    fun leaveReference() { client.cancelOperation("message:reference"); reference.reset() }

    fun load(channel: String, refresh: Boolean = true, more: Boolean = false, preserveLoaded: Boolean = false) {
        val same = selected == channel
        if (more && (!same || timeline.state.value is ReadState.Loading || timeline.value()?.hasMore != true)) return
        val previous = if ((more || preserveLoaded) && same) timeline.value() else null
        if (selected != channel) syncError.value = null
        selected = channel
        val request = timeline.begin(keep = same)
        client.perform({ core ->
            if (more && previous?.before != null) client.operation { core.earlierMessages(channel, previous.before, 100u, it) }
            else core.messages(channel, null, 100u)
        }, key = "message:timeline", replace = true, quiet = true, onError = { timeline.fail(request, it) }) { page ->
            val updated = previous?.withPage(page, older = more) ?: MessageTimeline(channel, page.items, page.hasMore, page.nextBefore)
            timeline.accept(request, updated)
        }
        val outgoing = outbox.begin(keep = same)
        client.perform({ MessageOutbox(channel, it.outbox(channel).items) }, key = "message:outbox", replace = true, quiet = true,
            onError = { outbox.fail(outgoing, it) }) { outbox.accept(outgoing, it) }
        if (refresh) refresh(channel)
    }

    fun refresh(channel: String? = selected) {
        channel ?: return
        val stamp = client.currentStamp()?.takeIf(client::accepts) ?: return
        if (client.isWorking("message:refresh")) { refreshAgain = true; return }
        refreshAgain = false
        fun finished() {
            if (refreshAgain) {
                refreshAgain = false
                if (client.accepts(stamp)) refresh(selected)
            }
        }
        client.perform({ core -> client.operation { core.refreshChannel(channel, it) } }, key = "message:refresh", quiet = true,
            onError = { code -> if (selected == channel) syncError.value = channel to code; finished() }) {
            if (selected == channel) {
                syncError.value = null
                if (timeline.value()?.hasAfter != true) load(channel, false, preserveLoaded = true)
            }
            invalidateDirectory(); browse(); finished()
        }
    }

    fun hint(channel: String?) { invalidateDirectory(); browse(); if (channel == selected) refresh(channel) }
    fun reload() { invalidateDirectory(); browse(); selected?.let { load(it, false) } }
    fun older() { selected?.let { load(it, false, more = true) } }
    fun markVisible(channel: String, through: ULong) {
        val key = "message:read:$channel"
        if (client.isWorking(key) && through <= (pendingReads[channel] ?: 0uL)) return
        pendingReads[channel] = through
        fun finished() { if (pendingReads[channel] == through) pendingReads.remove(channel) }
        client.perform({ it.markRead(channel, through) }, key = key, replace = true,
            onError = { finished() }) { finished(); browse() }
    }

    fun send(channel: String, onQueued: (String) -> Unit = {}) {
        val key = "message:queue:" + channel
        if (client.isWorking(key)) return
        if (!readyValue.value || pendingQueues[channel] == null && !composer(channel).sendable) return
        val account = client.state.value.session?.account ?: return
        val pending = pendingQueues[channel] ?: composer(channel).let { value ->
            PendingQueue(value, ledger.edit(account, channel, value)).also { pendingQueues[channel] = it }
        }
        val value = pending.value
        val authored = pending.authored
        writes[channel]?.cancel()
        client.perform({ core -> core.queueAuthoredMessages(authored.native()) },
            key = key, serial = turns.getOrPut(channel) { Mutex() }, onError = { code ->
                if (code !in setOf("EXECUTION_UNKNOWN", "OPERATION_FAILED", "UNAVAILABLE", "CANCELLED")) pendingQueues.remove(channel)
            }) { ids ->
            if (pendingQueues[channel] == pending) pendingQueues.remove(channel)
            if (ledger.current(authored) && composer(channel) == value) mutableComposers.value += channel to MessageComposer()
            else edit(channel, remainingComposer(value, composer(channel)))
            ledger.saved(authored)
            if (selected == channel) {
                load(channel, false, preserveLoaded = true)
                ids.lastOrNull()?.let(onQueued)
            }
            ids.forEach { retry(channel, it) }
        }
    }
    fun retry(channel: String, id: String) = client.perform({ core -> client.operation { core.sendMessage(channel, id, it) } },
        key = "message:send:" + id, serial = deliveryTurns.getOrPut(channel) { Mutex() }, onError = { if (selected == channel) load(channel, false, preserveLoaded = true) }) {
            if (selected == channel) load(channel, false, preserveLoaded = true)
            invalidateDirectory(); browse()
        }
    internal fun restore(account: String, input: MessageDraft) {
        if (ledger.text(account, input.channel) != null) return
        val reply = input.replySequence?.let { sequence -> input.replyDigest?.let { ComposerReply(sequence, it) } }
        val value = MessageComposer(input.text, input.attachments, reply)
        mutableComposers.value += input.channel to value
        ledger.edit(account, input.channel, value)
    }

    fun pending(account: String) = ledger.pending(account)
    fun saved(draft: DraftLedger.Draft) = ledger.saved(draft)
    fun suspend() { writes.values.forEach { it.cancel() }; writes.clear(); pendingReads.clear(); refreshAgain = false }
    fun clear(account: String?, retainDrafts: Boolean = false) {
        suspend(); syncError.value = null; channelHits.reset(); richMatches.reset(); conversationSearch.reset(); directoryText.value = ""; directoryFilesOnly.value = false; channels.reset(); timeline.reset(); outbox.reset(); attachmentChoices.reset(); reference.reset()
        if (!retainDrafts) { account?.let(ledger::clear); mutableComposers.value = emptyMap(); readyValue.value = false; pendingQueues.clear(); selected = null }
    }
}
