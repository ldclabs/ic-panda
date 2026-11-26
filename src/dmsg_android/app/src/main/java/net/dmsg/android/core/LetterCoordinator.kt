package net.dmsg.android.core

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.serialization.json.*
import net.dmsg.core.*
import java.security.SecureRandom

fun requestId(): String = ByteArray(32).also { SecureRandom().nextBytes(it) }.joinToString("") { "%02x".format(it) }
fun validAccount(value: String) = value.matches(Regex("[0-9a-v]{19}[0g]"))

data class LetterDraft(val id: String = requestId(), val recipient: String = "", val text: String = "", val invitation: String? = null, val updatedAt: ULong = 0u) {
    val bytes: Int get() = text.toByteArray(Charsets.UTF_8).size
    val valid: Boolean get() = validAccount(recipient) && text.isNotBlank() && bytes <= 6000 && (invitation.isNullOrEmpty() || invitation.matches(Regex("[0-9a-f]{64}")))
    fun input() = LetterDraftInput(id, recipient, text, invitation?.takeIf { it.isNotBlank() })
}
data class SentLetter(val id: String, val recipient: String, val text: String?, val createdAt: ULong?, val state: String, val error: String?, val pending: Boolean, val editable: Boolean)
fun parseLetterDrafts(value: JsonElement) = value.arrayRows().map { LetterDraft(it.required("id"), it.required("recipient"), it.required("text"), it.text("invitation"), it.requiredUnsigned("updated_at")) }
fun parseSentLetters(value: JsonElement) = value.arrayRows().map { SentLetter(it.required("id"), it.required("recipient"), it.text("text"), it.unsigned("created_at"), it.required("state"), it.text("error"), it.requiredFlag("pending"), it.requiredFlag("editable")) }

data class LetterLocation(val folder: String = "INBOX", val query: String = "", val editing: String? = null)

/** Draft identity survives Send, cancellation and retries. No form content enters Bundle. */
class LetterCoordinator(private val client: ClientController) {
    val incoming = ReadStore<List<Letter>> { it.isEmpty() }
    val history = ReadStore<List<SentLetter>> { it.isEmpty() }
    val saved = ReadStore<List<LetterDraft>> { it.isEmpty() }
    data class LetterSearchPage(val boxKind: String, val query: String, val hits: List<LetterSearchHit>)
    val searchResults = ReadStore<LetterSearchPage> { it.hits.isEmpty() }
    private var lastSearchKey = ""
    private val mutable = MutableStateFlow<Map<String, LetterDraft>>(emptyMap())
    val drafts = mutable.asStateFlow()
    private val dirty = mutableSetOf<String>()
    private val turns = mutableMapOf<String, Mutex>()
    private var revision = 0L
    var location = LetterLocation()
        private set
    fun rememberLocation(value: LetterLocation, owner: SessionStamp?) { if (owner != null && client.sessionCurrent(owner)) location = value }

    fun refresh(network: Boolean = true) {
        val request = incoming.begin()
        client.perform({ parseLetters(it.inbox(false).json) }, key = "letter:inbox", replace = true, quiet = true,
            onError = { incoming.fail(request, it) }) {
            incoming.accept(request, it)
            if (network) refreshIncoming()
        }
        val loaded = saved.begin()
        val sent = history.begin()
        val started = revision
        client.perform({ core -> parseLetterDrafts(core.letterDrafts().decode()) to parseSentLetters(core.letterHistory().decode()) },
            key = "letter:workspace", replace = true, quiet = true, onError = { saved.fail(loaded, it); history.fail(sent, it) }) { (values, outgoing) ->
            saved.accept(loaded, values); history.accept(sent, outgoing)
            val local = mutable.value.filterKeys { it in dirty || started != revision }
            mutable.value = values.associateBy { it.id } + local
        }
    }
    private fun refreshIncoming() {
        val request = incoming.begin()
        client.perform({ parseLetters(it.inbox(true).json) }, key = "letter:inbox:network", replace = true, quiet = true,
            onError = { incoming.fail(request, it) }) { incoming.accept(request, it) }
    }
    fun create(): String = LetterDraft().also { mutable.value += it.id to it; revision++ }.id
    fun update(draft: LetterDraft) {
        client.activity()
        if (client.isWorking("letter:queue:" + draft.id) || client.isWorking("letter:send:" + draft.id) || client.isWorking("letter:discard:" + draft.id)) return
        mutable.value += draft.id to draft; dirty += draft.id; revision++
        client.perform({ it.saveLetterDraft(draft.id, draft.recipient, draft.text, draft.invitation?.takeIf(String::isNotBlank)) },
            key = "letter:save:" + draft.id, replace = true, serial = turns.getOrPut(draft.id) { Mutex() }) {
            if (mutable.value[draft.id] == draft) dirty -= draft.id
        }
    }
    fun send(id: String, queued: () -> Unit = {}) {
        val draft = mutable.value[id] ?: return
        if (!draft.valid || client.isWorking("letter:queue:" + id)) return
        client.perform({ it.queueFreeLetter(id, draft.recipient, draft.text, draft.invitation?.takeIf(String::isNotBlank)) },
            key = "letter:queue:" + id, serial = turns.getOrPut(id) { Mutex() }) {
            mutable.value -= id; dirty -= id; revision++
            queued(); refresh(false); retry(id)
        }
    }
    fun retry(id: String) = client.perform({ core -> client.operation { core.sendFreeLetter(id, it) } },
        key = "letter:send:" + id, onError = { refresh(false) }) { refresh(false) }
    fun check(id: String) = client.perform({ core -> client.operation { core.refreshSentLetter(id, it) } },
        key = "letter:check:" + id, onError = { refresh(false) }) { refresh(false) }
    fun edit(id: String, ready: () -> Unit) = client.perform({ it.editUnsentLetter(id) },
        key = "letter:edit:" + id) { refresh(false); ready() }
    fun discard(id: String) {
        client.perform({ it.discardLetterDraft(id) }, key = "letter:discard:" + id,
            serial = turns.getOrPut(id) { Mutex() }, onError = { refresh(false) }) { mutable.value -= id; dirty -= id; revision++; refresh(false) }
    }
    fun mark(letter: Letter, read: Boolean = letter.read, archived: Boolean = letter.archived, reopen: Boolean = false) =
        client.perform({ core -> if (reopen) core.reopenLetterContact(letter.id, read, archived) else core.markLetter(letter.id, read, archived) },
            key = "letter:mark:" + letter.id) { refresh(false); client.notifications.status(); client.notices.refresh() }
    internal fun restore(input: LetterDraftInput) {
        if (input.id in dirty) return
        mutable.value += input.id to LetterDraft(input.id, input.recipient, input.text, input.invitation)
        dirty += input.id
        revision++
    }
    fun pending(): List<LetterDraftInput> = mutable.value.values.filter {
        it.id in dirty && !client.isWorking("letter:discard:" + it.id)
    }.map { it.input() }
    fun acknowledge(inputs: List<LetterDraftInput>) { inputs.forEach { input -> if (mutable.value[input.id]?.input() == input) dirty -= input.id } }
    fun search(query: String, boxKind: String) {
        val term = query.trim()
        if (term.isEmpty() || term.codePointCount(0, term.length) > 200) {
            client.cancelOperation("letter:search")
            searchResults.reset()
            lastSearchKey = ""
            return
        }
        val key = "$boxKind:$term"
        val same = key == lastSearchKey
        lastSearchKey = key
        val request = searchResults.begin(keep = same)
        client.perform({ core ->
            if (!same) delay(150)
            val snap = core.searchInbox(term, boxKind)
            LetterSearchPage(boxKind, term, parseLetterSearchHits(snap.decode()))
        }, key = "letter:search", replace = true, quiet = true, onError = { searchResults.fail(request, it) }) {
            searchResults.accept(request, it)
        }
    }

    fun clear(retainDrafts: Boolean = false) { incoming.reset(); saved.reset(); history.reset(); searchResults.reset(); lastSearchKey = ""; if (!retainDrafts) { location = LetterLocation(); mutable.value = emptyMap(); dirty.clear(); revision++ } }
}
