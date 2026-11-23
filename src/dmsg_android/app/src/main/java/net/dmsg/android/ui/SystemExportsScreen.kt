package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import android.view.autofill.AutofillManager
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import net.dmsg.android.platform.*

@Composable fun SystemExportsScreen(client: ClientController, purpose: ExportPurpose) {
    val context = LocalContext.current
    val coordinator = client.systemExports
    val status by coordinator.status.state.collectAsStateWithLifecycle()
    val chosenFiles by coordinator.chosenFiles.collectAsStateWithLifecycle()
    val only by coordinator.only.collectAsStateWithLifecycle()
    val failure by (if (purpose == ExportPurpose.credentials) coordinator.autoFillFailure else coordinator.failure).collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val busy = state.operations.any { it.startsWith("exports:") && it !in setOf("exports:refresh", "exports:status", "exports:logins", "exports:file-list") }
    var choosing by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf(false) }
    var browsers by remember { mutableStateOf(false) }
    val credentials = purpose == ExportPurpose.credentials
    LaunchedEffect(state.epoch, purpose) { coordinator.load() }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            ReadFeedback(status, coordinator::load)
            val header = if (credentials) status.valueOrNull()?.credentials else status.valueOrNull()?.files
            if (!credentials && header == null && status is ReadState.Ready) Text(str(R.string.text_no_snapshot_3189948), style = MaterialTheme.typography.titleMedium)
            if (header != null && !credentials) {
                Text(if (System.currentTimeMillis() in header.created until header.expires) str(R.string.text_active_copies_0be2cd6) else str(R.string.text_copies_expired_f75331e), style = MaterialTheme.typography.titleMedium)
                LabelValue(str(R.string.text_items_802a927), localizedNumber(header.count))
                LabelValue(str(R.string.text_expires_7e25e44), java.text.DateFormat.getDateTimeInstance().format(java.util.Date(header.expires)))
            }
            if (credentials) {
                val enabled = context.getSystemService(AutofillManager::class.java).hasEnabledAutofillServices()
                LabelValue(str(R.string.text_system_permission_ecf1e5e), if (enabled) str(R.string.text_enabled_b62610f) else str(R.string.text_not_enabled_66c94ff))
                OutlinedButton(onClick = { client.beginExternalInteraction(); context.startActivity(Intent(Settings.ACTION_REQUEST_SET_AUTOFILL_SERVICE, Uri.parse("package:" + context.packageName))) }, enabled = !busy) { Text(str(R.string.text_enable_dmsg_in_autofill_settings_68e4c0b)) }
                TextButton(onClick = { browsers = true }) { Text(str(R.string.text_trusted_browsers_12e7452)) }
                Text(str(R.string.autofill_choose_at_login))
                if (failure == null) header?.let { LabelValue(str(R.string.text_items_802a927), localizedNumber(it.count)) }
            } else {
                Notice(str(R.string.text_choose_up_to_10_immutable_file_copies_100_mib_each_and_200_mib_to_b0b24f1))
                chosenFiles.forEach { item -> Row(Modifier.fillMaxWidth()) { Column(Modifier.weight(1f)) { Text(item.name); Text(fileSize(item.size)) }; TextButton(onClick = { coordinator.toggle(item) }, enabled = !busy) { Text(str(R.string.text_remove_78190c6)) } } }
                OutlinedButton(onClick = { client.beginExternalInteraction(); context.startActivity(Intent(context, ExportAccessActivity::class.java).putExtra("files", true)) }, enabled = !busy && header != null) { Text(str(R.string.text_authorize_five_minute_file_access_a93d55d)) }
                TextButton(onClick = { ExportFileAccess.lock(); ExportFileAccess.changed(context) }, enabled = header != null) { Text(str(R.string.text_end_file_access_now_8128a6e)) }
            }
            if (!credentials) OutlinedButton(onClick = { choosing = true; coordinator.browseFiles() }, enabled = !busy) { Text(str(R.string.text_select_files_8b87cc5)) }
            SettingToggle(str(R.string.text_biometrics_only_for_copies_b141ee5), only, { if (credentials) coordinator.setCredentialBiometry(it) else coordinator.setFileBiometry(it) }, enabled = !busy && status.valueOrNull()?.requiresBiometry == false && status.valueOrNull()?.biometryAvailable == true)
            if (!credentials) Text(str(R.string.text_these_copies_have_separate_keys_and_never_include_account_roots_o_deb16b8), style = MaterialTheme.typography.bodySmall)
            failure?.let {
                Notice(errorText(it), true)
            }
            if (credentials) {
                TextButton(onClick = { coordinator.refreshCredentials(force = true) }, enabled = !busy && "exports:refresh" !in state.operations) {
                    Text(str(if (failure == null) R.string.text_update_copies_47cb43d else R.string.text_retry_563171c))
                }
            }
            if (!credentials) TextButton(onClick = { removing = true }, enabled = !busy && (header != null || chosenFiles.isNotEmpty())) { Text(str(R.string.text_remove_selected_copies_770ac24)) }
        }
        Column(Modifier.fillMaxWidth().padding(pagePadding())) {
            if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            if (!credentials) Button(onClick = coordinator::updateFiles, enabled = !busy && status.valueOrNull() != null && chosenFiles.isNotEmpty(), modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_update_copies_47cb43d)) }
        }
    }
    if (choosing) ExportSelectionDialog(client) { choosing = false }
    if (removing) AlertDialog(onDismissRequest = { removing = false }, title = { Text(str(R.string.text_remove_these_system_copies_8936b8b)) }, text = { Text(str(R.string.text_new_file_reads_stop_copies_already_kept_by_other_apps_cannot_be_r_c1ca3c2)) }, confirmButton = { TextButton(onClick = { removing = false; coordinator.removeFiles() }) { Text(str(R.string.text_remove_copies_5546885)) } }, dismissButton = { TextButton(onClick = { removing = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (browsers) TrustedBrowsersDialog { browsers = false }
}

@Composable private fun ExportSelectionDialog(client: ClientController, close: () -> Unit) {
    val coordinator = client.systemExports
    val choices by coordinator.fileChoices.state.collectAsStateWithLifecycle()
    val chosenFiles by coordinator.chosenFiles.collectAsStateWithLifecycle()
    var parents by remember { mutableStateOf(listOf("root")) }
    AlertDialog(onDismissRequest = close, title = { Text(str(R.string.text_select_files_8b87cc5)) }, text = {
        Column {
            LazyColumn(Modifier.heightIn(max = 400.dp)) {
                item { ReadFeedback(choices, { coordinator.browseFiles(parents.last()) }) }
                if (parents.size > 1) item { TextButton(onClick = { parents = parents.dropLast(1); coordinator.browseFiles(parents.last()) }) { Text(str(R.string.text_parent_folder_3af9479)) } }
                items(choices.valueOrNull()?.folders.orEmpty(), key = { "folder:" + it.id }) { folder -> AccountRow(folder.name) { parents = parents + folder.id; coordinator.browseFiles(folder.id) } }
                items(choices.valueOrNull()?.files.orEmpty(), key = { it.id }) { file ->
                    val selected = chosenFiles.any { it.id == file.id }
                    SelectionRow(file.name, selected, subtitle = fileSize(file.size), enabled = selected || chosenFiles.size < 10 && file.size <= SystemExportStore.MAX_FILE.toULong() && chosenFiles.sumOf { it.size } + file.size <= 200uL * SystemExportStore.PART.toULong()) { coordinator.toggle(file) }
                }
            }
        }
    }, confirmButton = { TextButton(onClick = close) { Text(str(R.string.text_done_9ffd599)) } })
}

@Composable private fun TrustedBrowsersDialog(close: () -> Unit) {
    val context = LocalContext.current
    val store = remember { TrustedBrowsers(context) }
    val browsers = remember { AutofillTrust.browserCandidates(context) }
    var changes by remember { mutableIntStateOf(0) }
    var pending by remember { mutableStateOf<BrowserTrust?>(null) }
    AlertDialog(onDismissRequest = close, title = { Text(str(R.string.text_trusted_browsers_12e7452)) }, text = {
        Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
            Text(str(R.string.text_a_trusted_browser_may_request_any_selected_website_login_trust_on_d1fa443))
            key(changes) { browsers.forEach { browser ->
                SelectionRow(browser.label, store.trusted(browser.packageName, browser.fingerprints), subtitle = browser.packageName) { value -> if (value) pending = browser else { store.set(browser, false); changes++ } }
            } }
        }
    }, confirmButton = { TextButton(onClick = close) { Text(str(R.string.text_done_9ffd599)) } })
    pending?.let { browser -> AlertDialog(onDismissRequest = { pending = null }, title = { Text(str(R.string.text_trust_this_browser_87e0172)) }, text = { Column(Modifier.heightIn(max = 350.dp).verticalScroll(rememberScrollState())) { Text(browser.label); Text(browser.packageName); browser.fingerprints.forEach { Text(it, style = MaterialTheme.typography.bodySmall) }; Text(str(R.string.text_it_will_be_trusted_to_identify_websites_for_your_selected_passwor_1858444)) } }, confirmButton = { TextButton(onClick = { store.set(browser, true); changes++; pending = null }) { Text(str(R.string.text_trust_browser_f229c49)) } }, dismissButton = { TextButton(onClick = { pending = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}
