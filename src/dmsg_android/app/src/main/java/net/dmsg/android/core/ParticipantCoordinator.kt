package net.dmsg.android.core

import android.graphics.Bitmap
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.android.platform.BoundedImages

data class Participant(val account: String, val name: String?, val avatar: Bitmap? = null, val observedAt: ULong = 0u, val updatedAt: ULong? = null, val stale: Boolean = false)
fun parseParticipants(value: JsonElement): Map<String, Participant> = value.objectValue().mapValues { (account, value) ->
    check(validAccount(account)) { "INTEGRITY_FAILED" }; val row = value.objectValue()
    Participant(account, row.text("name"), observedAt = row.requiredUnsigned("observed_at_ms"), updatedAt = row.unsigned("updated_at"))
}

/** Only certified core names and core-validated, bounded public avatars are presented. */
class ParticipantCoordinator(private val client: ClientController, private val scope: CoroutineScope) {
    private val value = MutableStateFlow<Map<String, Participant>>(emptyMap())
    val people = value.asStateFlow()
    val accounts = ReadStore<List<String>> { it.isEmpty() }
    private class Observers(var count: Int = 0, var avatars: Int = 0)
    private val visible = mutableMapOf<String, Observers>()
    private val pending = mutableSetOf<String>()
    private val attempted = mutableSetOf<String>()
    private var batch: Job? = null
    fun observe(account: String, showAvatar: Boolean) {
        if (!validAccount(account)) return
        val observers = visible.getOrPut(account) { Observers() }
        observers.count++
        if (showAvatar) observers.avatars++
        if (account in attempted) { avatar(account, false); return }
        pending += account
        if (batch?.isActive != true) batch = scope.launch {
            delay(80)
            while (pending.isNotEmpty()) {
                val selected = pending.take(100); pending.removeAll(selected.toSet()); attempted += selected
                read(selected)
            }
        }
    }
    fun release(account: String, showAvatar: Boolean) {
        val observers = visible[account] ?: return
        if (--observers.count == 0) visible.remove(account)
        if (showAvatar && --observers.avatars == 0) {
            client.cancelOperation("participant:avatar:$account")
            value.value[account]?.let { value.value += account to it.copy(avatar = null) }
        }
        trim()
    }
    private fun trim() {
        val removable = value.value.keys.filterNot { it in visible }.take((value.value.size - 128).coerceAtLeast(0))
        value.value -= removable.toSet(); attempted.removeAll(removable.toSet())
    }
    private fun merge(next: Map<String, Participant>) {
        value.value += next.mapValues { (id, p) -> p.copy(avatar = value.value[id]?.takeIf { it.updatedAt == p.updatedAt }?.avatar) }; trim()
    }
    private fun read(selected: List<String>) {
        val key = "participants:" + selected.first()
        client.perform({ core -> parseParticipants(client.operation { core.channelNames(selected, false, it) }.decode()) }, key = key, quiet = true) {
            merge(it)
            selected.filter { id -> id in visible }.forEach { id -> avatar(id, false) }
            client.perform({ core -> parseParticipants(client.operation { core.channelNames(selected, true, it) }.decode()) }, key = "$key:refresh", quiet = true, onError = {
                value.value += selected.mapNotNull { id -> value.value[id]?.let { p -> id to p.copy(stale = true) } }.toMap()
            }) { names -> merge(names); selected.filter { id -> id in visible }.forEach { id -> avatar(id, true) } }
        }
    }
    private fun avatar(account: String, refresh: Boolean) {
        if ((visible[account]?.avatars ?: 0) == 0 || value.value[account]?.avatar != null || client.isWorking("participant:avatar:$account")) return
        client.perform({ core -> core.channelAvatar(account, refresh)?.let { bytes ->
            try { BoundedImages.thumbnail(bytes, 96) }
            finally { bytes.fill(0) }
        } }, key = "participant:avatar:$account", quiet = true) { bitmap -> if (account in visible) value.value[account]?.let { value.value += account to it.copy(avatar = bitmap) } }
    }
    fun browse() { val request = accounts.begin(); client.perform({ core -> (core.messagingAccounts().decode() as? JsonArray ?: error("INTEGRITY_FAILED")).map { (it as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf(::validAccount) ?: error("INTEGRITY_FAILED") } }, key = "participants:known", replace = true, quiet = true, onError = { accounts.fail(request, it) }) { accounts.accept(request, it) } }
    fun clear() { batch?.cancel(); batch = null; value.value = emptyMap(); pending.clear(); attempted.clear(); visible.clear(); accounts.reset() }
}
