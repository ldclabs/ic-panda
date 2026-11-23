package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
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
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.*
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
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            ReadFeedback(status, coordinator::load)
            val header = if (credentials) status.valueOrNull()?.credentials else status.valueOrNull()?.files
            failure?.let { Notice(errorText(it), true) }
                ?: if (!credentials) Notice(str(R.string.text_choose_up_to_10_immutable_file_copies_100_mib_each_and_200_mib_to_b0b24f1)) else Unit
            if (credentials) {
                val enabled = context.getSystemService(AutofillManager::class.java).hasEnabledAutofillServices()
                DetailCard {
                    LabelValue(str(R.string.text_system_permission_ecf1e5e), if (enabled) str(R.string.text_enabled_b62610f) else str(R.string.text_not_enabled_66c94ff))
                    if (failure == null) header?.let { LabelValue(str(R.string.text_items_802a927), localizedNumber(it.count)) }
                    Text(str(R.string.autofill_choose_at_login), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    OutlinedButton(
                        onClick = { client.beginExternalInteraction(); context.startActivity(Intent(Settings.ACTION_REQUEST_SET_AUTOFILL_SERVICE, Uri.parse("package:" + context.packageName))) },
                        enabled = !busy,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(Icons.Outlined.Settings, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_enable_dmsg_in_autofill_settings_68e4c0b))
                    }
                }
                Section {
                    ListRow(str(R.string.text_trusted_browsers_12e7452), icon = Icons.Outlined.Public) { browsers = true }
                    SettingToggle(
                        str(R.string.text_biometrics_only_for_copies_b141ee5),
                        only,
                        { coordinator.setCredentialBiometry(it) },
                        enabled = !busy && status.valueOrNull()?.requiresBiometry == false && status.valueOrNull()?.biometryAvailable == true,
                        icon = Icons.Outlined.Fingerprint,
                    )
                }
            } else {
                DetailCard {
                    if (header == null && status is ReadState.Ready) {
                        Text(str(R.string.text_no_snapshot_3189948), style = MaterialTheme.typography.titleMedium)
                    } else if (header != null) {
                        Text(
                            if (System.currentTimeMillis() in header.created until header.expires) str(R.string.text_active_copies_0be2cd6) else str(R.string.text_copies_expired_f75331e),
                            style = MaterialTheme.typography.titleMedium,
                        )
                        LabelValue(str(R.string.text_items_802a927), localizedNumber(header.count))
                        LabelValue(str(R.string.text_expires_7e25e44), java.text.DateFormat.getDateTimeInstance().format(java.util.Date(header.expires)))
                    }
                    Text(str(R.string.text_these_copies_have_separate_keys_and_never_include_account_roots_o_deb16b8), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (chosenFiles.isNotEmpty()) Section(title = str(R.string.text_select_files_8b87cc5)) {
                    chosenFiles.forEach { item ->
                        ListItem(
                            headlineContent = { Text(item.name) },
                            supportingContent = { Text(fileSize(item.size)) },
                            leadingContent = { RowIcon(Icons.Outlined.Description) },
                            trailingContent = {
                                TextButton(onClick = { coordinator.toggle(item) }, enabled = !busy) { Text(str(R.string.text_remove_78190c6)) }
                            },
                        )
                    }
                }
                Section {
                    ListRow(str(R.string.text_select_files_8b87cc5), icon = Icons.Outlined.FolderOpen, enabled = !busy) { choosing = true; coordinator.browseFiles() }
                    ListRow(str(R.string.text_authorize_five_minute_file_access_a93d55d), icon = Icons.Outlined.LockOpen, enabled = !busy && header != null) {
                        client.beginExternalInteraction()
                        context.startActivity(Intent(context, ExportAccessActivity::class.java).putExtra("files", true))
                    }
                    ListRow(str(R.string.text_end_file_access_now_8128a6e), icon = Icons.Outlined.Lock, enabled = header != null) {
                        ExportFileAccess.lock()
                        ExportFileAccess.changed(context)
                    }
                    SettingToggle(
                        str(R.string.text_biometrics_only_for_copies_b141ee5),
                        only,
                        { coordinator.setFileBiometry(it) },
                        enabled = !busy && status.valueOrNull()?.requiresBiometry == false && status.valueOrNull()?.biometryAvailable == true,
                        icon = Icons.Outlined.Fingerprint,
                    )
                }
                Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                    ListRow(
                        str(R.string.text_remove_selected_copies_770ac24),
                        icon = Icons.Outlined.DeleteOutline,
                        destructive = true,
                        enabled = !busy && (header != null || chosenFiles.isNotEmpty()),
                    ) { removing = true }
                }
            }
        }
        BottomActionBar {
            if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            if (credentials) {
                Button(
                    onClick = { coordinator.refreshCredentials(force = true) },
                    enabled = !busy && "exports:refresh" !in state.operations,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(Icons.Outlined.Refresh, null); Spacer(Modifier.width(8.dp))
                    Text(str(if (failure == null) R.string.text_update_copies_47cb43d else R.string.text_retry_563171c))
                }
            } else {
                Button(
                    onClick = coordinator::updateFiles,
                    enabled = !busy && status.valueOrNull() != null && chosenFiles.isNotEmpty(),
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(str(R.string.text_update_copies_47cb43d)) }
            }
        }
    }
    if (choosing) ExportSelectionDialog(client) { choosing = false }
    if (removing) AlertDialog(onDismissRequest = { removing = false }, title = { Text(str(R.string.text_remove_these_system_copies_8936b8b)) }, text = { Text(str(R.string.text_new_file_reads_stop_copies_already_kept_by_other_apps_cannot_be_r_c1ca3c2)) }, confirmButton = { TextButton(onClick = { removing = false; coordinator.removeFiles() }) { Text(str(R.string.text_remove_copies_5546885), color = MaterialTheme.colorScheme.error) } }, dismissButton = { TextButton(onClick = { removing = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
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
                if (parents.size > 1) item { ListRow(str(R.string.text_parent_folder_3af9479), icon = Icons.AutoMirrored.Outlined.ArrowBack) { parents = parents.dropLast(1); coordinator.browseFiles(parents.last()) } }
                items(choices.valueOrNull()?.folders.orEmpty(), key = { "folder:" + it.id }) { folder -> ListRow(folder.name, icon = Icons.Outlined.Folder) { parents = parents + folder.id; coordinator.browseFiles(folder.id) } }
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
        Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(str(R.string.text_a_trusted_browser_may_request_any_selected_website_login_trust_on_d1fa443), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            key(changes) { browsers.forEach { browser ->
                SelectionRow(browser.label, store.trusted(browser.packageName, browser.fingerprints), subtitle = browser.packageName) { value -> if (value) pending = browser else { store.set(browser, false); changes++ } }
            } }
        }
    }, confirmButton = { TextButton(onClick = close) { Text(str(R.string.text_done_9ffd599)) } })
    pending?.let { browser ->
        AlertDialog(
            onDismissRequest = { pending = null },
            title = { Text(str(R.string.text_trust_this_browser_87e0172)) },
            text = {
                Column(Modifier.heightIn(max = 350.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(browser.label, style = MaterialTheme.typography.titleMedium)
                    SelectionContainer { Text(browser.packageName, style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    browser.fingerprints.forEach { SelectionContainer { Text(it, style = MaterialTheme.typography.bodySmall.code()) } }
                    Text(str(R.string.text_it_will_be_trusted_to_identify_websites_for_your_selected_passwor_1858444), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            },
            confirmButton = { TextButton(onClick = { store.set(browser, true); changes++; pending = null }) { Text(str(R.string.text_trust_browser_f229c49)) } },
            dismissButton = { TextButton(onClick = { pending = null }) { Text(str(R.string.text_cancel_998b9c4)) } },
        )
    }
}
