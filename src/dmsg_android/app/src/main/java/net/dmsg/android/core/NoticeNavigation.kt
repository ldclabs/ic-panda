package net.dmsg.android.core

import kotlinx.serialization.json.*

/** A notification is an opaque lookup key, never a user-supplied account/action. */
@JvmInline value class NoticeRoute private constructor(val value: String) {
    companion object {
        fun parse(value: String?): NoticeRoute? {
            if (value == null) return null
            val id = value.removePrefix("local:")
            val size = if (value.startsWith("local:")) 64 else 128
            return value.takeIf { id.length == size && id.all { it in 'a'..'f' || it in '0'..'9' } }?.let(::NoticeRoute)
        }
    }
}
data class NoticeDestination(val account: String, val target: NoticeTarget, val ended: Boolean)
fun parseNoticeDestination(value: JsonElement): NoticeDestination = value.objectValue().let {
    NoticeDestination(it.required("account").also { account -> check(validAccount(account)) { "INTEGRITY_FAILED" } }, parseNoticeTarget(it["target"]!!), it.requiredFlag("ended"))
}

/** Resolving or choosing an account never acknowledges observation or approval. */
class NoticeNavigation(private val client: ClientController) {
    val destination = ReadStore<NoticeDestination>()
    private var requestRoute: NoticeRoute? = null
    private var owner: SessionStamp? = null
    private var intention = 0L
    private var source: ReminderItem? = null
    private var local: NoticeDestination? = null
    private var routed: Pair<Long, SessionStamp>? = null
    fun open(item: ReminderItem) {
        client.beginNavigationIntent()
        requestRoute = null; intention = client.navigationIntent; source = item; owner = client.currentStamp(); routed = null
        local = NoticeDestination(item.account, item.target, !item.open)
        destination.accept(destination.begin(false), local!!)
        client.persistNavigation()
        continueIfReady()
    }
    fun resolve(route: NoticeRoute, force: Boolean = false) {
        val stamp = client.currentStamp() ?: return
        val intent = client.navigationIntent
        if (!force && requestRoute == route && owner == stamp && intention == intent && destination.state.value !is ReadState.Idle) { continueIfReady(); return }
        requestRoute = route; owner = stamp; intention = intent; source = null; routed = null
        val request = destination.begin(false)
        client.perform({ parseNoticeDestination(it.notificationResolve(route.value).decode()) }, key = "notices:resolve", replace = true, quiet = true, onError = {
            if (intention == intent && client.navigationIntent == intent) destination.fail(request, it)
        }) {
            if (intention == intent && client.navigationIntent == intent) { destination.accept(request, it); continueIfReady() }
        }
    }
    fun retry() { requestRoute?.let { resolve(it, true) } }
    fun continueIfReady() {
        if (destination.state.value is ReadState.Idle) local?.let { destination.accept(destination.begin(false), it) }
        val value = destination.value() ?: return
        val stamp = client.currentStamp() ?: return
        if (value.ended || value.account != stamp.account || !client.accepts(stamp) || client.navigationIntent != intention || routed == (intention to stamp)) return
        routed = intention to stamp
        client.notifications.openTarget(value.account, value.target, source)
        local = null
        client.finishNoticeRoute()
        destination.reset(); source = null; requestRoute = null
    }
    fun switchAccount() { destination.value()?.takeIf { !it.ended }?.let { client.switchAccount(it.account) } }
    fun dismiss() { clear(discard = true); client.finishNoticeRoute() }
    /** Clear decrypted target metadata, retaining the root's opaque pending route. */
    fun save(): String? = local?.let { value -> buildJsonObject {
        put("account", value.account); put("target", Json.parseToJsonElement(value.target.json())); put("ended", value.ended)
    }.toString() }
    fun restore(saved: String?) {
        if (saved == null || saved.length > 2048) return
        local = runCatching { parseNoticeDestination(Json.parseToJsonElement(saved)) }.getOrNull()
        intention = if (client.navigationIntent == 0L) 1L else client.navigationIntent
    }
    fun clear(discard: Boolean = false) {
        client.cancelOperation("notices:resolve"); destination.reset(); requestRoute = null; source = null; owner = null; routed = null
        if (discard) local = null
    }
}
