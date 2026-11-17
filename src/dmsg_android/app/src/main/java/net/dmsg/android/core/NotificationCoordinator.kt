package net.dmsg.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.core.ReminderScope
import kotlinx.serialization.json.JsonNull

class NotificationCoordinator(private val client: ClientController, private val scope: CoroutineScope) {
    val preferences = ReadStore<NoticePreferences>()
    val status = ReadStore<NoticeStatus>()
    val reminders = ReadStore<ReminderPage> { it.items.isEmpty() }
    private val draftValue = MutableStateFlow<NoticePreferences?>(null)
    val draft = draftValue.asStateFlow()
    private val errorValue = MutableStateFlow<String?>(null)
    val saveError = errorValue.asStateFlow()
    private val recentValue = MutableStateFlow(false)
    val recent = recentValue.asStateFlow()
    private var dirty = false
    private var saving = false
    private var saveAgain: Job? = null
    private var saved: NoticePreferences? = null
    private val openedValue = MutableStateFlow<ReminderItem?>(null)
    val opened = openedValue.asStateFlow()
    private var openedTarget: Pair<String, NoticeTarget>? = null
    private val observing = mutableSetOf<String>()
    private class Viewport(val target: NoticeTarget, var through: ULong?, var active: () -> Boolean, var item: ReminderItem? = null)
    private val viewports = mutableMapOf<String, Viewport>()
    fun visible(): List<net.dmsg.core.VisibleReminder> = viewports.values.filter { it.active() }.mapNotNull { view -> view.item?.let { net.dmsg.core.VisibleReminder(it.id, it.version.toString()) } }.distinctBy { it.id }.take(32)
    fun hide(owner: String) { viewports.remove(owner); observing.remove(owner); client.cancelOperation("notices:observed:" + owner) }


    fun open(item: ReminderItem) = client.noticeNavigation.open(item)
    internal fun openTarget(account: String, target: NoticeTarget, item: ReminderItem? = null) {
        if (!client.state.value.unlocked || client.state.value.session?.account != account) return
        val destination = account to target
        openedTarget = destination
        openedValue.value = item?.takeIf { it.account == account && it.target == target }
        // A missing reminder projection must not block the actual business page.
        client.perform({ core -> core.notificationReference(account, target.json()).decode().let { if (it == JsonNull) null else parseReminder(it) } }, key = "notices:opened", replace = true, quiet = true) {
            if (openedTarget == destination) openedValue.value = it?.takeIf { current -> current.account == account && current.target == target }
        }
        when (target.kind) {
            "channel" -> client.navigation.push(Route("messages", id = target.id))
            "inbox" -> {
                client.letters.rememberLocation(LetterLocation(selected = target.id), client.currentStamp())
                client.navigation.push(Route("inbox"))
            }
            "approval" -> client.approvals.open(target.id!!)
            "plan" -> client.navigation.push(Route("membership"))
            "wallet" -> client.navigation.push(Route("wallet-proposal", target.wallet, sequence = canonicalNoticeVersion(target.id!!)))
            "recovery" -> client.navigation.push(Route("recovery", id = target.id))
            "task" -> client.navigation.push(Route(if (target.id!!.startsWith("conflicts:")) "conflicts" else "sync"))
            "service" -> client.navigation.push(Route(if (target.id!!.startsWith("network:")) "sync" else "notifications"))
            "summary" -> { selectRecent(target.category in setOf("message", "service")); client.navigation.push(Route("reminders")) }
            else -> { openedValue.value = null; client.showError("REMINDER_UNAVAILABLE") }
        }
    }
    /** Reference lookup is independent of how the person reached the page. */
    fun observed(target: NoticeTarget, through: ULong? = null, owner: String = target.json(), active: () -> Boolean = { true }) {
        val stamp = client.currentStamp() ?: return
        val account = stamp.account ?: return
        if (!client.accepts(stamp) || !active() || target.kind == "channel" && through == null) return
        val viewport = viewports[owner]?.takeIf { it.target == target }?.also { it.through = through; it.active = active }
            ?: Viewport(target, through, active).also { viewports[owner] = it }
        if (!observing.add(owner)) return
        client.perform({ core ->
            val value = core.notificationReference(account, target.json()).decode()
            val item = if (value == JsonNull) null else parseReminder(value)
            val eligible = withContext(Dispatchers.Main.immediate) {
                client.accepts(stamp) && viewports[owner] === viewport && viewport.active() && item != null && item.account == account && item.target == target && item.open && !item.unverified && viewport.through?.let { it >= item.version } != false &&
                    (target.kind != "channel" || viewport.through?.let { it >= item.version } == true)
            }
            if (!eligible || item == null) return@perform null
            val changed = item.observed < item.version
            if (changed) core.notificationObserved(account, item.id, item.version.toString())
            item to changed
        }, key = "notices:observed:" + owner, quiet = true, onError = { observing.remove(owner) }) { result ->
            observing.remove(owner)
            if (viewports[owner] !== viewport || !viewport.active() || result == null) return@perform
            val (item, changed) = result
            viewport.item = item.copy(observed = maxOf(item.observed, item.version))
            if (openedValue.value?.id == item.id) openedValue.value = viewport.item
            if (changed) status()
        }
    }

    fun load() {
        val account = client.state.value.session?.account ?: return
        val request = preferences.begin()
        client.perform({ parseNoticePreferences(it.notificationPreferences(account).decode()) }, key = "notices:preferences", replace = true, quiet = true, onError = { preferences.fail(request, it) }) {
            check(it.accountId == account) { "INTEGRITY_FAILED" }
            preferences.accept(request, it); saved = it
            if (!dirty) draftValue.value = it
        }
        status()
    }
    fun status() {
        val request = status.begin()
        client.perform({ parseNoticeStatus(it.notificationStatus().decode()) }, key = "notices:status", replace = true, quiet = true, onError = { status.fail(request, it) }) { status.accept(request, it) }
    }
    fun edit(global: NoticeGlobal? = null, account: NoticeAccount? = null) {
        val current = draftValue.value ?: return
        client.activity()
        draftValue.value = current.copy(global = global ?: current.global, account = account ?: current.account)
        dirty = true
        client.push.changed(checkNotNull(draftValue.value))
        if (errorValue.value == null) save()
    }
    fun retrySave() { errorValue.value = null; save() }
    private fun save() {
        val original = saved ?: return; val submitted = draftValue.value ?: return
        if (!dirty || saving || !submitted.global.valid || original.accountId != client.state.value.session?.account) return
        saving = true
        client.perform({ core ->
            if (submitted.global != original.global || original.globalConflict) core.notificationSetGlobal(original.global.revision, submitted.global.json())
            if (submitted.account != null && original.account != null && (submitted.account != original.account || original.accountConflict)) core.notificationSetAccount(original.accountId!!, original.account.revision, submitted.account.json())
            parseNoticePreferences(core.notificationPreferences(original.accountId).decode())
        }, key = "notices:save", quiet = true, onError = {
            saving = false; errorValue.value = it
            // A global save may have succeeded before an account CAS failed. Reload each revision,
            // retain the edit, and require explicit review/retry instead of silently overwriting.
            load()
        }) { value ->
            saving = false; saved = value
            val request = preferences.begin(); preferences.accept(request, value)
            if (draftValue.value == submitted) { dirty = false; draftValue.value = value }
            else {
                draftValue.value = draftValue.value?.let { current -> current.copy(global = current.global.copy(revision = value.global.revision), account = current.account?.copy(revision = value.account!!.revision, remoteRevision = value.account.remoteRevision)) }
                val stamp = client.currentStamp()
                saveAgain = scope.launch { yield(); if (stamp != null && client.accepts(stamp)) save() }
            }
            status(); client.notices.refresh(); client.push.reconcile(force = true)
        }
    }
    fun selectRecent(value: Boolean) { if (recentValue.value != value) { recentValue.value = value; reminders.reset() }; list() }
    fun list(more: Boolean = false) {
        val account = client.state.value.session?.account ?: return
        val page = reminders.value(); val offset = if (more) page?.next ?: return else 0u
        val recent = recentValue.value
        val request = reminders.begin()
        client.perform({ parseReminderPage(it.notificationItems(account, if (recent) ReminderScope.RECENT else ReminderScope.NEEDS_ACTION, offset).decode()) }, key = "notices:list", replace = true, quiet = true, onError = { reminders.fail(request, it) }) { value ->
            check(value.items.all { it.account == account }) { "INTEGRITY_FAILED" }
            reminders.accept(request, if (more) value.copy(items = (page!!.items + value.items).distinctBy { it.id }) else value)
        }
        status()
    }
    fun refresh() = client.perform({ it.notificationRefresh() }, key = "notices:refresh", quiet = true, onError = { list(); load() }) { list(); load() }
    fun markRead(item: ReminderItem) = client.perform({ it.notificationMarkRead(item.account, item.id, item.version.toString()) }, key = "notices:read:" + item.id) { list() }
    fun markAllRead() = client.perform({ it.notificationMarkAllRead() }, key = "notices:read-all") { list() }
    fun clearRecent() {
        val account = client.state.value.session?.account ?: return
        client.perform({ it.notificationClearRecent(account) }, key = "notices:clear") { list() }
    }
    fun snooze(item: ReminderItem, until: ULong) = client.perform({ it.notificationSnooze(item.account, item.id, item.version.toString(), until) }, key = "notices:snooze:" + item.id) { list() }
    fun clear(retainDraft: Boolean = false) {
        preferences.reset(); status.reset(); reminders.reset(); saving = false; saveAgain?.cancel(); saveAgain = null; observing.clear(); viewports.clear(); openedValue.value = null; openedTarget = null
        if (!retainDraft) { draftValue.value = null; errorValue.value = null; dirty = false; saved = null; recentValue.value = false }
    }
}
