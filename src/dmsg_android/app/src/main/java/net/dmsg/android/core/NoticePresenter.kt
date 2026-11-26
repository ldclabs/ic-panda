package net.dmsg.android.core

import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.android.platform.SystemNotices
import net.dmsg.core.NotificationPresentationContext
import net.dmsg.core.NotificationPresentationResult
import java.time.Instant

/** The shared core owns budgets, grouping and phases. Android owns delivery. */
class NoticePresenter(private val client: ClientController, private val context: Context, private val scope: CoroutineScope) {
    val system = SystemNotices(context)
    private val presentedValue = MutableStateFlow<NoticeDelivery?>(null)
    val presented = presentedValue.asStateFlow()
    private val testValue = MutableStateFlow<SystemNotices.Result?>(null)
    val testResult = testValue.asStateFlow()
    fun test() { client.activity(); testValue.value = system.test() }
    private var wake: Job? = null
    private var dismissTimer: Job? = null
    private var running = false
    private var again = false
    private var receiving = false
    private var active = false
    fun start() { if (active) return; active = true; refresh(receive = true) }
    fun refresh(receive: Boolean = false) {
        if (!active || client.state.value.starting || client.state.value.concealed) return
        if (receive && !receiving) {
            receiving = true
            client.perform({ it.notificationRefresh() }, key = "notices:receive", quiet = true, onError = { receiving = false; refresh() }) { receiving = false; refresh() }
        }
        if (running) { again = true; return }
        val stamp = client.currentStamp() ?: return
        val visible = client.notifications.visible()
        val foreground = client.isAppForeground && !client.state.value.concealed
        running = true
        client.perform({ core ->
            val preferences = parseNoticePreferences(core.notificationPreferences(stamp.account).decode())
            val batch = parseNoticeBatch(core.notificationNext(NotificationPresentationContext(system.allowed(), foreground, preferences.global.quiet?.until(Instant.now()), if (foreground) visible else emptyList())).decode())
            val eligible = mutableSetOf<String>()
            for (plan in batch.plans) {
                val current = parseNoticePreferences(core.notificationPreferences(plan.account).decode())
                val now = Instant.now()
                val quiet = current.global.quiet?.until(now) != null && !(plan.category == "security" && current.global.securityException)
                if (current.global.enabled && current.account?.enabled == true && current.account.categories[plan.category]?.enabled == true && current.global.pausedUntil?.let { it > now.toEpochMilli().toULong() } != true && !quiet)
                    eligible += plan.deliveryId
            }
            batch to eligible
        }, key = "notices:presentation", quiet = true, onError = {
            running = false
            if (active) schedule(30_000)
        }) { (batch, eligible) ->
            running = false
            system.remove(batch.remove)
            if (presentedValue.value?.systemId in batch.remove) { dismissTimer?.cancel(); dismissTimer = null; presentedValue.value = null }
            for (plan in batch.plans) {
                val allowed = plan.deliveryId in eligible && previewPermits(plan)
                if (plan.inApp) {
                    val current = presentedValue.value
                    val chosen = active && client.isAppForeground && client.sessionCurrent(stamp) && !client.state.value.concealed && allowed && (current == null || current.systemId == plan.systemId || plan.category == "security" || current.category != "security")
                    if (chosen) presentInApp(plan)
                    report(plan, if (chosen) NotificationPresentationResult.SUBMITTED else NotificationPresentationResult.SUPPRESSED)
                } else {
                    val result = if (!allowed || (client.isAppForeground && !client.state.value.concealed)) NotificationPresentationResult.SUPPRESSED else postLocal(plan)
                    report(plan, result)
                }
            }
            client.notifications.status()
            val wait = batch.nextDue?.let { (it.coerceAtMost(Long.MAX_VALUE.toULong()).toLong() - System.currentTimeMillis()).coerceIn(250, 60_000) } ?: 60_000
            schedule(wait)
            if (again) { again = false; if (active) refresh() }
        }
    }
    /** Deliver pending local-only reminders (approvals, sync tasks, service) when backgrounding. */
    suspend fun deliverBackgroundLocal(core: net.dmsg.core.MobileEngine) {
        val preferences = parseNoticePreferences(core.notificationPreferences(null).decode())
        val batch = parseNoticeBatch(core.notificationNext(NotificationPresentationContext(system.allowed(), false, preferences.global.quiet?.until(Instant.now()), emptyList())).decode())
        system.remove(batch.remove)
        val now = Instant.now()
        for (plan in batch.plans) {
            if (plan.inApp) {
                core.notificationResult(plan.account, plan.systemId, plan.deliveryId, NotificationPresentationResult.SUPPRESSED)
                continue
            }
            val current = parseNoticePreferences(core.notificationPreferences(plan.account).decode())
            val quiet = current.global.quiet?.until(now) != null && !(plan.category == "security" && current.global.securityException)
            val allowed = current.global.enabled && current.account?.enabled == true && current.account.categories[plan.category]?.enabled == true && current.global.pausedUntil?.let { it > now.toEpochMilli().toULong() } != true && !quiet
            val outcome = if (allowed) postLocal(plan) else NotificationPresentationResult.SUPPRESSED
            core.notificationResult(plan.account, plan.systemId, plan.deliveryId, outcome)
        }
    }
    private fun postLocal(plan: NoticeDelivery): NotificationPresentationResult {
        val route = NoticeRoute.parse("local:" + plan.systemId) ?: return NotificationPresentationResult.FAILED
        return when (system.post(SystemNotices.Presentation(plan.systemId, route, plan.category, plan.sound, plan.update, plan.generic, plan.count))) {
            SystemNotices.Result.SUBMITTED -> NotificationPresentationResult.SUBMITTED
            SystemNotices.Result.SUPPRESSED -> NotificationPresentationResult.SUPPRESSED
            SystemNotices.Result.FAILED -> NotificationPresentationResult.FAILED
        }
    }
    private fun presentInApp(plan: NoticeDelivery) {
        presentedValue.value = plan
        dismissTimer?.cancel()
        dismissTimer = scope.launch {
            delay(6_000)
            if (presentedValue.value?.deliveryId == plan.deliveryId) presentedValue.value = null
        }
    }
    private fun previewPermits(plan: NoticeDelivery): Boolean {
        val draft = client.notifications.draft.value ?: return true
        val now = Instant.now()
        if (!draft.global.enabled || draft.global.pausedUntil?.let { it > now.toEpochMilli().toULong() } == true || draft.global.quiet?.until(now) != null && !(plan.category == "security" && draft.global.securityException)) return false
        return draft.accountId != plan.account || draft.account?.let { it.enabled && it.categories[plan.category]?.enabled == true } != false
    }
    private fun report(plan: NoticeDelivery, result: NotificationPresentationResult) = client.perform({ it.notificationResult(plan.account, plan.systemId, plan.deliveryId, result) }, key = "notices:result:" + plan.deliveryId, quiet = true) {}
    private fun schedule(waitMs: Long) { wake?.cancel(); wake = scope.launch { delay(waitMs); if (active) refresh(receive = true) } }
    fun open() { dismissTimer?.cancel(); dismissTimer = null; presentedValue.value?.let { client.requestNotice(checkNotNull(NoticeRoute.parse("local:" + it.systemId))) }; presentedValue.value = null }
    fun dismiss() { dismissTimer?.cancel(); dismissTimer = null; presentedValue.value = null }
    fun stop() { client.cancelOperation("notices:presentation"); client.cancelOperation("notices:receive"); active = false; running = false; receiving = false; again = false; wake?.cancel(); wake = null; dismissTimer?.cancel(); dismissTimer = null; presentedValue.value = null; testValue.value = null }
}
