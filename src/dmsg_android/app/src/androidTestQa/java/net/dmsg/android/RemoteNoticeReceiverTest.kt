package net.dmsg.android

import android.app.Notification
import android.app.NotificationManager
import android.os.Build
import android.os.ParcelFileDescriptor
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.serialization.json.Json
import net.dmsg.android.core.*
import net.dmsg.android.platform.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

@RunWith(AndroidJUnit4::class)
class RemoteNoticeReceiverTest {
    private val account = "0".repeat(20)
    private val installation = "a".repeat(64)
    private val registration = "b".repeat(64)
    private val system = "d".repeat(64)
    private fun preferences(): String {
        val global = NoticeGlobal(1u, true, null, null, false, "all")
        val settings = NoticeAccount(1u, true, noticeCategories.associateWith { NoticeCategory(true, false) }, true, emptyMap(), 1u)
        return JSONObject().put("global", JSONObject(global.json())).put("account", JSONObject(settings.json())).put("account_id", account).put("remote_supported", true).put("global_pending_sync", false).put("global_conflict", false).put("account_conflict", false).put("registration_required", false).toString()
    }
    private fun payload() = mapOf("version" to "1", "route" to installation + "e".repeat(64), "registration" to registration, "event_id" to "c".repeat(64), "system_id" to system, "category" to "message", "generic" to "message", "sound" to "true", "expires" to (System.currentTimeMillis() + 60_000).toString())
    private fun withStore(test: (DMsgApplication, PushDeliveryStore, PushDeliveryStore.Lease, String) -> Unit) {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        val file = File(app.noBackupFilesDir, "push-delivery-v1.json")
        val old = file.takeIf(File::isFile)?.readBytes()
        file.delete()
        try {
            val store = PushDeliveryStore(app); val json = preferences(); val prefs = parseNoticePreferences(Json.parseToJsonElement(json))
            val lease = PushDeliveryStore.Lease(account, installation, registration, 1u, System.currentTimeMillis() + 300_000, "f".repeat(64), prefs)
            store.consent(account)
            assertTrue(store.put(lease, json, store.sequence()))
            test(app, store, lease, json)
        } finally {
            SystemNotices(app).remove(listOf(system))
            file.delete()
            if (old != null) file.writeBytes(old)
        }
    }
    @Test fun coldReceiverUsesOnlyTheKnownLeaseAndKeepsDuplicateDeliverySilent() = withStore { app, store, _, _ ->
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val before = app.hasMainClient
        if (Build.VERSION.SDK_INT >= 33) ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand("pm grant " + app.packageName + " android.permission.POST_NOTIFICATIONS")).use { it.readBytes() }
        val config = GoogleConfiguration(app.packageName, "production", "test-only", "public-test-only", "test-project", 123456789)
        val receiver = RemoteNoticeReceiver(app); val data = payload()
        assertEquals(RemoteNoticeReceiver.Result.INVALID, receiver.receive("other-project", data, config))
        assertEquals(RemoteNoticeReceiver.Result.SUBMITTED, receiver.receive("123456789", data, config))
        assertEquals(RemoteNoticeReceiver.Result.SUBMITTED, RemoteNoticeReceiver(app).receive("123456789", data, config))
        val manager = app.getSystemService(NotificationManager::class.java)
        val until = android.os.SystemClock.elapsedRealtime() + 5000
        while (android.os.SystemClock.elapsedRealtime() < until && manager.activeNotifications.none { it.tag == system && it.notification.flags and Notification.FLAG_ONLY_ALERT_ONCE != 0 }) Thread.sleep(50)
        val posted = manager.activeNotifications.filter { it.tag == system }
        assertEquals(1, posted.size)
        assertEquals(registration, posted.single().notification.extras.getString("dmsg.registration"))
        assertEquals(Notification.VISIBILITY_PRIVATE, posted.single().notification.visibility)
        assertTrue(posted.single().notification.flags and Notification.FLAG_ONLY_ALERT_ONCE != 0)
        store.forget(account)
        assertEquals(RemoteNoticeReceiver.Result.SUPPRESSED, receiver.receive("123456789", data + ("event_id" to "1".repeat(64)), config))
        assertEquals(before, app.hasMainClient)
    }
    @Test fun eachAccountOwnsItsPendingIntentAndCanDisableDeliverySeparately() = withStore { app, store, first, saved ->
        val other = "080g2081040g2081040g"
        val otherRegistration = "2".repeat(64)
        val otherSystem = "3".repeat(64)
        val json = JSONObject(saved).put("account_id", other).toString()
        val second = first.copy(account = other, registration = otherRegistration, preferences = parseNoticePreferences(Json.parseToJsonElement(json)))
        store.consent(other)
        assertTrue(store.put(second, json, store.sequence()))
        if (Build.VERSION.SDK_INT >= 33) ParcelFileDescriptor.AutoCloseInputStream(
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("pm grant " + app.packageName + " android.permission.POST_NOTIFICATIONS")
        ).use { it.readBytes() }
        val receiver = RemoteNoticeReceiver(app)
        val config = GoogleConfiguration(app.packageName, "production", "test-only", "public-test-only", "test-project", 123456789)
        val secondPayload = payload() + mapOf("registration" to otherRegistration, "system_id" to otherSystem,
            "route" to (installation + "4".repeat(64)), "event_id" to "5".repeat(64))
        // An unsynced local preference edit for one account must not block another account's lease.
        store.edit(first.preferences.copy(account = first.preferences.account!!.copy(categories = first.preferences.account.categories + ("message" to NoticeCategory(false, false)))))
        assertFalse(store.permitted(RemoteNotice.parse(payload(), System.currentTimeMillis())!!))
        assertTrue(store.permitted(RemoteNotice.parse(secondPayload, System.currentTimeMillis())!!))
        assertTrue(store.put(first, saved, store.sequence()))
        val manager = app.getSystemService(NotificationManager::class.java)
        try {
            assertEquals(RemoteNoticeReceiver.Result.SUBMITTED, receiver.receive("123456789", payload(), config))
            assertEquals(RemoteNoticeReceiver.Result.SUBMITTED, receiver.receive("123456789", secondPayload, config))
            val until = android.os.SystemClock.elapsedRealtime() + 5000
            while (android.os.SystemClock.elapsedRealtime() < until && manager.activeNotifications.none { it.tag == otherSystem }) Thread.sleep(50)
            val firstNotice = manager.activeNotifications.single { it.tag == system }.notification
            val secondNotice = manager.activeNotifications.single { it.tag == otherSystem }.notification
            assertNotEquals(firstNotice.contentIntent, secondNotice.contentIntent)
            assertEquals(otherRegistration, secondNotice.extras.getString("dmsg.registration"))
            assertNotNull(secondNotice.publicVersion)
            store.forget(account)
            assertEquals(RemoteNoticeReceiver.Result.SUPPRESSED, receiver.receive("123456789", payload(), config))
            assertEquals(RemoteNoticeReceiver.Result.SUBMITTED, receiver.receive("123456789", secondPayload, config))
        } finally { SystemNotices(app).remove(listOf(otherSystem)) }
    }

    @Test fun localEditsLogoutAndTokenChangesFenceDelayedRegistrationResults() = withStore { app, store, lease, json ->
        val old = store.sequence()
        store.edit(lease.preferences.copy(account = lease.preferences.account!!.copy(categories = lease.preferences.account.categories + ("message" to NoticeCategory(false, false)))))
        assertFalse(store.permitted(RemoteNotice.parse(payload(), System.currentTimeMillis())!!))
        assertFalse(store.put(lease, json, old))
        assertTrue(store.put(lease, json, store.sequence()))
        store.block()
        assertFalse(store.permitted(RemoteNotice.parse(payload(), System.currentTimeMillis())!!))
        store.forget(account)
        assertFalse(PushDeliveryStore(app).canRegister(account))
        assertFalse(store.put(lease, json, store.sequence()))
        store.consent(account)
        assertTrue(store.put(lease, json, store.sequence()))
        assertFalse(store.permitted(RemoteNotice.parse(payload() + ("registration" to "8".repeat(64)), System.currentTimeMillis())!!))
    }
}
