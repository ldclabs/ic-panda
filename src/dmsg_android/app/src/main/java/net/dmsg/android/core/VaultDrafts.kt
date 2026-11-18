package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.serialization.json.*
import net.dmsg.core.*
import java.security.SecureRandom
import java.time.LocalDate
import java.time.YearMonth
import java.net.URI
import java.util.UUID

data class DraftField(val id: String, val kind: String, val label: String, val section: String, val value: String, val changed: Boolean, val concealed: Boolean) {
    fun edit(): FieldEdit = FieldEdit(id, kind, label, section, if (concealed && !changed) null else value)
}
data class VaultDraft(val id: String?, val revision: String?, val title: String, val kind: String, val notes: String,
    val favorite: Boolean, val tags: String, val fields: List<DraftField>, val files: List<String>) {
    val key: String get() = id ?: "new"
    fun edit(): VaultEdit = VaultEdit(title.trim(), kind, notes, favorite,
        tags.split(',').map(String::trim).filter(String::isNotEmpty).distinct(), fields.map { it.edit() }, files)
    fun switchKind(kind: String, template: List<VaultField>): VaultDraft {
        val keep = fields.filter { it.changed || it.value.isNotEmpty() || it.label.isNotEmpty() || it.section.isNotEmpty() || template.any { next -> next.id == it.id } }
        return copy(kind = kind, fields = keep + template.filter { field -> keep.none { it.id == field.id } }.map { it.draft(id == null) })
    }
    fun newField(kind: String): VaultDraft = copy(fields = fields + DraftField("field_" + UUID.randomUUID().toString().replace("-", ""), kind, "", "", "", true, kind in listOf("concealed", "otp")))
    fun field(id: String, change: (DraftField) -> DraftField): VaultDraft = copy(fields = fields.map { if (it.id == id) change(it) else it })
    fun move(id: String, delta: Int): VaultDraft {
        val at = fields.indexOfFirst { it.id == id }; val to = at + delta
        if (at < 0 || to !in fields.indices) return this
        return copy(fields = fields.toMutableList().apply { add(to, removeAt(at)) })
    }
    companion object {
        fun from(item: VaultItem?, template: List<VaultField>): VaultDraft = if (item == null)
            VaultDraft(null, null, "", "login", "", false, "", template.map { it.draft(true) }, emptyList())
        else VaultDraft(item.id, item.revision, item.title, item.kind, item.notes, item.favorite, item.tags.joinToString(", "), item.fields.map { it.draft(false) }, item.files)
    }
}
private fun VaultField.draft(new: Boolean) = DraftField(id, kind, label, section, if (concealed) "" else value, new, concealed)

class VaultDrafts(private val client: ClientController) {
    private data class Record(val input: VaultDraftInput, val draft: VaultDraft, val original: VaultDraft)
    private val drafts = mutableMapOf<String, Record>()
    private val pending = mutableSetOf<String>()
    private val recovered = mutableSetOf<String>()
    private val turn = Mutex()
    private val mutable = MutableStateFlow<VaultDraft?>(null)
    val current = mutable.asStateFlow()
    private val presenting = MutableStateFlow(false)
    val presented = presenting.asStateFlow()
    private var selected: String? = null
    private val mutableReady = MutableStateFlow(false)
    val ready = mutableReady.asStateFlow()
    private var loaded: Boolean
        get() = mutableReady.value
        set(value) { mutableReady.value = value }
    private var loading = false
    private var afterLoad: (() -> Unit)? = null
    val dirty: Boolean get() = selected?.let { drafts[it]?.let { record -> record.draft != record.original } } == true

    fun open(item: VaultItem?) {
        if (!loaded) { load { open(item) }; return }
        val previous = drafts.values.firstOrNull { it.draft.id == item?.id }
        val original = previous?.original ?: VaultDraft.from(item, if (item == null) vaultTemplate("login") else emptyList())
        val value = previous?.draft ?: original
        val input = previous?.input?.copy(revision = previous.input.revision + 1u, active = true)
            ?: VaultDraftInput(requestId(), value.id, value.revision, 1u, vaultDraftForm(value, original), true)
        drafts[input.id] = Record(input, value, original)
        selected = input.id; mutable.value = value; presenting.value = true; pending += input.id
        write(input)
    }

    fun update(change: (VaultDraft) -> VaultDraft) {
        val id = selected ?: return; val old = drafts[id] ?: return
        val next = change(old.draft)
        if (next == old.draft) return
        val input = old.input.copy(revision = old.input.revision + 1u, form = vaultDraftForm(next, old.original))
        drafts[id] = Record(input, next, old.original); mutable.value = next; pending += id
        recovered -= id
        write(input)
    }

    private fun write(input: VaultDraftInput) {
        client.perform({ core -> delay(200); core.checkpointVaultDraft(input) }, key = "vault:draft:" + input.id,
            replace = true, serial = turn, quiet = true, onError = client::showError) { accepted ->
            if (!accepted) finished(input.id)
            else acknowledge(input)
        }
    }

    internal fun restore(input: VaultDraftInput) {
        if (input.id in pending) return
        val form = Json.parseToJsonElement(input.form).jsonObject
        val value = parseVaultDraft(form.getValue("draft"))
        drafts[input.id] = Record(input, value, parseVaultDraft(form.getValue("initial")))
        pending += input.id
        recovered += input.id
        if (input.active) { selected = input.id; mutable.value = value; presenting.value = true }
    }

    fun pending(): List<VaultDraftInput> = pending.mapNotNull { drafts[it]?.input }
    fun acknowledge(input: VaultDraftInput) { if (drafts[input.id]?.input == input) pending -= input.id }
    fun input(): VaultDraftInput = drafts[selected]?.input ?: error("LOCKED")

    fun keep() {
        val id = selected ?: return; val record = drafts[id] ?: return
        val input = record.input.copy(revision = record.input.revision + 1u, active = false)
        drafts[id] = record.copy(input = input); pending += id
        presenting.value = false; write(input)
    }

    fun discard() {
        val id = selected ?: return
        client.perform({ it.discardVaultDraft(id) }, key = "vault:discard", serial = turn) { finished(id) }
    }

    fun finished(id: String) {
        drafts.remove(id); pending -= id; recovered -= id
        if (selected == id) { selected = null; mutable.value = null; presenting.value = false }
    }

    fun load(after: (() -> Unit)? = null) {
        if (loaded) { after?.invoke(); return }
        if (after != null) afterLoad = after
        if (loading || !client.state.value.unlocked || client.state.value.concealed) return
        loading = true
        val inputs = pending().filterNot { it.id in recovered }
        client.perform({ core ->
            val closed = inputs.filterNot { core.checkpointVaultDraft(it) }.map { it.id }.toSet()
            val values = core.vaultDrafts().decode().jsonArray.map { value ->
                val row = value.objectValue()
                val input = VaultDraftInput(row.required("id"), row.text("item_id"), row.text("base_revision"), row.requiredUnsigned("revision"), row.required("form"), row.requiredFlag("active"))
                val form = Json.parseToJsonElement(input.form).jsonObject
                val draft = parseVaultDraft(form.getValue("draft"))
                val original = parseVaultDraft(form.getValue("initial"))
                check(draft.id == input.itemId && draft.revision == input.baseRevision && original.id == input.itemId && original.revision == input.baseRevision) { "INTEGRITY_FAILED" }
                Record(input, draft, original)
            }
            values to closed
        }, key = "vault:drafts:read", serial = turn, onError = { loading = false }) { (values, closed) ->
            closed.forEach(::finished); inputs.forEach(::acknowledge)
            drafts.keys.filter { it !in pending && values.none { value -> value.input.id == it } }.toList().forEach(::finished)
            values.forEach { if (it.input.id !in pending) drafts[it.input.id] = it }
            if (!presenting.value) values.firstOrNull { it.input.active }?.let { selected = it.input.id; mutable.value = it.draft; presenting.value = true }
            loaded = true; loading = false
            val continuation = afterLoad; afterLoad = null; continuation?.invoke()
        }
    }

    fun suspend() { loaded = false; loading = false; afterLoad = null }
    fun clear() { suspend(); drafts.clear(); pending.clear(); recovered.clear(); selected = null; mutable.value = null; presenting.value = false }
}

internal fun vaultDraftForm(draft: VaultDraft, original: VaultDraft) = buildJsonObject {
    put("draft", draft.json()); put("initial", original.json())
}.toString()

private fun VaultDraft.json() = buildJsonObject {
    put("id", id); put("revision", revision); put("title", title); put("kind", kind); put("notes", notes)
    put("favorite", favorite); put("tags", tags)
    putJsonArray("files") { files.forEach { add(it) } }
    putJsonArray("fields") { fields.forEach { field -> add(buildJsonObject {
        put("id", field.id); put("kind", field.kind); put("label", field.label); put("section", field.section)
        put("value", field.value); put("changed", field.changed); put("concealed", field.concealed)
    }) } }
}

internal fun parseVaultDraft(value: JsonElement): VaultDraft {
    val row = value.objectValue()
    return VaultDraft(row.text("id"), row.text("revision"), row.required("title"), row.required("kind"), row.required("notes"),
        row.requiredFlag("favorite"), row.required("tags"), row.rows("fields").map { field ->
            DraftField(field.required("id"), field.required("kind"), field.required("label"), field.required("section"), field.required("value"), field.requiredFlag("changed"), field.requiredFlag("concealed"))
        }, row.getValue("files").jsonArray.map { it.jsonPrimitive.content })
}

fun validateDraftField(field: DraftField, otp: (String) -> Boolean = ::validateVaultOtp): String? {
    if (field.label.codePointCount(0, field.label.length) > 100 || field.section.codePointCount(0, field.section.length) > 100) return "FIELD_LABEL_TOO_LONG"
    if (field.id.startsWith("field_") && field.label.isBlank()) return "FIELD_NAME_REQUIRED"
    if (!field.changed || field.value.isEmpty()) return null
    if (field.value.codePointCount(0, field.value.length) > 10_000) return "FIELD_VALUE_TOO_LONG"
    val value = field.value.trim()
    val valid = when (field.kind) {
        "otp" -> otp(value)
        "date" -> runCatching { LocalDate.parse(value).toString() == value }.getOrDefault(false)
        "month" -> runCatching { YearMonth.parse(value).toString() == value }.getOrDefault(false)
        "email" -> value.contains('@') && !value.startsWith('@') && !value.endsWith('@')
        "url" -> runCatching { URI(if (value.contains("://")) value else "https://" + value) }.getOrNull()?.let { it.scheme?.lowercase() in listOf("http", "https") && !it.host.isNullOrEmpty() } == true
        else -> true
    }
    return if (valid) null else "INVALID_" + field.kind.uppercase()
}

fun generatedPassword(length: Int, symbols: Boolean, random: SecureRandom = SecureRandom()): String {
    val groups = listOf("ABCDEFGHJKLMNPQRSTUVWXYZ", "abcdefghijkmnopqrstuvwxyz", "23456789") + if (symbols) listOf("!@#$%&*-_+=?") else emptyList()
    val all = groups.joinToString("")
    val chars = groups.map { it[random.nextInt(it.length)] }.toMutableList()
    while (chars.size < length.coerceIn(12, 64)) chars += all[random.nextInt(all.length)]
    for (i in chars.lastIndex downTo 1) { val j = random.nextInt(i + 1); val temp = chars[i]; chars[i] = chars[j]; chars[j] = temp }
    return chars.joinToString("")
}
