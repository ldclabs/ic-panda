package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun VaultImportScreen(client: ClientController, choose: () -> Unit) {
    val form by client.vaultImport.form.collectAsStateWithLifecycle()
    val read by client.vaultImport.preview.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val preview = read.valueOrNull()
    val working = state.working("vault:import-preview") || state.working("vault:import-commit")
    val editable = !working && form?.pending == null && form?.completed == null
    Column(Modifier.fillMaxSize()) {
    LazyColumn(Modifier.weight(1f), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Notice(str(R.string.text_import_an_unencrypted_1password_bitwarden_or_browser_password_exp_4f960de))
            Button(onClick = { if (client.vaultImport.begin()) choose() }, enabled = !working && form?.pending == null) { Text(str(R.string.text_choose_password_export_100c6c6)) }
            Text(str(R.string.text_source_limit_64_mib_archive_attachments_passkeys_and_history_are_7b86dbc), style = MaterialTheme.typography.bodySmall)
            if (read !is ReadState.Idle) ReadFeedback(read, client.vaultImport::retry)
            if (working) {
                LinearProgressIndicator(Modifier.fillMaxWidth())
                if (state.working("vault:import-preview")) TextButton(onClick = client.vaultImport::cancelPreview) { Text(str(R.string.text_cancel_preview_a89c579)) }
            }
        }
        form?.completed?.let { imported ->
            item { SectionTitle(str(R.string.text_imported_on_this_device_3a765bd)) }
            items(imported, key = { it.first }) { (id, title) -> TextButton(onClick = { client.navigation.push(Route("vault", id)) }) { Text(title) } }
        }
        if (form?.pending != null) item {
            Notice(str(R.string.text_keep_this_original_selection_while_its_result_is_unconfirmed_retr_a3c38d3))
            Button(onClick = client.vaultImport::commit, enabled = !working) { Text(str(R.string.text_retry_original_import_c3a9967)) }
        }
        if (form != null && form?.completed == null && preview == null && !working && read is ReadState.Idle) item {
            TextButton(onClick = client.vaultImport::retry) { Text(str(R.string.text_retry_preview_8e414a8)) }
        }
        if (preview != null) {
            item {
                SectionTitle(preview.format)
                Text(str(R.string.text_total_9e5ff1f) + localizedNumber(preview.items.size) + str(R.string.text_already_present_b9416d8) + localizedNumber(preview.items.count { it.duplicate }))
                Text(str(R.string.text_skipped_901756c) + localizedNumber(preview.skipped) + str(R.string.text_incomplete_542ffc1) + localizedNumber(preview.incomplete))
                Text(str(R.string.text_selected_26244a5) + localizedNumber(form?.selected?.size ?: 0))
                if (preview.skipped > 0u || preview.incomplete > 0u) Notice(str(R.string.text_some_records_or_properties_are_unsupported_review_the_preview_bef_b26699b))
                TextButton(onClick = { client.vaultImport.edit { it.copy(selected = preview.items.filterNot(VaultImportItem::duplicate).take(1000).map { it.index }.toSet()) } }, enabled = editable) { Text(str(R.string.text_select_non_duplicates_c01238f)) }
                TextButton(onClick = { client.vaultImport.edit { it.copy(selected = emptySet()) } }, enabled = editable) { Text(str(R.string.text_clear_selection_66ecd44)) }
                OutlinedTextField(form?.query.orEmpty(), { query -> client.vaultImport.edit { it.copy(query = query) } }, Modifier.fillMaxWidth(), enabled = editable,
                    label = { Text(str(R.string.text_filter_title_or_username_8e4df4f)) }, singleLine = true)
                Text(str(R.string.text_select_up_to_1_000_records_per_import_9a20398), style = MaterialTheme.typography.bodySmall)
            }
            items(preview.items.filter { form?.query.orEmpty().isBlank() || it.title.contains(form?.query.orEmpty(), true) || it.username.contains(form?.query.orEmpty(), true) }, key = { it.index.toLong() }) { item ->
                SelectionRow(item.title, item.index in form?.selected.orEmpty(), editable, item.username + if (item.duplicate) " · " + str(R.string.text_already_present_d7b7081) else "") { checked ->
                    client.vaultImport.edit { it.copy(selected = if (checked) it.selected + item.index else it.selected - item.index) }
                }
            }
        }
    }
    if (preview != null && form?.completed == null) {
        Button(onClick = client.vaultImport::commit, enabled = editable && (form?.selected?.size ?: 0) in 1..1000,
            modifier = Modifier.fillMaxWidth().padding(horizontal = pageMargin(), vertical = 12.dp)) {
            Text(str(R.string.text_import_selected_records_1f3e87e))
        }
    }
    }
}
