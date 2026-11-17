package net.dmsg.android

import android.app.Notification
import android.app.NotificationManager
import android.os.Build
import android.os.ParcelFileDescriptor
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.core.NoticeRoute
import net.dmsg.android.core.noticeCategories
import net.dmsg.android.platform.SystemNotices
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SystemNoticeTest {
    @Test fun stableChannelsAndPrivateNotificationsDoNotStartAClientOrDuplicateAGroup() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext.applicationContext as DMsgApplication
        val before = app.hasMainClient
        val notices = SystemNotices(app)
        val manager = app.getSystemService(NotificationManager::class.java)
        notices.installChannels()
        val first = noticeCategories.associateWith { manager.getNotificationChannel(SystemNotices.channel(it)) }
        notices.installChannels()
        noticeCategories.forEach { category ->
            val current = manager.getNotificationChannel(SystemNotices.channel(category))
            assertEquals(first[category]!!.importance, current.importance)
            assertEquals(first[category]!!.sound, current.sound)
        }
        // This is the disposable QA package. Gradle removes it after the run;
        // revoking a runtime permission inside instrumentation can kill its UID.
        if (Build.VERSION.SDK_INT >= 33) ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand("pm grant " + app.packageName + " android.permission.POST_NOTIFICATIONS")).use { it.readBytes() }
        assertTrue(notices.allowed())
        val id = "af".repeat(32)
        val plan = SystemNotices.Presentation(id, NoticeRoute.parse("local:" + id)!!, "message", false, false, count = 7u)
        try {
            assertEquals(SystemNotices.Result.SUBMITTED, notices.post(plan))
            assertEquals(SystemNotices.Result.SUBMITTED, notices.post(plan.copy(update = true, count = 8u)))
            val until = android.os.SystemClock.elapsedRealtime() + 5000
            while (android.os.SystemClock.elapsedRealtime() < until && manager.activeNotifications.none { it.tag == id && it.notification.number == 8 }) Thread.sleep(50)
            val active = manager.activeNotifications.filter { it.tag == id }
            assertEquals(1, active.size)
            val value = active.single().notification
            assertEquals(Notification.VISIBILITY_PRIVATE, value.visibility)
            assertEquals(app.getString(R.string.notice_body_message), value.extras.getCharSequence(Notification.EXTRA_TEXT).toString())
            assertEquals(app.getString(R.string.notice_body_generic), value.publicVersion.extras.getCharSequence(Notification.EXTRA_TEXT).toString())
            assertEquals(0, value.publicVersion.number)
            assertEquals(app.packageName, value.contentIntent.creatorPackage)
            assertEquals(SystemNotices.Result.SUBMITTED, notices.test())
            val untilTest = android.os.SystemClock.elapsedRealtime() + 5000
            while (android.os.SystemClock.elapsedRealtime() < untilTest && manager.activeNotifications.none { it.tag == SystemNotices.TEST_ID }) Thread.sleep(50)
            assertEquals(app.getString(R.string.notice_test), manager.activeNotifications.single { it.tag == SystemNotices.TEST_ID }.notification.extras.getCharSequence(Notification.EXTRA_TEXT).toString())
            assertEquals(before, app.hasMainClient)
        } finally { notices.remove(listOf(id, SystemNotices.TEST_ID)) }
    }
}
