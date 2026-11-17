package net.dmsg.android.platform

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import net.dmsg.android.MainActivity
import net.dmsg.android.R
import net.dmsg.android.core.NoticeRoute
import net.dmsg.android.core.noticeCategories
import net.dmsg.android.core.validNoticeHash

/** Stable OS groups. Only public vocabulary and opaque navigation cross here. */
class SystemNotices(private val context: Context) {
    enum class Result { SUBMITTED, SUPPRESSED, FAILED }
    data class Presentation(val id: String, val route: NoticeRoute, val category: String, val sound: Boolean, val update: Boolean, val generic: Boolean = false, val count: ULong? = null, val registration: String? = null)
    private val manager = context.getSystemService(NotificationManager::class.java)
    fun allowed() = NotificationManagerCompat.from(context).areNotificationsEnabled()
    fun refreshExistingChannelNames() {
        for (category in noticeCategories) {
            val existing = manager.getNotificationChannel(channel(category)) ?: continue
            existing.name = AppLanguage.wrap(context).getString(name(category))
            manager.createNotificationChannel(existing)
        }
    }
    fun installChannels() {
        for (category in noticeCategories) {
            val name = AppLanguage.wrap(context).getString(name(category))
            val channel = manager.getNotificationChannel(channel(category))?.apply { this.name = name }
                ?: NotificationChannel(channel(category), name, if (category == "security") NotificationManager.IMPORTANCE_HIGH else NotificationManager.IMPORTANCE_DEFAULT).apply { lockscreenVisibility = Notification.VISIBILITY_PRIVATE }
            manager.createNotificationChannel(channel)
        }
    }
    fun body(category: String, generic: Boolean = false) = AppLanguage.wrap(context).getString(if (generic) R.string.notice_body_generic else when (category) {
        "security" -> R.string.notice_body_security
        "action" -> R.string.notice_body_action
        "message" -> R.string.notice_body_message
        "service" -> R.string.notice_body_service
        else -> error("INVALID_INPUT")
    })
    fun post(value: Presentation): Result {
        check(validNoticeHash(value.id) && value.category in noticeCategories) { "INVALID_INPUT" }
        installChannels()
        if (!allowed() || manager.getNotificationChannel(channel(value.category))?.importance == NotificationManager.IMPORTANCE_NONE) return Result.SUPPRESSED
        val intent = Intent(context, MainActivity::class.java).setAction(OPEN)
            .setData(Uri.Builder().scheme("dmsg-notice").authority("open").appendPath(value.route.value).build())
            .putExtra("version", 1).putExtra("route", value.route.value).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_NEW_TASK)
        val pending = PendingIntent.getActivity(context, 0, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val public = NotificationCompat.Builder(context, channel(value.category)).setSmallIcon(R.drawable.dmsg_notification)
            .setContentTitle("dMsg").setContentText(body(value.category, true)).setVisibility(NotificationCompat.VISIBILITY_PUBLIC).build()
        val notice = NotificationCompat.Builder(context, channel(value.category)).setSmallIcon(R.drawable.dmsg_notification)
            .setContentTitle("dMsg").setContentText(body(value.category, value.generic)).setContentIntent(pending)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE).setPublicVersion(public).setAutoCancel(true)
            .setSilent(!value.sound || value.update).setOnlyAlertOnce(value.update)
            .setCategory(if (value.category == "message") NotificationCompat.CATEGORY_MESSAGE else NotificationCompat.CATEGORY_STATUS)
        value.registration?.let { notice.addExtras(android.os.Bundle().apply { putString("dmsg.registration", it) }) }
        value.count?.takeIf { !value.generic }?.let { notice.setNumber(it.coerceAtMost(Int.MAX_VALUE.toULong()).toInt()) }
        return try { manager.notify(value.id, 0, notice.build()); Result.SUBMITTED }
        catch (_: SecurityException) { Result.SUPPRESSED }
        catch (_: RuntimeException) { Result.FAILED }
    }
    fun removeRegistration(registration: String) { manager.activeNotifications.filter { it.notification.extras.getString("dmsg.registration") == registration }.forEach { manager.cancel(it.tag, it.id) } }
    fun remove(ids: List<String>) { ids.filter(::validNoticeHash).forEach { manager.cancel(it, 0) } }
    fun test(): Result {
        installChannels()
        if (!allowed() || manager.getNotificationChannel(channel("service"))?.importance == NotificationManager.IMPORTANCE_NONE) return Result.SUPPRESSED
        val pending = PendingIntent.getActivity(context, 0, WorkspaceEntries.intent(context, "notifications"), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val public = NotificationCompat.Builder(context, channel("service")).setSmallIcon(R.drawable.dmsg_notification)
            .setContentTitle("dMsg").setContentText(body("service", true)).build()
        val notice = NotificationCompat.Builder(context, channel("service")).setSmallIcon(R.drawable.dmsg_notification)
            .setContentTitle("dMsg").setContentText(AppLanguage.wrap(context).getString(R.string.notice_test)).setContentIntent(pending)
            .setAutoCancel(true).setSilent(true).setVisibility(NotificationCompat.VISIBILITY_PRIVATE).setPublicVersion(public).build()
        return try { manager.notify(TEST_ID, 0, notice); Result.SUBMITTED }
        catch (_: SecurityException) { Result.SUPPRESSED }
        catch (_: RuntimeException) { Result.FAILED }
    }
    fun settings(category: String? = null) = Intent(if (category == null) android.provider.Settings.ACTION_APP_NOTIFICATION_SETTINGS else android.provider.Settings.ACTION_CHANNEL_NOTIFICATION_SETTINGS)
        .putExtra(android.provider.Settings.EXTRA_APP_PACKAGE, context.packageName).apply { category?.let { putExtra(android.provider.Settings.EXTRA_CHANNEL_ID, channel(it)) } }
    companion object {
        const val OPEN = "net.dmsg.android.OPEN_REMINDER"
        val TEST_ID = SystemExportStore.digest("dmsg/android/notification-test/1".toByteArray())
        fun channel(category: String) = "dmsg." + category.also { require(it in noticeCategories) } + ".v1"
        private fun name(category: String) = when (category) { "security" -> R.string.notice_channel_security; "action" -> R.string.notice_channel_action; "message" -> R.string.notice_channel_message; "service" -> R.string.notice_channel_service; else -> error("INVALID_INPUT") }
    }
}
