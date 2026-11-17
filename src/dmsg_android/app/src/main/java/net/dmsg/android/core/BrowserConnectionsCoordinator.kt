package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import java.net.URI
import java.net.URLDecoder

data class ConnectLink(val destination: String, val account: String?, val request: String?) {
    fun url() = "https://connect.dmsg.net/app/" + destination + listOfNotNull(account?.let { "account=" + it }, request?.let { "request=" + it }).let { if (it.isEmpty()) "" else "?" + it.joinToString("&") }
    fun route() = Route("browser-request", id = request, channel = account, field = destination)
    companion object {
        fun parse(raw: String): ConnectLink? = runCatching {
            check(raw.length <= 2048)
            val uri = URI(raw)
            check(uri.rawUserInfo == null && uri.port == -1 && uri.rawFragment == null)
            val destination = when {
                uri.scheme == "https" && uri.host == "connect.dmsg.net" && uri.rawPath in setOf("/app/pair", "/app/request") -> uri.rawPath.removePrefix("/app/")
                uri.scheme == "dmsg" && uri.host == "connect" && uri.rawPath in setOf("/pair", "/request") -> uri.rawPath.removePrefix("/")
                else -> error("INVALID_INPUT")
            }
            val entries = uri.rawQuery?.split('&')?.map {
                val pair = it.split('=', limit = 2); check(pair.size == 2)
                URLDecoder.decode(pair[0], "UTF-8") to URLDecoder.decode(pair[1], "UTF-8")
            }.orEmpty()
            check(entries.map { it.first }.distinct().size == entries.size && entries.all { it.first in setOf("account", "request") })
            val query = entries.toMap(); val account = query["account"]; val request = query["request"]
            check(account == null || validAccount(account))
            if (destination == "request") check(account != null && request != null && request.matches(Regex("[0-9a-f]{64}")))
            else check(request == null)
            ConnectLink(destination, account, request)
        }.getOrNull()
    }
}
data class BrowserInvitation(val id: String, val url: String, val expiresAt: ULong)
data class BrowserCandidate(val id: String, val label: String)
data class BrowserPairing(val id: String, val label: String?, val state: String, val expiresAt: ULong?)
fun parseBrowserInvitation(value: JsonElement) = value.objectValue().let { BrowserInvitation(it.required("invite_id"), it.required("url"), it.requiredUnsigned("expires_at")) }
fun parseBrowserInvitations(value: JsonElement) = value.arrayRows().map(::parseBrowserInvitation)
fun parseBrowserCandidates(value: JsonElement) = value.objectValue().rows("candidates").map { BrowserCandidate(it.required("pairing_id"), it.required("label")) }
fun parseBrowserPairings(value: JsonElement) = value.arrayRows().map { BrowserPairing(it.required("pairing_id"), it.text("label"), it.required("state"), it.unsigned("expires_at")) }
fun trustedBrowserInvitation(raw: String, origin: String): Boolean = runCatching {
    val uri = URI(raw); val base = URI(origin)
    raw.length <= 32 * 1024 && uri.scheme == "https" && uri.scheme == base.scheme && uri.host == base.host && uri.port == base.port && uri.rawUserInfo == null && uri.rawPath == base.rawPath.trimEnd('/') + "/v1/pair" && uri.rawQuery == null && uri.rawFragment?.matches(Regex("[A-Za-z0-9_-]+")) == true
}.getOrDefault(false)

class BrowserConnectionsCoordinator(private val client: ClientController) {
    val invitations = ReadStore<List<BrowserInvitation>> { it.isEmpty() }
    val pairings = ReadStore<List<BrowserPairing>> { it.isEmpty() }
    val request = ReadStore<String?> { it == null }
    private val candidatesValue = MutableStateFlow<Map<String, List<BrowserCandidate>>>(emptyMap())
    val candidates = candidatesValue.asStateFlow()
    private val errorsValue = MutableStateFlow<Map<String, String>>(emptyMap())
    val errors = errorsValue.asStateFlow()
    private var requestLink: ConnectLink? = null

    fun load() {
        val local = invitations.begin()
        client.perform({ parseBrowserInvitations(it.browserInvitations().decode()) }, key = "browser:invitations", replace = true, quiet = true, onError = { invitations.fail(local, it) }) { values ->
            invitations.accept(local, values)
            candidatesValue.value = candidatesValue.value.filterKeys { id -> values.any { it.id == id } }
            errorsValue.value = errorsValue.value.filterKeys { id -> values.any { it.id == id } }
            values.forEach(::candidates)
        }
        val remote = pairings.begin()
        client.perform({ parseBrowserPairings(it.browserPairings().decode()) }, key = "browser:pairings", replace = true, quiet = true, onError = { pairings.fail(remote, it) }) { pairings.accept(remote, it) }
    }
    fun candidates(invite: BrowserInvitation) {
        if (client.isWorking("browser:candidates:" + invite.id) || invite.expiresAt <= System.currentTimeMillis().toULong()) return
        client.perform({ parseBrowserCandidates(it.browserCandidates(invite.id).decode()) }, key = "browser:candidates:" + invite.id, quiet = true, onError = { errorsValue.value += invite.id to it }) {
            candidatesValue.value += invite.id to it; errorsValue.value -= invite.id
        }
    }
    fun create(open: (BrowserInvitation) -> Unit) = client.perform({ parseBrowserInvitation(it.inviteBrowser().decode()) }, key = "browser:create", onError = { load() }) { load(); open(it) }
    fun close(invite: BrowserInvitation) = client.perform({ it.forgetBrowserInvitation(invite.id) }, key = "browser:close:" + invite.id) {
        candidatesValue.value -= invite.id; errorsValue.value -= invite.id; load()
    }
    fun grant(invite: BrowserInvitation, candidate: BrowserCandidate) {
        if (invite.expiresAt <= System.currentTimeMillis().toULong()) { client.showError("EXPIRED"); return }
        client.perform({ it.grantBrowser(invite.id, candidate.id) }, key = "browser:grant:" + candidate.id, onError = { load() }) { load() }
    }
    fun revoke(pairing: BrowserPairing) = client.perform({ it.revokeBrowser(pairing.id) }, key = "browser:revoke:" + pairing.id, onError = { load() }) { load() }
    fun locate(link: ConnectLink) {
        if (link.account != client.state.value.session?.account || !client.state.value.unlocked || link.request == null) return
        if (requestLink != link) { request.reset(); requestLink = link }
        val read = request.begin()
        client.perform({ core -> client.operation { core.syncConnect(it) }; core.browserRequestApproval(link.request) }, key = "browser:request", replace = true, quiet = true, onError = { request.fail(read, it) }) { request.accept(read, it) }
    }
    fun clear() { invitations.reset(); pairings.reset(); request.reset(); candidatesValue.value = emptyMap(); errorsValue.value = emptyMap(); requestLink = null }
}
