package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.core.ChannelMemberChange
import java.util.Base64
import java.util.UUID

data class ChannelMember(val account: String, val role: String, val canRotate: Boolean, val joinedEpoch: ULong, val history: List<Pair<ULong, ULong>>)
data class ChannelDetails(val id: String, val name: String, val muted: Boolean, val hidden: Boolean, val mutedUntil: ULong?, val notificationPending: Boolean,
    val notificationConflict: Boolean, val kind: String?, val status: String?, val epoch: ULong?, val head: String?, val members: List<ChannelMember>, val readableEpochs: List<ULong>, val left: Boolean) {
    fun member(account: String?) = members.find { it.account == account }
    fun canManage(account: String?) = !left && status != "archived" && member(account)?.role in setOf("owner", "admin")
    fun canRemove(actor: String?, target: ChannelMember) = !left && target.account != actor && status != "archived" &&
        (member(actor)?.role == "owner" || member(actor)?.role == "admin" && target.role in setOf("member", "publisher"))
}
data class ChannelPending(val key: String, val action: String, val request: String, val deadline: ULong)
data class ChannelCreation(val request: String, val channel: String, val name: String, val kind: String, val createdAt: ULong)
data class ChannelReview(val details: ChannelDetails, val pending: List<ChannelPending>)
data class ChannelAction(val action: String, val account: String? = null, val role: String? = null, val canRotate: Boolean = false)
data class PendingChannelChange(val action: ChannelAction, val request: String = freshChannelRequest())
data class ChannelSetup(val joining: Boolean = false, val name: String = "", val kind: String = "collaboration", val invitation: String = "", val request: String? = null, val error: String? = null, val invitee: String? = null)
data class InvitationAttempt(val request: String, val account: String, val expiresAt: ULong, val link: String?)
data class InvitationAttempts(val channel: String, val items: List<InvitationAttempt>)
data class ChannelInvite(val account: String = "", val request: String? = null, val link: String? = null, val error: String? = null)
fun freshChannelRequest(): String = (UUID.randomUUID().toString() + UUID.randomUUID().toString()).replace("-", "")
fun validChannelName(value: String) = value.trim().isNotEmpty() && value.codePointCount(0, value.length) <= 100 && value.none { it.isISOControl() }
private fun JsonElement.u64() = buildJsonObject { put("v", this@u64) }.requiredUnsigned("v")
fun parseChannelDetails(value: JsonElement): ChannelDetails {
    val row = value.objectValue(); val ledger = row["ledger"]?.takeIf { it != JsonNull }?.objectValue()
    val members = ledger?.get("members")?.objectValue()?.map { (account, value) ->
        val m = value.objectValue(); check(m.required("account") == account) { "INTEGRITY_FAILED" }
        ChannelMember(account, m.required("role"), m.requiredFlag("can_rotate"), m.requiredUnsigned("joined_epoch"),
            (m["history"] as? JsonArray ?: error("INTEGRITY_FAILED")).map { pair -> val a = pair as? JsonArray ?: error("INTEGRITY_FAILED"); check(a.size == 2); a[0].u64() to a[1].u64() })
    }.orEmpty()
    return ChannelDetails(row.required("id"), row.required("name"), row.requiredFlag("muted"), row.requiredFlag("hidden"), row.unsigned("muted_until"),
        row.requiredFlag("notification_pending"), row.requiredFlag("notification_conflict"), ledger?.required("type"), ledger?.required("status"), ledger?.requiredUnsigned("epoch"), ledger?.required("head"), members,
        (row["readable_epochs"] as? JsonArray ?: error("INTEGRITY_FAILED")).map { it.u64() }, row.requiredFlag("left"))
}
fun parseChannelPending(value: JsonElement) = value.arrayRows().map { ChannelPending(it.required("key"), it.required("action"), it.required("request_id"), it.requiredUnsigned("deadline")) }
fun parseChannelCreations(value: JsonElement) = value.arrayRows().map { ChannelCreation(it.required("request"), it.required("channel"), it.required("name"), it.required("kind"), it.requiredUnsigned("created_at")) }

/** These are link claims, never a verified membership or an authorization. */
data class ChannelInvitation(val channel: String, val account: String, val name: String, val expiresAt: ULong) {
    companion object {
        fun parse(input: String): ChannelInvitation? = runCatching {
            check(input.toByteArray(Charsets.UTF_8).size <= 16 * 1024)
            val trimmed = input.trim()
            val json = if (trimmed.startsWith("dmsg://")) {
                check(trimmed.startsWith("dmsg://channel/join#"))
                val fragment = trimmed.removePrefix("dmsg://channel/join#")
                check(fragment.matches(Regex("[A-Za-z0-9_-]+")))
                Base64.getUrlDecoder().decode(fragment).toString(Charsets.UTF_8)
            } else trimmed
            val row = Json.parseToJsonElement(json).objectValue()
            check(row.required("format") == "dmsg-channel-invitation/1")
            listOf("channel", "genesis", "head", "invitation").forEach { check(row.required(it).matches(Regex("[0-9a-f]{64}"))) }
            check(validAccount(row.required("account")))
            check(validChannelName(row.required("name")))
            ChannelInvitation(row.required("channel"), row.required("account"), row.required("name"), row.requiredUnsigned("expires_at"))
        }.getOrNull()
    }
}

class ChannelCoordinator(private val client: ClientController) {
    val review = ReadStore<ChannelReview>()
    val hidden = ReadStore<List<ChannelDetails>> { it.isEmpty() }
    val creations = ReadStore<List<ChannelCreation>> { it.isEmpty() }
    val invitationAttempts = ReadStore<InvitationAttempts> { it.items.isEmpty() }
    private val namesValue = MutableStateFlow<Map<String, String>>(emptyMap())
    val names = namesValue.asStateFlow()
    private val changesValue = MutableStateFlow<Map<String, PendingChannelChange>>(emptyMap())
    val changes = changesValue.asStateFlow()
    private val setupValue = MutableStateFlow(ChannelSetup())
    val setup = setupValue.asStateFlow()
    private val invitesValue = MutableStateFlow<Map<String, ChannelInvite>>(emptyMap())
    val invites = invitesValue.asStateFlow()
    private var selected: String? = null
    fun load(channel: String) {
        val same = selected == channel; selected = channel
        val request = review.begin(same)
        client.perform({ core -> ChannelReview(parseChannelDetails(core.channelDetails(channel).decode()), parseChannelPending(core.pendingChannelOperations(channel).decode())) },
            key = "channel:review", replace = true, quiet = true, onError = { review.fail(request, it) }) {
            review.accept(request, it); if (channel !in namesValue.value) namesValue.value += channel to it.details.name
        }
    }
    fun browseHidden() { val request = hidden.begin(); client.perform({ core -> core.hiddenChannels().decode().arrayRows().map(::parseChannelDetails) }, key = "channel:hidden", replace = true, onError = { hidden.fail(request, it) }) { hidden.accept(request, it) } }
    fun editName(channel: String, name: String) { client.activity(); namesValue.value += channel to name }
    fun saveName(channel: String) {
        val original = review.value()?.takeIf { it.details.id == channel }?.details ?: return
        val name = namesValue.value[channel]?.trim() ?: return
        if (!validChannelName(name)) return
        client.perform({ parseChannelDetails(it.renameChannel(channel, original.name, name).decode()) }, key = "channel:settings:$channel", onError = { load(channel) }) {
            if (namesValue.value[channel]?.trim() == name) namesValue.value -= channel
            updated(channel)
        }
    }
    fun mute(channel: String, muted: Boolean, until: ULong? = null) = client.perform({ core -> if (muted) core.channelMuteUntil(channel, until) else core.setChannelMuted(channel, false) }, key = "channel:settings:$channel") { updated(channel) }
    fun hide(channel: String, hidden: Boolean) = client.perform({ it.setChannelHidden(channel, hidden) }, key = "channel:settings:$channel") { updated(channel); browseHidden() }
    private fun updated(channel: String) { if (selected == channel) load(channel); client.messages.browse() }
    fun change(channel: String, action: ChannelAction) {
        val pending = changesValue.value[channel] ?: PendingChannelChange(action).also { changesValue.value += channel to it }
        if (pending.action != action) return
        client.perform({ core -> client.operation { core.channelMemberAction(ChannelMemberChange(channel, action.action, action.account, action.role, action.canRotate, pending.request), it) } },
            key = "channel:control:$channel", onError = { load(channel) }) { changesValue.value -= channel; updated(channel) }
    }
    fun resume(channel: String, key: String) = client.perform({ it.resumeChannelOperation(channel, key) }, key = "channel:control:$channel", onError = { load(channel) }) {
        changesValue.value[channel]?.takeIf { key == "control:${it.request}" }?.let { changesValue.value -= channel }; updated(channel)
    }
    fun editSetup(value: ChannelSetup) { client.activity(); setupValue.value = value.copy(error = null) }
    fun openSetup(joining: Boolean, invitation: String? = null, invitee: String? = null) {
        setupValue.value = setupValue.value.copy(joining = joining, invitation = invitation ?: setupValue.value.invitation, invitee = invitee, error = null)
        client.navigation.push(Route("channel-setup")); loadCreations()
    }
    fun loadCreations() { val request = creations.begin(); client.perform({ parseChannelCreations(it.channelCreations().decode()) }, key = "channel:creations", replace = true, quiet = true, onError = { creations.fail(request, it) }) { creations.accept(request, it) } }
    fun completeSetup() {
        val form = setupValue.value
        if (form.joining) {
            val invitation = ChannelInvitation.parse(form.invitation) ?: return
            if (invitation.account != client.state.value.session?.account || invitation.expiresAt <= System.currentTimeMillis().toULong()) return
            client.perform({ core -> client.operation { core.joinChannel(form.invitation, it) } }, key = "channel:setup", quiet = true, onError = { setupValue.value = setupValue.value.copy(error = it) }) {
                setupValue.value = ChannelSetup(); client.consumeChannelLink(form.invitation); client.messages.browse()
                if (client.navigation.state.value.current.page == "channel-setup") { client.navigation.back(); client.navigation.open("messages"); client.navigation.select(invitation.channel) }
            }
        } else {
            if (!validChannelName(form.name)) return
            val request = form.request ?: freshChannelRequest()
            setupValue.value = form.copy(request = request, error = null)
            create(ChannelCreation(request, "", form.name.trim(), form.kind, 0u))
        }
    }
    fun create(intent: ChannelCreation) = client.perform({ core -> client.operation { core.createChannelIntent(intent.name, intent.kind, intent.request, it) } }, key = "channel:setup", quiet = true, onError = { setupValue.value = setupValue.value.copy(error = it); loadCreations() }) { channel ->
        val invitee = setupValue.value.invitee
        if (invitee != null) editInvite(channel, ChannelInvite(invitee))
        setupValue.value = ChannelSetup(); loadCreations(); client.messages.browse()
        if (client.navigation.state.value.current.page == "channel-setup") {
            client.navigation.back(); client.navigation.open("messages"); client.navigation.select(channel); client.navigation.push(Route("channel-invite", channel = channel))
        }
    }
    fun discard(intent: ChannelCreation) = client.perform({ it.discardChannelCreation(intent.request) }, key = "channel:setup") {
        if (setupValue.value.request == intent.request) setupValue.value = setupValue.value.copy(request = null, error = null)
        loadCreations()
    }
    fun loadInvites(channel: String) {
        val request = invitationAttempts.begin(invitationAttempts.value()?.channel == channel)
        client.perform({ core -> InvitationAttempts(channel, core.channelInvitationAttempts(channel).decode().arrayRows().map { InvitationAttempt(it.required("request"), it.required("account"), it.requiredUnsigned("expires_at"), it.text("link")) }) },
            key = "channel:invitations", replace = true, quiet = true, onError = { invitationAttempts.fail(request, it) }) { invitationAttempts.accept(request, it) }
    }
    fun continueInvite(channel: String, attempt: InvitationAttempt) {
        editInvite(channel, ChannelInvite(attempt.account, attempt.request, attempt.link))
        if (attempt.link == null) invite(channel)
    }
    fun editInvite(channel: String, value: ChannelInvite) { client.activity(); invitesValue.value += channel to value.copy(error = null) }
    fun invite(channel: String) {
        val form = invitesValue.value[channel] ?: return
        if (!validAccount(form.account) || form.link != null) return
        val request = form.request ?: freshChannelRequest()
        invitesValue.value += channel to form.copy(request = request, error = null)
        client.perform({ core -> client.operation { core.inviteChannelIntent(channel, form.account, request, it) } }, key = "channel:invite:$channel", quiet = true, onError = {
            invitesValue.value += channel to form.copy(request = request, error = it); load(channel); loadInvites(channel)
        }) { link -> invitesValue.value += channel to form.copy(request = request, link = link); updated(channel); loadInvites(channel) }
    }
    fun clear(retainForms: Boolean = false) {
        review.reset(); hidden.reset(); creations.reset(); invitationAttempts.reset(); selected = null
        if (!retainForms) { namesValue.value = emptyMap(); changesValue.value = emptyMap(); setupValue.value = ChannelSetup(); invitesValue.value = emptyMap() }
    }
}
