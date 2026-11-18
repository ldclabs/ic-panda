package net.dmsg.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import net.dmsg.core.*
import kotlinx.serialization.json.*

data class MessageTimeline(val channel: String, val query: String, val items: List<MessageItem>, val hasMore: Boolean, val before: ULong?)
data class ChannelMatches(val channel: String, val items: List<MessageItem>, val before: ULong?)
data class MessageOutbox(val channel: String, val items: List<OutgoingMessage>)
data class ComposerReply(val sequence: ULong, val digest: String, val preview: String? = null)
data class MessageComposer(val text: String = "", val attachments: List<ComposerAttachment> = emptyList(), val reply: ComposerReply? = null) {
    val tooLong get() = text.toByteArray(Charsets.UTF_8).size > 16_000
    val sendable get() = !tooLong && attachments.size <= 10 && (text.isNotBlank() || attachments.isNotEmpty())
}

fun parseMessageComposers(value: JsonElement): Map<String, MessageComposer> = value.arrayRows().associate { row ->
    val draft = row["draft"]!!.objectValue()
    check(draft["attachments"] is JsonArray && "reply_to" in draft) { "INTEGRITY_FAILED" }
    val reply = draft["reply_to"]?.takeIf { it != JsonNull }?.objectValue()?.let { ComposerReply(it.requiredUnsigned("seq"), it.required("digest")) }
    row.required("channel") to MessageComposer(draft.required("text"), draft.rows("attachments").map { ComposerAttachment(it.required("file"), it.required("version"), it.required("name")) }, reply)
}

internal fun remainingComposer(queued: MessageComposer, current: MessageComposer) = current.copy(
    attachments = current.attachments.filterNot { file -> queued.attachments.any { it.file == file.file && it.version == file.version } },
    reply = current.reply.takeUnless { it?.sequence == queued.reply?.sequence && it?.digest == queued.reply?.digest },
)

/** Account-owned message reads, paging and authored drafts. Native outbox owns delivery. */
class MessageCoordinator(private val client: ClientController, private val scope: CoroutineScope) {
    val channels = ReadStore<List<ChannelItem>> { it.isEmpty() }
    val timeline = ReadStore<MessageTimeline> { it.items.isEmpty() }
    val outbox = ReadStore<MessageOutbox> { it.items.isEmpty() }
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
    private var selected: String? = null
    private var refreshing = false
    private var refreshAgain = false
    var query = ""
        private set

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
    val matches = ReadStore<List<ChannelMatches>> { it.isEmpty() }

    fun editDirectoryQuery(text: String) {
        if (text == directoryText.value) return
        client.cancelOperation("message:directory")
        matches.reset()
        directoryText.value = text
    }

    fun searchDirectory(text: String, sources: List<String>, blocked: Set<String>) {
        val query = text.trim()
        if (text != directoryText.value || query.isEmpty() || query.codePointCount(0, query.length) > 200) return
        val request = matches.begin(false)
        client.perform({ core ->
            delay(180)
            sources.map { channel -> localMatches(core, channel, query, blocked, null) }.filter { it.items.isNotEmpty() }
        }, key = "message:directory", replace = true, quiet = true, onError = { matches.fail(request, it) }) {
            matches.accept(request, it)
        }
    }

    fun moreMatches(group: ChannelMatches, blocked: Set<String>) {
        val before = group.before ?: return
        val query = directoryText.value.trim()
        val previous = matches.value() ?: return
        val request = matches.begin()
        client.perform({ localMatches(it, group.channel, query, blocked, before) }, key = "message:directory", replace = true,
            quiet = true, onError = { matches.fail(request, it) }) { next ->
            matches.accept(request, previous.map { if (it.channel == group.channel) next.copy(items = group.items + next.items) else it })
        }
    }

    private suspend fun localMatches(core: MobileEngine, channel: String, query: String, blocked: Set<String>, before: ULong?): ChannelMatches {
        var cursor = before
        while (true) {
            val page = core.searchMessagesPage(channel, query, cursor)
            val hits = page.items.asReversed().filter { item ->
                !item.hidden && !item.retracted && !item.unreadable && item.account !in blocked &&
                    (item.text.contains(query, true) || !item.attachmentDeleted && item.attachmentName?.contains(query, true) == true)
            }
            val next = page.nextBefore.takeIf { page.hasMore && it != cursor }
            if (hits.isNotEmpty() || next == null) return ChannelMatches(channel, hits, next)
            cursor = next
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

    fun load(channel: String, refresh: Boolean = true, search: String = query, more: Boolean = false, preserveLoaded: Boolean = false) {
        val same = selected == channel && query == search
        if (more && (!same || timeline.state.value is ReadState.Loading || timeline.value()?.hasMore != true)) return
        val previous = if ((more || preserveLoaded) && same) timeline.value() else null
        selected = channel; query = search
        val request = timeline.begin(keep = same)
        client.perform({ core ->
            if (more && search.isBlank() && previous?.before != null) client.operation { core.earlierMessages(channel, previous.before, 100u, it) }
            else if (search.isBlank()) core.messages(channel, null, 100u)
            else core.searchMessagesPage(channel, search, if (more) previous?.before else null)
        }, key = "message:timeline", replace = true, quiet = true, onError = { timeline.fail(request, it) }) { page ->
            val items = if (previous == null) page.items else (page.items + previous.items).distinctBy { it.id }.sortedBy { it.sequence }
            timeline.accept(request, MessageTimeline(channel, search, items, if (preserveLoaded && previous != null) previous.hasMore else page.hasMore, if (preserveLoaded && previous != null) previous.before else page.nextBefore))
        }
        val outgoing = outbox.begin(keep = same)
        client.perform({ MessageOutbox(channel, it.outbox(channel).items) }, key = "message:outbox", replace = true,
            onError = { outbox.fail(outgoing, it) }) { outbox.accept(outgoing, it) }
        if (refresh) refresh(channel)
    }

    fun refresh(channel: String? = selected) {
        channel ?: return
        val stamp = client.currentStamp()?.takeIf(client::accepts) ?: return
        if (refreshing) { refreshAgain = true; return }
        refreshing = true
        fun finished() {
            refreshing = false
            if (refreshAgain) {
                refreshAgain = false
                // The completion callback still owns message:refresh until it returns.
                scope.launch { yield(); if (client.accepts(stamp)) refresh(selected) }
            }
        }
        client.perform({ core -> client.operation { core.refreshChannel(channel, it) } }, key = "message:refresh",
            onError = { finished() }) {
            if (selected == channel) load(channel, false, preserveLoaded = true)
            browse(); finished()
        }
    }

    fun hint(channel: String?) { browse(); if (channel == selected) refresh(channel) }
    fun reload() { browse(); selected?.let { load(it, false) } }
    fun search(text: String) { selected?.let { load(it, false, text) } }
    fun older() { selected?.let { load(it, false, more = true) } }
    fun markVisible(channel: String, through: ULong) = client.perform({ it.markRead(channel, through) }, key = "message:read:" + channel) { browse() }

    fun send(channel: String) {
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
            if (selected == channel) load(channel, false, preserveLoaded = true)
            ids.forEach { retry(channel, it) }
        }
    }
    fun retry(channel: String, id: String) = client.perform({ core -> client.operation { core.sendMessage(channel, id, it) } },
        key = "message:send:" + id, serial = deliveryTurns.getOrPut(channel) { Mutex() }, onError = { if (selected == channel) load(channel, false, preserveLoaded = true) }) {
            if (selected == channel) load(channel, false, preserveLoaded = true)
            browse()
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
    fun suspend() { writes.values.forEach { it.cancel() }; writes.clear(); refreshing = false; refreshAgain = false }
    fun clear(account: String?, retainDrafts: Boolean = false) {
        suspend(); matches.reset(); directoryText.value = ""; channels.reset(); timeline.reset(); outbox.reset(); attachmentChoices.reset(); reference.reset()
        if (!retainDrafts) { account?.let(ledger::clear); mutableComposers.value = emptyMap(); readyValue.value = false; pendingQueues.clear(); selected = null; query = "" }
    }
}
