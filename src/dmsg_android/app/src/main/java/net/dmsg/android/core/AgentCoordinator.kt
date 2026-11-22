package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import java.net.URI

fun agentLines(value: String) = value.lineSequence().map(String::trim).filter(String::isNotEmpty).toList()
fun validAgentLabel(value: String) = value.matches(Regex("[a-z0-9_-]{1,40}"))
fun validAgentIdentifier(value: String) = value.startsWith("did:agent:") && value.length in 11..256 && value.none { it.isWhitespace() || it.isISOControl() }
data class AgentAuthority(val scopes: String = "", val audiences: String = "") {
    val valid get(): Boolean {
        val scope = agentLines(scopes); val audience = agentLines(audiences)
        return scope.size in 1..8 && scope.distinct().size == scope.size && scope.all { it.toByteArray(Charsets.UTF_8).size <= 64 && '*' !in it && it.none { c -> c.isWhitespace() || c.isISOControl() } } &&
            audience.size in 1..4 && audience.distinct().size == audience.size && audience.all {
                it.toByteArray(Charsets.UTF_8).size <= 256 && it.none { c -> c.isWhitespace() || c.isISOControl() } &&
                    (validAgentIdentifier(it) || runCatching { URI(it).let { uri -> uri.scheme == "https" && uri.host != null && uri.rawPath.isNullOrEmpty() && uri.rawUserInfo == null && uri.rawQuery == null && uri.rawFragment == null } }.getOrDefault(false))
            }
    }
}
data class AgentPermissionsDraft(val capabilities: String = "", val days: String = "7", val pattern: String = "", val target: String = "") {
    val valid get() = days.toULongOrNull() in 1uL..366uL && agentLines(capabilities).let { it.size in 1..16 && it.distinct().size == it.size && it.all { value -> value.toByteArray(Charsets.UTF_8).size <= 512 && value.none(Char::isISOControl) } }
}
data class AgentDelegationDraft(val name: String = "", val controller: AgentAuthority = AgentAuthority(), val unrestricted: Boolean = false, val subject: String = "", val grant: AgentAuthority = AgentAuthority(), val days: String = "7") {
    val validController get() = name.trim().isNotEmpty() && name.toByteArray(Charsets.UTF_8).size <= 64 && name.none(Char::isISOControl) && (unrestricted || controller.valid)
    val validGrant get() = validAgentIdentifier(subject.trim()) && grant.valid && days.toULongOrNull() in 1uL..366uL
}
data class HostedAgentQuota(val used: ULong, val limit: ULong?)
data class ManagedAgent(val label: String, val id: String, val paused: Boolean, val resident: Boolean, val createdAt: ULong)
data class AgentDeviceGrant(val id: String, val grantee: String, val capabilities: List<String>, val createdAt: ULong, val expiresAt: ULong?, val revokedAt: ULong?)
data class AgentController(val generation: UInt, val id: String, val name: String?, val validFrom: ULong, val retiredAt: ULong?, val invalidFrom: ULong?, val unrestricted: Boolean, val scopes: List<String>, val audiences: List<String>)
data class AgentPrincipal(val id: String, val controllers: List<AgentController>, val version: ULong, val publishedVersion: ULong, val published: Boolean)
data class AgentCredential(val id: String, val subject: String, val scopes: List<String>, val audiences: List<String>, val status: String, val notBefore: ULong?, val expiresAt: ULong?)
fun parseManagedAgents(value: JsonElement) = value.objectValue().rows("agents").map { ManagedAgent(it.required("label"), it.required("agent_id"), it.requiredFlag("paused"), it.requiredFlag("resident"), it.requiredUnsigned("created_at")) }
fun parseHostedAgentQuota(value: JsonElement): HostedAgentQuota =
    value.objectValue().let { HostedAgentQuota(it.requiredUnsigned("used"), it.unsigned("limit")) }
fun parseAgentDeviceGrants(value: JsonElement) = value.arrayRows().map { AgentDeviceGrant(it.required("id"), it.required("grantee"), it.strings("capabilities"), it.requiredUnsigned("created_at"), it.unsigned("expires_at"), it.unsigned("revoked_at")) }
fun parseAgentPrincipal(value: JsonElement): AgentPrincipal? {
    if (value == JsonNull) return null
    val row = value.objectValue()
    return AgentPrincipal(row.required("principal_id"), row.rows("controllers").map {
        val authority = it["delegation"]!!
        val unrestricted = authority is JsonPrimitive && authority.isString && authority.content == "*"
        val restricted = if (!unrestricted) authority.objectValue() else null
        AgentController(it.requiredUInt("generation"), it.required("agent_id"), it.text("name"), it.requiredUnsigned("valid_from"), it.unsigned("retired_at"), it.unsigned("invalid_from"), unrestricted, restricted?.strings("scopes").orEmpty(), restricted?.strings("audiences").orEmpty())
    }, row.requiredUnsigned("version"), row.requiredUnsigned("published_version"), row.requiredFlag("published")).also {
        check(it.published == (it.version == it.publishedVersion) && it.publishedVersion <= it.version) { "INTEGRITY_FAILED" }
    }
}
fun parseAgentCredentials(value: JsonElement) = value.arrayRows().map { AgentCredential(it.required("id"), it.required("subject"), it.strings("scopes"), it.strings("audiences"), it.required("status"), it.unsigned("not_before"), it.unsigned("expires_at")) }

class AgentCoordinator(private val client: ClientController) {
    val listing = ReadStore<List<ManagedAgent>> { it.isEmpty() }
    val quota = ReadStore<HostedAgentQuota>()
    val grants = ReadStore<List<AgentDeviceGrant>> { it.isEmpty() }
    val principal = ReadStore<AgentPrincipal?> { it == null }
    val credentials = ReadStore<List<AgentCredential>> { it.isEmpty() }
    private val labelValue = MutableStateFlow("")
    val label = labelValue.asStateFlow()
    fun editLabel(value: String) { client.activity(); labelValue.value = value }
    private val permissionsValue = MutableStateFlow<Map<String, AgentPermissionsDraft>>(emptyMap())
    val permissions = permissionsValue.asStateFlow()
    private val delegationValue = MutableStateFlow(AgentDelegationDraft())
    val delegation = delegationValue.asStateFlow()
    fun editPermissions(id: String, value: AgentPermissionsDraft) { client.activity(); permissionsValue.value += id to value }
    fun editDelegation(value: AgentDelegationDraft) { client.activity(); delegationValue.value = value }
    fun load() {
        val read = listing.begin()
        client.perform({ parseManagedAgents(it.agents().decode()) }, key = "agents:list", replace = true, quiet = true, onError = { listing.fail(read, it) }) { listing.accept(read, it) }
        val seats = quota.begin()
        client.perform({ parseHostedAgentQuota(it.hostedAgentQuota().decode()) }, key = "agents:quota", replace = true, quiet = true, onError = { quota.fail(seats, it) }) { quota.accept(seats, it) }
        val access = grants.begin()
        client.perform({ parseAgentDeviceGrants(it.agentGrants().decode()) }, key = "agents:grants", replace = true, quiet = true, onError = { grants.fail(access, it) }) { grants.accept(access, it) }
    }
    fun create(label: String) {
        if (!validAgentLabel(label.trim())) { client.showError("INVALID_AGENT_LABEL"); return }
        client.perform({ it.createAgent(label.trim()) }, key = "agents:create", onError = { load() }) { labelValue.value = ""; load() }
    }
    fun pause(agent: ManagedAgent, value: Boolean) = client.perform({ it.pauseAgent(agent.label, value) }, key = "agents:pause:" + agent.id, onError = { load() }) { load() }
    fun remove(agent: ManagedAgent) = client.perform({ it.removeAgent(agent.label) }, key = "agents:remove:" + agent.id, onError = { load() }) { permissionsValue.value -= agent.id; load(); client.navigation.back() }
    fun requestPermissions(agent: ManagedAgent) {
        val draft = permissionsValue.value[agent.id] ?: return
        if (!draft.valid || agentLines(draft.capabilities).any { !net.dmsg.core.validAgentCapability(it) }) return
        client.approvals.request("agent:permissions:" + agent.id) { it.requestAgentPermissions(agent.label, agentLines(draft.capabilities), draft.days.toULong()) }
    }
    fun revokeGrant(grant: AgentDeviceGrant) = client.perform({ it.revokeAgentPermissions(grant.id) }, key = "agents:revoke:" + grant.id, onError = { load() }) { load() }
    fun loadPrincipal() {
        client.account.devices()
        val read = principal.begin()
        client.perform({ parseAgentPrincipal(it.agentPrincipal().decode()) }, key = "agents:principal", replace = true, quiet = true, onError = { principal.fail(read, it) }) {
            principal.accept(read, it)
            if (it != null) loadCredentials() else credentials.reset()
        }
    }
    fun loadCredentials() {
        val read = credentials.begin()
        client.perform({ parseAgentCredentials(it.agentDelegations().decode()) }, key = "agents:credentials", replace = true, quiet = true, onError = { credentials.fail(read, it) }) { credentials.accept(read, it) }
    }
    fun enablePrincipal() = client.approvals.request("agent:enable") { it.enableAgentPrincipal() }
    fun registerController() {
        val form = delegationValue.value
        if (!form.validController) return
        client.approvals.request("agent:controller") { it.registerAgentController(form.name.trim(), if (form.unrestricted) emptyList() else agentLines(form.controller.scopes), if (form.unrestricted) emptyList() else agentLines(form.controller.audiences), form.unrestricted) }
    }
    fun retireController(generation: UInt) = client.approvals.request("agent:retire:" + generation) { it.retireAgentController(generation) }
    fun publish() = client.perform({ it.retryProfilePublication() }, key = "agents:publish", onError = { loadPrincipal() }) { loadPrincipal() }
    fun grantDelegation() {
        val form = delegationValue.value
        if (!form.validGrant) return
        client.approvals.request("agent:delegation") { it.grantAgentDelegation(form.subject.trim(), agentLines(form.grant.scopes), agentLines(form.grant.audiences), form.days.toULong()) }
    }
    fun revokeDelegation(id: String) = client.approvals.request("agent:revoke-delegation:" + id) { it.revokeAgentDelegation(id, null) }
    fun clear(retainForms: Boolean = false) {
        listing.reset(); quota.reset(); grants.reset(); principal.reset(); credentials.reset()
        if (!retainForms) { labelValue.value = ""; permissionsValue.value = emptyMap(); delegationValue.value = AgentDelegationDraft() }
    }
}
