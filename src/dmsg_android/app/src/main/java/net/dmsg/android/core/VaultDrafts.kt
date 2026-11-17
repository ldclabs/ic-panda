package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
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

class VaultDrafts {
    private val drafts = mutableMapOf<String, VaultDraft>()
    private val originals = mutableMapOf<String, VaultDraft>()
    private val mutable = MutableStateFlow<VaultDraft?>(null)
    val current = mutable.asStateFlow()
    val dirty: Boolean get() = mutable.value?.let { it != originals[it.key] } == true
    fun open(item: VaultItem?) {
        val key = item?.id ?: "new"
        mutable.value = drafts.getOrPut(key) { VaultDraft.from(item, if (item == null) vaultTemplate("login") else emptyList()).also { originals[key] = it } }
    }
    fun update(change: (VaultDraft) -> VaultDraft) { mutable.value?.let { val next = change(it); drafts[next.key] = next; mutable.value = next } }
    fun discard() { mutable.value?.let { drafts.remove(it.key); originals.remove(it.key) }; mutable.value = null }
    fun clear() { drafts.clear(); originals.clear(); mutable.value = null }
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
