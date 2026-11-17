package net.dmsg.android.platform

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import net.dmsg.android.DMsgApplication

/** A bounded OS receiver: metadata and generic notification only, never a Client. */
class GoogleMessagingService : FirebaseMessagingService() {
    override fun onMessageReceived(message: RemoteMessage) {
        runCatching {
            val configuration = GoogleConfiguration.load(this) ?: return
            if (message.notification != null) return
            RemoteNoticeReceiver(this).receive(message.from, message.data, configuration)
        }
    }
    override fun onNewToken(token: String) {
        // Firebase retains the token; persist no extra plaintext copy or account root.
        runCatching { PushDeliveryStore(this).block() }
        (application as DMsgApplication).receivePushTokenChange()
    }
    override fun onDeletedMessages() { (application as DMsgApplication).receiveNoticeHint() }
}
