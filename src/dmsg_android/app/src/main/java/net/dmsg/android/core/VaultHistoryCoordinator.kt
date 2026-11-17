package net.dmsg.android.core

import kotlinx.serialization.json.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.core.*

data class VaultVersion(val revision: String, val at: ULong, val device: String, val current: Boolean, val deleted: Boolean, val conflict: Boolean, val synced: Boolean)
data class VaultHistory(val current: VaultItem, val versions: List<VaultVersion>)
data class ConflictField(val id: String, val kind: String, val label: String, val section: String, val value: String) {
    val hidden: Boolean get() = kind in setOf("concealed", "otp")
}
data class ConflictItem(val id: String, val revision: String, val title: String, val notes: String, val tags: List<String>, val files: List<String>, val favorite: Boolean, val deleted: Boolean, val fields: List<ConflictField>)
data class VaultConflict(val copy: ConflictItem, val current: ConflictItem, val hidden: List<String>) { val key: String get() = copy.id + ":" + copy.revision }
data class ConflictChoice(val expected: String, val picks: Set<String> = emptySet(), val merging: Boolean = false)
data class ConflictPart(val id: String, val current: String, val other: String, val hidden: Boolean = false)
fun parseVaultVersions(value: JsonElement) = value.arrayRows().map { VaultVersion(it.required("revision"), it.requiredUnsigned("at"), it.required("device"), it.requiredFlag("current"), it.requiredFlag("deleted"), it.requiredFlag("conflict"), it.requiredFlag("synced")) }
private fun parseConflictItem(value: JsonElement): ConflictItem {
    val entry = value.objectValue(); val item = entry["item"]!!.objectValue()
    return ConflictItem(entry.required("id"), entry.required("revision"), item.required("title"), item.required("notes"), item.strings("tags"), item.strings("files"), item.requiredFlag("favorite"), entry.requiredFlag("deleted"), item.rows("fields").map {
        ConflictField(it.required("id"), it.required("type"), it.required("label"), it.required("section"), it.required("value"))
    })
}
fun parseVaultConflicts(value: JsonElement) = value.arrayRows().map { VaultConflict(parseConflictItem(it["copy"]!!), parseConflictItem(it["head"]!!), it.strings("hidden")) }
fun conflictParts(value: VaultConflict): List<ConflictPart> {
    val a = value.current; val b = value.copy
    val parts = mutableListOf(ConflictPart("title", a.title, b.title), ConflictPart("notes", a.notes, b.notes), ConflictPart("tags", a.tags.joinToString(", "), b.tags.joinToString(", ")), ConflictPart("files", a.files.joinToString("\n"), b.files.joinToString("\n")), ConflictPart("favorite", a.favorite.toString(), b.favorite.toString()))
    for (id in (a.fields.map { it.id } + b.fields.map { it.id }).distinct()) {
        val left = a.fields.find { it.id == id }; val right = b.fields.find { it.id == id }
        val concealed = left?.hidden == true || right?.hidden == true
        fun description(field: ConflictField?): String = field?.let { listOf(it.section, it.label.ifBlank { it.id }, it.kind).filter(String::isNotBlank).joinToString(" · ") + "\n" + if (concealed) "••••••••" else it.value } ?: "—"
        if (left != right || id in value.hidden) parts += ConflictPart("field:" + id, description(left), description(right), concealed)
    }
    return parts.filter { it.current != it.other || it.id.removePrefix("field:") in value.hidden }
}

class VaultHistoryCoordinator(private val client: ClientController) {
    val history = ReadStore<VaultHistory> { it.versions.isEmpty() }
    val historical = ReadStore<VaultItem>()
    val comparison = ReadStore<Pair<VaultItem, VaultItem>>()
    val conflicts = ReadStore<List<VaultConflict>> { it.isEmpty() }
    private val mutableChoices = MutableStateFlow<Map<String, ConflictChoice>>(emptyMap())
    val choices = mutableChoices.asStateFlow()
    fun choice(value: VaultConflict): ConflictChoice = mutableChoices.value[value.key]?.takeIf { it.expected == value.current.revision } ?: ConflictChoice(value.current.revision)
    fun choose(value: VaultConflict, next: ConflictChoice) { if (next.expected == value.current.revision) mutableChoices.value += value.key to next }
    private var item: String? = null
    private var version: Pair<String, String>? = null
    fun load(id: String) {
        val request = history.begin(keep = item == id); item = id
        client.perform({ core -> VaultHistory(core.vaultItem(id, null).item, parseVaultVersions(core.vaultHistory(id).decode())) }, key = "vault:history", replace = true,
            onError = { history.fail(request, it) }) { history.accept(request, it) }
    }
    fun select(id: String, revision: String) {
        val request = historical.begin(keep = version == (id to revision)); version = id to revision
        client.perform({ it.vaultItem(id, revision).item }, key = "vault:historical", replace = true,
            onError = { historical.fail(request, it) }) { historical.accept(request, it) }
    }
    fun compare(id: String, revision: String) {
        val request = comparison.begin(false)
        client.perform({ core -> core.vaultItem(id, null).item to core.vaultItem(id, revision).item }, key = "vault:compare", replace = true,
            onError = { comparison.fail(request, it) }) { comparison.accept(request, it) }
    }
    fun restore(id: String, revision: String, expected: String, done: () -> Unit) = client.perform({ it.revertItem(id, revision, expected) }, key = "vault:restore:" + id) { load(id); client.vault.browse(); client.vault.select(id); done() }
    fun conflicts() {
        val request = conflicts.begin()
        client.perform({ parseVaultConflicts(it.conflicts().decode()) }, key = "vault:conflicts", replace = true,
            onError = { conflicts.fail(request, it) }) { conflicts.accept(request, it) }
    }
    fun resolve(value: VaultConflict, current: Boolean, done: () -> Unit) = client.perform({ it.resolveConflict(value.key, value.current.revision, current) }, key = "vault:resolve:" + value.key,
        onError = { conflicts() }) { mutableChoices.value -= value.key; conflicts(); client.vault.browse(); done() }
    fun merge(value: VaultConflict, theirs: Set<String>, done: () -> Unit) = client.perform({ it.mergeVaultConflict(value.key, value.current.revision, theirs.sorted()) }, key = "vault:resolve:" + value.key,
        onError = { conflicts() }) { mutableChoices.value -= value.key; conflicts(); client.vault.browse(); done() }
    fun clear() { history.reset(); historical.reset(); comparison.reset(); conflicts.reset(); mutableChoices.value = emptyMap(); item = null; version = null }
}
