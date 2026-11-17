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
        running = true
        client.perform({ core ->
            val preferences = parseNoticePreferences(core.notificationPreferences(stamp.account).decode())
            val batch = parseNoticeBatch(core.notificationNext(NotificationPresentationContext(system.allowed(), true, preferences.global.quiet?.until(Instant.now()), visible)).decode())
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
            if (presentedValue.value?.systemId in batch.remove) presentedValue.value = null
            for (plan in batch.plans) {
                val current = presentedValue.value
                val chosen = active && client.sessionCurrent(stamp) && !client.state.value.concealed && plan.deliveryId in eligible && previewPermits(plan) && plan.inApp && (current == null || current.systemId == plan.systemId || plan.category == "security" || current.category != "security")
                if (chosen) presentedValue.value = plan
                // A foreground consumer never produces a second background notification.
                report(plan, if (chosen) NotificationPresentationResult.SUBMITTED else NotificationPresentationResult.SUPPRESSED)
            }
            client.notifications.status()
            val wait = batch.nextDue?.let { (it.coerceAtMost(Long.MAX_VALUE.toULong()).toLong() - System.currentTimeMillis()).coerceIn(250, 60_000) } ?: 60_000
            schedule(wait)
            // Let ClientController release this operation's slot first.
            if (again) { again = false; scope.launch { yield(); if (active) refresh() } }
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
    fun open() { presentedValue.value?.let { client.requestNotice(checkNotNull(NoticeRoute.parse("local:" + it.systemId))) }; presentedValue.value = null }
    fun dismiss() { presentedValue.value = null }
    fun stop() { client.cancelOperation("notices:presentation"); client.cancelOperation("notices:receive"); active = false; running = false; receiving = false; again = false; wake?.cancel(); wake = null; presentedValue.value = null; testValue.value = null }
}
