package net.dmsg.android

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.browser.customtabs.CustomTabsIntent
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import net.dmsg.android.ui.DMsgApp

class MainActivity : ComponentActivity() {
    override fun attachBaseContext(newBase: Context) = super.attachBaseContext(net.dmsg.android.platform.AppLanguage.wrap(newBase))
    private val client get() = (application as DMsgApplication).client
    private var callback: String? = null
    private val screenOff = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) { if (intent.action == Intent.ACTION_SCREEN_OFF) client.lock() }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        net.dmsg.android.platform.AppLanguage.applyToWindow(this)
        client.navigation.restore(savedInstanceState?.getString("workspace-navigation"))
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        enableEdgeToEdge()
        if (savedInstanceState == null) receive(intent)
        ContextCompat.registerReceiver(this, screenOff, IntentFilter(Intent.ACTION_SCREEN_OFF), ContextCompat.RECEIVER_NOT_EXPORTED)
        setContent {
            DMsgApp(client,
                browser = { url -> CustomTabsIntent.Builder().build().launchUrl(this, android.net.Uri.parse(url)) },
                share = { file, mime ->
                    client.beginExternalInteraction()
                    val uri = FileProvider.getUriForFile(this, "$packageName.files", file)
                    startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
                        type = mime.ifBlank { "application/octet-stream" }
                        putExtra(Intent.EXTRA_STREAM, uri)
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    }, null))
                })
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) { super.onWindowFocusChanged(hasFocus); if (hasFocus) { client.clipboard.clearExpired(); consumeCallback(); client.tryAutomaticUnlock() } }

    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); receive(intent); if (hasWindowFocus()) consumeCallback() }
    override fun onSaveInstanceState(outState: Bundle) { outState.putString("workspace-navigation", client.navigation.save()); super.onSaveInstanceState(outState) }
    private fun receive(intent: Intent) {
        if (client.incoming.receive(intent)) {
            callback = null
            // The bounded intake copied its inputs. Do not replay parcelled
            // text/URIs on recreation or reinterpret SEND data as navigation.
            setIntent(Intent(this, MainActivity::class.java))
            return
        }
        if (intent.action == net.dmsg.android.platform.SystemNotices.OPEN) {
            val route = runCatching { if (intent.getIntExtra("version", 0) == 1) net.dmsg.android.core.NoticeRoute.parse(intent.getStringExtra("route")) else null }.getOrNull()
            if (route != null) client.requestNotice(route) else client.showError("INVALID_LINK")
            callback = null; setIntent(Intent(this, MainActivity::class.java)); return
        }
        if (intent.action == net.dmsg.android.platform.CipherTransferService.OPEN) client.requestWorkspace(net.dmsg.android.core.WorkspaceLink("transfers"))
        callback = intent.dataString?.takeIf { it.length <= 96 * 1024 }
        intent.data = null
        setIntent(Intent(intent).setData(null))
    }
    private fun consumeCallback() { callback?.let { callback = null; client.acceptCallback(it) } }

    override fun onStart() { super.onStart(); client.prompts.attach(this); client.passkeys.attach(this); (application as DMsgApplication).mainStarted() }
    override fun onResume() { super.onResume(); consumeCallback() }
    override fun onStop() { client.prompts.detach(this); (application as DMsgApplication).mainStopped(); super.onStop() }
    override fun onDestroy() { unregisterReceiver(screenOff); client.prompts.detach(this); client.passkeys.detach(this); super.onDestroy() }
    override fun onUserInteraction() { super.onUserInteraction(); client.activity() }
}
