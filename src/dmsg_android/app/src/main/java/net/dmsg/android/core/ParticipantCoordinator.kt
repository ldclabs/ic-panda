package net.dmsg.android.core

import android.graphics.Bitmap
import android.os.SystemClock
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.android.platform.BoundedImages

internal const val AVATAR_RETRY_MS = 60_000L

data class Participant(
    val account: String, val name: String?, val avatar: Bitmap? = null, val observedAt: ULong = 0u,
    val updatedAt: ULong? = null, val stale: Boolean = false, val avatarUrl: String? = null,
)
fun parseParticipants(value: JsonElement): Map<String, Participant> = value.objectValue().mapValues { (account, value) ->
    check(validAccount(account)) { "INTEGRITY_FAILED" }; val row = value.objectValue()
    Participant(account, row.text("name"), observedAt = row.requiredUnsigned("observed_at_ms"),
        updatedAt = row.unsigned("updated_at"), avatarUrl = row.text("avatar_url"))
}

/** Only certified core names and core-validated, bounded public avatars are presented. */
class ParticipantCoordinator(private val client: ClientController, private val scope: CoroutineScope) {
    private val value = MutableStateFlow<Map<String, Participant>>(emptyMap())
    val people = value.asStateFlow()
    val accounts = ReadStore<List<String>> { it.isEmpty() }
    private val visible = mutableMapOf<String, Int>()
    private val pending = mutableSetOf<String>()
    private val attempted = mutableSetOf<String>()
    private data class AvatarAttempt(val url: String, val retryAfter: Long = 0)
    private val avatarAttempts = mutableMapOf<String, AvatarAttempt>()
    private var batch: Job? = null
    fun observe(account: String) {
        if (!validAccount(account)) return
        val count = visible[account] ?: 0
        visible[account] = count + 1
        if (count > 0) return
        avatar(account)
        if (account in attempted) return
        pending += account
        if (batch?.isActive != true) batch = scope.launch {
            delay(80)
            while (pending.isNotEmpty()) {
                val selected = pending.take(100); pending.removeAll(selected.toSet()); attempted += selected
                read(selected)
            }
        }
    }
    fun release(account: String) {
        val count = visible[account] ?: return
        if (count == 1) {
            visible.remove(account)
            pending.remove(account)
        } else {
            visible[account] = count - 1
        }
        trim()
    }
    private fun trim() {
        val excess = value.value.size - 128
        if (excess <= 0) return
        val removable = value.value.keys.filterNot { it in visible }.take(excess).toSet()
        value.value -= removable
        attempted.removeAll(removable)
        avatarAttempts.keys.removeAll(removable)
    }
    private fun merge(next: Map<String, Participant>) {
        value.value += next.mapValues { (id, person) ->
            val previous = value.value[id]?.takeIf { it.avatarUrl == person.avatarUrl }
            person.copy(avatar = previous?.avatar)
        }
        trim()
    }
    private fun read(selected: List<String>) {
        val key = "participants:" + selected.first()
        client.perform({ core -> parseParticipants(client.operation { core.channelNames(selected, false, it) }.decode()) }, key = key, quiet = true,
            onError = { attempted.removeAll(selected.toSet()) }) { names ->
            merge(names)
            selected.forEach(::avatar)
            val visibleAccounts = selected.filter { it in visible }
            attempted.removeAll(selected.filterNot { it in visible }.toSet())
            if (visibleAccounts.isEmpty()) return@perform
            client.perform({ core -> parseParticipants(client.operation { core.channelNames(visibleAccounts, true, it) }.decode()) }, key = "$key:refresh", quiet = true, onError = {
                value.value += visibleAccounts.mapNotNull { id -> value.value[id]?.let { p -> id to p.copy(stale = true) } }.toMap()
                attempted.removeAll(visibleAccounts.filterNot { it in value.value }.toSet())
            }) { fresh ->
                merge(fresh)
                attempted.removeAll(visibleAccounts.filterNot { it in value.value }.toSet())
                visibleAccounts.forEach(::avatar)
            }
        }
    }
    private fun avatar(account: String) {
        val person = value.value[account] ?: return
        val url = person.avatarUrl ?: return
        if (account !in visible || person.avatar != null) return
        val key = "participant:avatar:$account"
        val previous = avatarAttempts[account]
        if (previous?.url == url && (client.isWorking(key) || SystemClock.elapsedRealtime() < previous.retryAfter)) return
        avatarAttempts[account] = AvatarAttempt(url)
        fun finish(bitmap: Bitmap?) {
            avatarAttempts.remove(account)
            val current = value.value[account]
            if (current == null || current.avatarUrl != url) return
            if (bitmap == null) {
                avatarAttempts[account] = AvatarAttempt(url, SystemClock.elapsedRealtime() + AVATAR_RETRY_MS)
            } else {
                value.value += account to current.copy(avatar = bitmap)
            }
        }
        client.perform({ core ->
            val bytes = core.channelAvatar(account, false) ?: core.channelAvatar(account, true)
            bytes?.let { BoundedImages.avatarThumbnail(it, 96) }
        }, key = key, replace = true, quiet = true, onError = { finish(null) }, apply = ::finish)
    }
    fun browse() { val request = accounts.begin(); client.perform({ core -> (core.messagingAccounts().decode() as? JsonArray ?: error("INTEGRITY_FAILED")).map { (it as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf(::validAccount) ?: error("INTEGRITY_FAILED") } }, key = "participants:known", replace = true, quiet = true, onError = { accounts.fail(request, it) }) { accounts.accept(request, it) } }
    fun clear() {
        batch?.cancel(); batch = null
        client.state.value.operations.filter { it.startsWith("participants:") || it.startsWith("participant:avatar:") }
            .forEach(client::cancelOperation)
        value.value = emptyMap(); pending.clear(); attempted.clear(); visible.clear(); avatarAttempts.clear(); accounts.reset()
    }
}
