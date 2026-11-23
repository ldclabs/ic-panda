package net.dmsg.android.platform

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.os.SystemClock
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.service.autofill.Dataset
import android.view.WindowManager
import android.view.autofill.AutofillManager
import android.view.autofill.AutofillValue
import android.widget.RemoteViews
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.*
import net.dmsg.android.ui.*

private const val PICKER_TIMEOUT_MS = 120_000L

/** Independent authentication UI: no access to DMsgApplication.client. */
class ExportAccessActivity : ComponentActivity() {
    override fun attachBaseContext(newBase: android.content.Context) = super.attachBaseContext(AppLanguage.wrap(newBase))
    private val prompts = PresencePrompts()
    private var opened by mutableStateOf<OpenExport?>(null)
    private var catalogHeader by mutableStateOf<ExportHeader?>(null)
    private var authenticatedAt = 0L
    private var idleTimeout: Job? = null
    private var keyExpiry: Job? = null
    private var work: Job? = null
    private var entries by mutableStateOf<List<ExportCredential>>(emptyList())
    private var error by mutableStateOf<String?>(null)
    private var busy by mutableStateOf(false)
    private var recent by mutableStateOf<List<String>>(emptyList())
    private var pending by mutableStateOf<ExportCredential?>(null)
    private var service: String? = null
    private var startedAuthentication = false
    private var active = true
    private var target: AutofillTarget? = null
    private var request: String? = null
    private val files get() = intent.getBooleanExtra("files", false)
    private val store by lazy { systemExports(this, if (files) ExportPurpose.files else ExportPurpose.credentials, prompts) }
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        AppLanguage.applyToWindow(this)
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        window.setHideOverlayWindows(true)
        setResult(Activity.RESULT_CANCELED)
        request = intent.getStringExtra("request")
        target = request?.let(AutofillRequests::get)
        if (!files && target == null) { finish(); return }
        scheduleTimeout()
        setContent {
            DMsgTheme(null) {
                Scaffold(modifier = Modifier.imePadding(), bottomBar = {
                    if (files) TextButton(onClick = { finish() }, modifier = Modifier.navigationBarsPadding().fillMaxWidth().heightIn(min = 48.dp)) {
                        Text(str(R.string.text_cancel_998b9c4))
                    }
                }) { padding ->
                    val heading: @Composable () -> Unit = {
                        if (files) {
                            Text(str(R.string.text_files_app_copies_96ebae6), style = MaterialTheme.typography.headlineSmall, modifier = Modifier.semantics { heading() })
                            Text(str(R.string.text_system_authentication_permits_access_to_selected_copies_for_five_150be41))
                        } else {
                            target?.let { PasswordPickerHeading(it.displayName, it.host, ::finish) }
                        }
                        error?.let { code ->
                            Box(Modifier.semantics { liveRegion = LiveRegionMode.Polite }) {
                                Notice(if (!files && code in setOf("EXPORT_NOT_CONFIGURED", "EXPORT_EXPIRED")) str(R.string.autofill_open) else errorText(code), true)
                            }
                        }
                        if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                        if (opened == null && catalogHeader == null && !busy) Button(onClick = ::authenticate, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_authenticate_a0fd7c1)) }
                    }
                    if (catalogHeader != null) {
                        PasswordPicker(Modifier.fillMaxSize().padding(padding), entries, recent, target?.host, busy,
                            onChoose = ::choose, onSearch = ::scheduleTimeout, heading = heading)
                    } else {
                        LazyColumn(Modifier.fillMaxSize().padding(padding), contentPadding = pagePadding()) {
                            item { Column(verticalArrangement = Arrangement.spacedBy(12.dp)) { heading() } }
                        }
                    }
                }
                pending?.let { entry ->
                    ConfirmLogin(entry, target?.displayName.orEmpty(), target?.host, service != null && entry.itemId != null,
                        dismiss = { pending = null }, fill = { remember ->
                            if (pending == entry) {
                                pending = null
                                fill(entry, remember)
                            }
                        })
                }
            }
        }
    }
    private fun valid() = active && !isFinishing && (files || request?.let(AutofillRequests::get) == target)

    private fun scheduleTimeout() {
        idleTimeout?.cancel()
        idleTimeout = lifecycleScope.launch { delay(PICKER_TIMEOUT_MS); finish() }
    }

    override fun onUserInteraction() {
        super.onUserInteraction()
        if (!files) scheduleTimeout()
    }

    private suspend fun openExport(): OpenExport {
        var acquired: OpenExport? = null
        try {
            val result = withContext(Dispatchers.IO) { store.open(::valid).also { acquired = it } }
            check(valid()) { "CANCELLED" }
            opened = result
            authenticatedAt = SystemClock.elapsedRealtime()
            acquired = null
            return result
        } finally { acquired?.close() }
    }

    private suspend fun openLogins(value: AutofillTarget, previous: ExportHeader? = null, confirmed: ExportCredential? = null): ExportCredential? {
        val snapshot = openExport()
        val logins = withContext(Dispatchers.IO) { store.credentials(snapshot) }
        val selected = previous?.let { confirmed?.afterUnlock(it, snapshot.header, logins) }
        val key = withContext(Dispatchers.IO) { serviceFor(value, snapshot.header.account, selected) }
        val order = withContext(Dispatchers.IO) { store.recentLogins(snapshot.header.account, key) }
        check(valid()) { "CANCELLED" }
        service = key
        recent = order
        entries = logins
        catalogHeader = snapshot.header
        scheduleTimeout()
        // Browsing may continue while the user is active. The decrypted key still
        // expires on its original deadline, independently of touches in the list.
        keyExpiry = lifecycleScope.launch {
            delay((authenticatedAt + PICKER_TIMEOUT_MS - SystemClock.elapsedRealtime()).coerceAtLeast(0))
            if (opened === snapshot) {
                opened = null
                snapshot.close()
            }
        }
        return selected
    }

    private fun authenticate() {
        if (busy) return
        busy = true; error = null
        work = lifecycleScope.launch {
            try {
                if (files) {
                    val result = openExport()
                    withContext(Dispatchers.IO) { ExportFileAccess.grant(this@ExportAccessActivity, store, result) }
                    setResult(Activity.RESULT_OK); finish()
                } else {
                    openLogins(checkNotNull(target))
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Throwable) {
                closePicker()
                val reason = safeError(failure)
                if (reason == "CANCELLED") finish() else error = reason
            }
            finally { busy = false }
        }
    }
    private fun serviceFor(value: AutofillTarget, account: String, selected: ExportCredential? = null): String? {
        val fingerprints = AutofillTrust.fingerprints(this, value.packageName)
        check(fingerprints == value.fingerprints) { "AUTOFILL_UNVERIFIED" }
        val browsers = TrustedBrowsers(this)
        val trustedBrowser = browsers.trusted(value.packageName, fingerprints)
        val app = "app:" + value.packageName + ":" + fingerprints.sorted().joinToString(",")
        if (value.host != null && value.secureWeb) {
            if (trustedBrowser) return "https://" + value.host
            val scopedSite = app + "|https://" + value.host
            // Ask for a site's credential-sharing statement only for an explicit
            // selection or a previous grant to this particular signed webview.
            val previouslyUsed = store.recentLogins(account, scopedSite).isNotEmpty()
            if (account != SystemExportStore.digest(net.dmsg.android.core.LOCAL_ACCOUNT.toByteArray()) && (previouslyUsed || selected?.let { AutofillTrust.host(it.origin) == value.host } == true) &&
                runCatching { AutofillTrust.associated(value.host, value.packageName, fingerprints) }.getOrDefault(false)) return scopedSite
        }
        if (value.web || trustedBrowser || AutofillTrust.isBrowser(this, value.packageName)) return null
        return app
    }

    private fun choose(entry: ExportCredential) {
        if (busy || pending != null || !valid()) return
        val account = catalogHeader?.account ?: return
        if (entry.itemId in recent) { fill(entry, true); return }
        val value = target ?: return
        if (service != null || !value.secureWeb || value.host == null) { pending = entry; return }
        busy = true
        work = lifecycleScope.launch {
            try {
                service = withContext(Dispatchers.IO) { serviceFor(value, account, entry) }
                if (valid()) pending = entry
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Throwable) { error = safeError(failure) }
            finally { busy = false }
        }
    }

    private fun fill(entry: ExportCredential, remember: Boolean) {
        val value = target ?: return
        val previous = catalogHeader ?: return
        if (busy || !valid()) return
        val acceptedService = service
        val previouslyUsed = entry.itemId in recent
        busy = true; error = null
        work = lifecycleScope.launch {
            try {
                val selected = if (opened == null || SystemClock.elapsedRealtime() - authenticatedAt >= PICKER_TIMEOUT_MS || !store.current(previous)) {
                    closePicker()
                    val refreshed = openLogins(value, previous, entry) ?: return@launch
                    if (remember && (service == null || service != acceptedService || (previouslyUsed && refreshed.itemId !in recent))) {
                        pending = refreshed
                        return@launch
                    }
                    refreshed
                } else entry
                val snapshot = checkNotNull(opened)
                keyExpiry?.cancel()
                val response = withContext(Dispatchers.IO) {
                    check(valid() && store.current(snapshot.header)) { "EXPORT_EXPIRED" }
                    val fingerprints = AutofillTrust.fingerprints(this@ExportAccessActivity, value.packageName)
                    check(fingerprints == value.fingerprints) { "AUTOFILL_UNVERIFIED" }
                    if (remember && service?.startsWith("https://") == true) {
                        check(TrustedBrowsers(this@ExportAccessActivity).trusted(value.packageName, fingerprints)) { "AUTOFILL_UNVERIFIED" }
                    }
                    check(valid() && store.current(snapshot.header)) { "CANCELLED" }
                    check(store.credentials(snapshot).contains(selected)) { "EXPORT_EXPIRED" }
                    val bytes = store.plain(snapshot, selected.id)
                    try {
                        val presentation = RemoteViews(packageName, android.R.layout.simple_list_item_1).apply { setTextViewText(android.R.id.text1, selected.username) }
                        val dataset = Dataset.Builder(presentation).setValue(value.password, AutofillValue.forText(bytes.toString(Charsets.UTF_8)))
                        value.username?.let { dataset.setValue(it, AutofillValue.forText(selected.username)) }
                        dataset.build()
                    } finally { bytes.fill(0) }
                }
                check(valid()) { "CANCELLED" }
                check(store.current(snapshot.header)) { "EXPORT_EXPIRED" }
                if (remember) store.remember(checkNotNull(selected.itemId), checkNotNull(service), snapshot)
                closePicker()
                setResult(Activity.RESULT_OK, Intent().putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT, response)
                    .putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT_EPHEMERAL_DATASET, true))
                finish()
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Throwable) {
                val code = safeError(failure)
                closePicker()
                if (code == "CANCELLED") finish() else error = code
            }
            finally { busy = false }
        }
    }
    private fun closePicker() {
        keyExpiry?.cancel()
        catalogHeader = null
        opened?.close()
        opened = null
        pending = null
        entries = emptyList()
        recent = emptyList()
        service = null
    }

    override fun onStart() {
        super.onStart(); active = true; prompts.attach(this)
        if (!files && !startedAuthentication) { startedAuthentication = true; authenticate() }
    }
    override fun onStop() {
        active = false; prompts.cancel(); prompts.detach(this); work?.cancel(); closePicker()
        if (!files) finish()
        super.onStop()
    }
    override fun onDestroy() { prompts.cancel(); opened?.close(); opened = null; super.onDestroy() }
    private fun safeError(error: Throwable) = when (error) {
        is net.dmsg.core.MobileException.Failure -> error.code
        is KeyPermanentlyInvalidatedException -> "EXPORT_KEY_UNAVAILABLE"
        else -> error.message?.takeIf { it in setOf("EXPORT_EXPIRED", "EXPORT_NOT_CONFIGURED", "EXPORT_KEY_UNAVAILABLE", "AUTOFILL_UNVERIFIED", "CANCELLED", "DEVICE_LOCKED", "HARDWARE_UNAVAILABLE") } ?: "OPERATION_FAILED"
    }
}

/** DocumentsUI may open only this harmless authentication entry, never a file. */
class ExportDocumentSettingsActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val uri = intent.data
        if (intent.action == android.provider.DocumentsContract.ACTION_DOCUMENT_SETTINGS && uri?.scheme == "content" && uri.authority == packageName + ".documents" && uri.path == "/document/root")
            startActivity(Intent(this, ExportAccessActivity::class.java).putExtra("files", true))
        finish()
    }
}
class AutofillSettingsActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        startActivity(Intent(Intent.ACTION_VIEW, android.net.Uri.parse("dmsg://open/autofill"), this, net.dmsg.android.MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP))
        finish()
    }
}
