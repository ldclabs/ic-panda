package net.dmsg.android.core

import kotlinx.serialization.json.*

data class SavedAccount(val id: String, val device: String, val current: Boolean, val bound: Boolean, val quickUnlock: Boolean, val moved: Boolean, val lastLogin: ULong?)
data class LocalContent(val versions: ULong, val bytes: ULong, val unbacked: ULong, val unbackedBytes: ULong, val pending: ULong, val lastSync: ULong?)
data class DisplayCacheUsage(val bytes: ULong, val domains: Int)
fun parseSavedAccounts(value: JsonElement) = value.arrayRows().map { SavedAccount(it.required("account"), it.required("device_id"), it.requiredFlag("current"), it.requiredFlag("bound"), it.requiredFlag("fast_unlock"), it.requiredFlag("moved"), it.unsigned("login_unlocked_at")) }
fun parseLocalContent(value: JsonElement) = value.objectValue().let { LocalContent(it.requiredUnsigned("versions"), it.requiredUnsigned("bytes"), it.requiredUnsigned("unbacked_versions"), it.requiredUnsigned("unbacked_bytes"), it.requiredUnsigned("pending_sync"), it.unsigned("last_sync_ok_at")) }
fun parseDisplayUsage(value: JsonElement): DisplayCacheUsage {
    val rows = value.objectValue().rows("domains")
    val bytes = rows.fold(0uL) { total, row -> val size = row.requiredUnsigned("sealed_bytes"); check(ULong.MAX_VALUE - total >= size) { "INTEGRITY_FAILED" }; total + size }
    return DisplayCacheUsage(bytes, rows.size)
}

class AccountWorkspaceCoordinator(private val client: ClientController) {
    val accounts = ReadStore<List<SavedAccount>> { it.isEmpty() }
    val local = ReadStore<LocalContent>()
    val cache = ReadStore<DisplayCacheUsage>()
    fun accounts() {
        val request = accounts.begin()
        client.perform({ parseSavedAccounts(it.accounts().decode()) }, key = "account:local-list", replace = true, quiet = true, onError = { accounts.fail(request, it) }) { values ->
            accounts.accept(request, values)
            if (values.isNotEmpty()) client.perform({ core -> parseParticipants(client.operation { core.channelNames(values.map { it.id }.take(100), true, it) }.decode()) }, key = "account:local-names", replace = true, quiet = true) { names ->
                client.rememberAccountNames(names.filterKeys { it != client.state.value.session?.account || client.profile.saved.value() == null }.mapValues { it.value.name })
            }
        }
    }
    fun local() { val request = local.begin(); client.perform({ parseLocalContent(it.localOnly().decode()) }, key = "account:local-only", replace = true, quiet = true, onError = { local.fail(request, it) }) { local.accept(request, it) } }
    fun cache() { val request = cache.begin(); client.perform({ parseDisplayUsage(it.displayCacheStatus().decode()) }, key = "account:display-cache", replace = true, quiet = true, onError = { cache.fail(request, it) }) { cache.accept(request, it) } }
    fun clearCache() = client.perform({ it.clearDisplayCache() }, key = "account:clear-cache") {
        client.profile.clear(retainForm = true); client.participants.clear(); client.cloudFiles.clear()
        cache(); client.notice("CACHE_CLEARED")
    }
    fun remove() {
        val account = client.state.value.session?.account ?: return
        if (local.value() == null || local.state.value is ReadState.Stale || local.state.value is ReadState.Failed) return
        client.removeLocalAccount(account)
    }
    fun clear() { accounts.reset(); local.reset(); cache.reset() }
}
