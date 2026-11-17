package net.dmsg.android

import android.app.Application
import android.os.Handler
import android.os.Looper
import net.dmsg.android.core.ClientController
import net.dmsg.android.platform.ForegroundEnvironment

class DMsgApplication : Application() {
    override fun attachBaseContext(base: android.content.Context) {
        super.attachBaseContext(base)
        // Providers are published before Application.onCreate(). Their public
        // labels must be ready even when Files starts this process first.
        net.dmsg.android.platform.AppLanguage.initialize(this)
    }
    private val mainClient = lazy { ClientController(this) }
    val client: ClientController get() = mainClient.value
    internal val hasMainClient: Boolean get() = mainClient.isInitialized()
    internal val playPurchases by lazy {
        val root = java.io.File(noBackupFilesDir, "system/play")
        net.dmsg.android.platform.PlayPurchaseStore(root, net.dmsg.android.platform.PlayPurchaseKeys(this, root))
    }
    internal val playBilling by lazy { net.dmsg.android.platform.GooglePlayBilling(this, playPurchases) }
    private val environment by lazy { ForegroundEnvironment(this, { client.synchronization.networkChanged(); client.fileWork.networkChanged() }, { client.synchronization.setConstrained(it) }) }
    private val mainHandler = Handler(Looper.getMainLooper())
    @Volatile private var mainWindows = 0
    private val leave = Runnable {
        if (mainWindows == 0 && mainClient.isInitialized()) {
            environment.stop()
            client.background()
        }
    }

    override fun onCreate() {
        super.onCreate()
        net.dmsg.android.platform.GooglePush.initializeConsented(this)
        androidx.core.content.ContextCompat.registerReceiver(this, object : android.content.BroadcastReceiver() {
            override fun onReceive(context: android.content.Context, intent: android.content.Intent) {
                net.dmsg.android.platform.ExportFileAccess.lock()
                net.dmsg.android.platform.AutofillRequests.clear()
            }
        }, android.content.IntentFilter(android.content.Intent.ACTION_SCREEN_OFF), androidx.core.content.ContextCompat.RECEIVER_NOT_EXPORTED)
    }

    override fun onConfigurationChanged(newConfig: android.content.res.Configuration) {
        super.onConfigurationChanged(newConfig)
        net.dmsg.android.platform.AppLanguage.configurationChanged()
        net.dmsg.android.platform.AppLanguage.refreshSystemLabels()
    }

    /** A remote receiver can signal an existing foreground owner, never start it. */
    fun receiveNoticeHint(): Boolean {
        if (mainWindows == 0 || !mainClient.isInitialized()) return false
        mainHandler.post { if (mainWindows > 0 && mainClient.isInitialized()) client.notices.refresh(receive = true) }
        return true
    }

    fun receivePushTokenChange() {
        mainHandler.post { if (mainWindows > 0 && mainClient.isInitialized()) client.push.reconcile(force = true) }
    }

    /** Extension authentication and providers never start the main Client. */
    fun mainStarted() {
        mainHandler.removeCallbacks(leave)
        if (mainWindows++ == 0) {
            client.foreground(); environment.start()
            runCatching { net.dmsg.android.platform.WorkspaceEntries.updateShortcuts(this) }
        }
    }

    fun mainStopped() {
        mainWindows = (mainWindows - 1).coerceAtLeast(0)
        if (mainWindows == 0) mainHandler.postDelayed(leave, 700)
    }
}
