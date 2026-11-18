package net.dmsg.android.platform

import net.dmsg.android.R
import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.service.autofill.Dataset
import android.service.autofill.FillResponse
import android.view.WindowManager
import android.view.autofill.AutofillManager
import android.view.autofill.AutofillValue
import android.widget.RemoteViews
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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

/** Independent authentication UI: no access to DMsgApplication.client. */
class ExportAccessActivity : ComponentActivity() {
    override fun attachBaseContext(newBase: android.content.Context) = super.attachBaseContext(AppLanguage.wrap(newBase))
    private val prompts = PresencePrompts()
    private var opened: OpenExport? = null
    private var work: Job? = null
    private var entries by mutableStateOf<List<ExportCredential>>(emptyList())
    private var error by mutableStateOf<String?>(null)
    private var busy by mutableStateOf(false)
    private var query by mutableStateOf("")
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
        lifecycleScope.launch { delay(120_000); finish() }
        setContent {
            DMsgTheme(null) {
                Scaffold(modifier = Modifier.imePadding(), bottomBar = {
                    TextButton(onClick = { finish() }, modifier = Modifier.navigationBarsPadding().fillMaxWidth().heightIn(min = 48.dp)) {
                        Text(str(R.string.text_cancel_998b9c4))
                    }
                }) { padding ->
                    Column(Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                        Text(if (files) str(R.string.text_files_app_copies_96ebae6) else str(R.string.text_choose_a_password_972ff98), style = MaterialTheme.typography.headlineSmall, modifier = Modifier.semantics { heading() })
                        if (files) Text(str(R.string.text_system_authentication_permits_access_to_selected_copies_for_five_150be41))
                        else target?.let { value ->
                            Text(str(R.string.text_requesting_app_c06bb1c) + ": " + value.packageName)
                            value.host?.let { Text(str(R.string.text_website_claimed_by_app_ae298e3) + ": " + it) }
                            Text(str(R.string.text_dmsg_verifies_the_installed_app_before_sharing_a_password_only_lo_38ddeeb))
                        }
                        error?.let { Box(Modifier.semantics { liveRegion = LiveRegionMode.Polite }) { Notice(errorText(it), true) } }
                        if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                        if (opened == null) Button(onClick = ::authenticate, enabled = !busy, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_authenticate_a0fd7c1)) }
                        if (opened != null && !files) OutlinedTextField(query, { query = it.take(256) }, label = { Text(str(R.string.text_search_selected_logins_048697e)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        val matches = entries.filter { query.isBlank() || listOf(it.title, it.username, it.origin).any { text -> text.contains(query, true) } }
                        matches.forEach { entry ->
                            OutlinedButton(onClick = { fill(entry) }, enabled = !busy, modifier = Modifier.fillMaxWidth()) {
                                Column(Modifier.fillMaxWidth()) { Text(entry.title); Text(entry.username); Text(entry.origin) }
                            }
                        }
                        if (opened != null && !files && entries.isEmpty()) Notice(str(R.string.text_no_selected_login_matches_this_website_update_your_selected_copie_9eaf25d))
                        if (opened != null && !files && entries.isNotEmpty() && matches.isEmpty()) {
                            Text(str(R.string.no_matches), modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                            TextButton(onClick = { query = "" }) { Text(str(R.string.text_clear_search_3f4dec6)) }
                        }
                    }
                }
            }
        }
    }
    private fun valid() = active && !isFinishing && (files || request?.let(AutofillRequests::get) == target)
    private fun authenticate() {
        if (busy) return
        busy = true; error = null
        work = lifecycleScope.launch {
            try {
                val result = withContext(Dispatchers.IO) { store.open(::valid) }
                if (!valid()) { result.close(); return@launch }
                opened = result
                if (files) {
                    withContext(Dispatchers.IO) { ExportFileAccess.grant(this@ExportAccessActivity, store, result) }
                    setResult(Activity.RESULT_OK); finish()
                } else entries = withContext(Dispatchers.IO) { store.credentials(result).filter { target?.host == null || AutofillTrust.host(it.origin) == target?.host } }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Throwable) {
                val reason = safeError(failure)
                if (reason == "CANCELLED") finish() else error = reason
            }
            finally { busy = false }
        }
    }
    private fun fill(entry: ExportCredential) {
        val value = target ?: return; val snapshot = opened ?: return
        if (busy) return
        busy = true; error = null
        work = lifecycleScope.launch {
            try {
                val response = withContext(Dispatchers.IO) {
                    check(valid() && store.current(snapshot.header)) { "EXPORT_EXPIRED" }
                    val fingerprints = AutofillTrust.fingerprints(this@ExportAccessActivity, value.packageName)
                    check(fingerprints == value.fingerprints) { "AUTOFILL_UNVERIFIED" }
                    val origin = AutofillTrust.host(entry.origin) ?: error("AUTOFILL_UNVERIFIED")
                    check(value.host == null || value.host == origin) { "AUTOFILL_UNVERIFIED" }
                    val trustedBrowser = value.host != null && TrustedBrowsers(this@ExportAccessActivity).trusted(value.packageName, fingerprints)
                    check(trustedBrowser || runCatching { AutofillTrust.associated(entry.origin, value.packageName, fingerprints) }.getOrDefault(false)) { "AUTOFILL_UNVERIFIED" }
                    check(valid() && store.current(snapshot.header) && AutofillTrust.fingerprints(this@ExportAccessActivity, value.packageName) == fingerprints) { "CANCELLED" }
                    check(store.credentials(snapshot).contains(entry)) { "EXPORT_EXPIRED" }
                    val bytes = store.plain(snapshot, entry.id)
                    try {
                        val presentation = RemoteViews(packageName, android.R.layout.simple_list_item_1).apply { setTextViewText(android.R.id.text1, entry.username) }
                        val dataset = Dataset.Builder(presentation).setValue(value.password, AutofillValue.forText(bytes.toString(Charsets.UTF_8)))
                        value.username?.let { dataset.setValue(it, AutofillValue.forText(entry.username)) }
                        FillResponse.Builder().addDataset(dataset.build()).build()
                    } finally { bytes.fill(0) }
                }
                if (valid() && store.current(snapshot.header)) {
                    setResult(Activity.RESULT_OK, Intent().putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT, response)); finish()
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Throwable) { error = safeError(failure) }
            finally { busy = false }
        }
    }
    override fun onStart() { super.onStart(); active = true; prompts.attach(this) }
    override fun onStop() {
        active = false; prompts.cancel(); prompts.detach(this); work?.cancel(); entries = emptyList(); query = ""; opened?.close(); opened = null
        if (!files) { request?.let(AutofillRequests::remove); finish() }
        super.onStop()
    }
    override fun onDestroy() { prompts.cancel(); opened?.close(); opened = null; request?.let(AutofillRequests::remove); super.onDestroy() }
    private fun safeError(error: Throwable) = (error as? net.dmsg.core.MobileException.Failure)?.code ?: error.message?.takeIf { it in setOf("EXPORT_EXPIRED", "EXPORT_NOT_CONFIGURED", "EXPORT_KEY_UNAVAILABLE", "AUTOFILL_UNVERIFIED", "CANCELLED", "DEVICE_LOCKED", "HARDWARE_UNAVAILABLE") } ?: "OPERATION_FAILED"
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
