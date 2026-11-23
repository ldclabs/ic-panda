package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun VaultHistoryScreen(client: ClientController, id: String) {
    val read by client.vaultHistory.history.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var restoring by remember(id, state.epoch) { mutableStateOf<Pair<String, String>?>(null) }
    var naming by remember(id, state.epoch) { mutableStateOf<VaultVersion?>(null) }
    LaunchedEffect(id, state.epoch) { client.vaultHistory.load(id) }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item { ReadFeedback(read, { client.vaultHistory.load(id) }) }
        items(read.valueOrNull()?.takeIf { it.current.id == id }?.versions.orEmpty(), key = { it.revision }) { version ->
            Column {
                ListItem(headlineContent = { Text(displayTime(version.at)) }, supportingContent = { Column {
                    if (version.current) Text(str(R.string.text_current_version_b3f03bb))
                    if (version.deleted) Text(str(R.string.text_in_trash_3b5e1bf))
                    if (version.conflict) Text(str(R.string.text_conflicting_version_ffce550))
                    if (!version.synced && version.saveState != "local_only") Text(str(R.string.text_waiting_to_sync_9a0891e))
                    if (!version.deleted && version.changes.isNotEmpty()) Text(version.changes.joinToString(" · ") { historyChange(it) })
                    val deviceName = version.alias ?: if (version.device == state.session?.deviceId) str(R.string.text_this_device_ce825c6)
                        else str(R.string.history_device) + " · " + version.device.take(8)
                    TextButton(onClick = { naming = version }, modifier = Modifier.testTag("vault-version-device:" + version.revision)) { Text(deviceName) }
                } }, modifier = Modifier.testTag("vault-version:" + version.revision).clickable { client.navigation.push(Route("vault-version", id, revision = version.revision)) })
                if (!version.current && !version.deleted && read.valueOrNull()?.current?.reserved == false && read.valueOrNull()?.current?.deleted == false) {
                    TextButton(onClick = { client.navigation.push(Route("vault-compare", id, revision = version.revision)) }) { Text(str(R.string.text_compare_with_current_33863ce)) }
                    TextButton(onClick = { read.valueOrNull()?.current?.revision?.let { head -> restoring = version.revision to head } },
                        enabled = read is ReadState.Ready && !state.working("vault:restore:" + id)) { Text(str(R.string.text_restore_this_version_463fc13)) }
                }
                HorizontalDivider()
            }
        }
        if (read.valueOrNull()?.more == true) item { TextButton(onClick = { client.vaultHistory.load(id, more = true) }, enabled = read !is ReadState.Loading) { Text(str(R.string.text_load_more_4e1d9f8)) } }
    }
    naming?.let { row ->
        var name by remember(row.device) { mutableStateOf(row.alias.orEmpty()) }
        val trimmed = name.trim()
        AlertDialog(onDismissRequest = { naming = null }, title = { Text(str(R.string.text_local_label_c31fe59)) }, text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SelectionContainer { Text(row.device, style = MaterialTheme.typography.bodySmall) }
                OutlinedTextField(name, { name = it }, label = { Text(str(R.string.text_local_label_c31fe59)) }, singleLine = true)
            }
        }, confirmButton = {
            TextButton(enabled = trimmed != row.alias.orEmpty() && trimmed.codePointCount(0, trimmed.length) <= 80 && trimmed.none(Char::isISOControl) && !state.working("vault:history:alias"),
                onClick = { client.vaultHistory.deviceAlias(id, row.device, trimmed) { naming = null } }) { Text(str(R.string.text_save_ec8e6d5)) }
        }, dismissButton = { TextButton(onClick = { naming = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
    }
    restoring?.let { (revision, head) -> AlertDialog(onDismissRequest = { restoring = null }, title = { Text(str(R.string.text_restore_as_a_new_current_version_4bad49d)) }, text = { Text(str(R.string.text_existing_history_stays_a_changed_current_version_requires_another_3a702a6)) }, confirmButton = { TextButton(enabled = !state.working("vault:restore:" + id), onClick = {
        restoring = null
        client.vaultHistory.restore(id, revision, head)
    }) { Text(str(R.string.text_restore_b04c493)) } }, dismissButton = { TextButton(onClick = { restoring = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}

private fun historyChange(code: String) = str(when (code) {
    "created" -> R.string.text_created_36ba304
    "restored" -> R.string.text_restore_b04c493
    "title" -> R.string.text_title_4c18fa6
    "type" -> R.string.text_type_6172196
    "password" -> R.string.text_password_347d7fc
    "fields" -> R.string.history_fields
    "notes" -> R.string.text_notes_f7df47b
    "tags" -> R.string.text_tags_33e9c4d
    "attachments" -> R.string.text_attachments_be5733e
    "favorite" -> R.string.text_favorite_898f4c6
    "conflict" -> R.string.text_conflicting_version_ffce550
    else -> R.string.text_unavailable_4ae32d2
})

@Composable fun VaultComparisonScreen(client: ClientController, id: String, revision: String) {
    val read by client.vaultHistory.comparison.state.collectAsStateWithLifecycle()
    LaunchedEffect(id, revision) { client.vaultHistory.compare(id, revision) }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ReadFeedback(read, { client.vaultHistory.compare(id, revision) })
        read.valueOrNull()?.let { (current, previous) ->
            ComparisonRow(str(R.string.text_title_4c18fa6), current.title, previous.title)
            ComparisonRow(str(R.string.text_type_6172196), vaultKindName(current.kind), vaultKindName(previous.kind))
            ComparisonRow(str(R.string.text_favorite_898f4c6), if (current.favorite) str(R.string.text_yes_4e00e01) else str(R.string.text_no_739dd58), if (previous.favorite) str(R.string.text_yes_4e00e01) else str(R.string.text_no_739dd58))
            ComparisonRow(str(R.string.text_notes_f7df47b), current.notes, previous.notes)
            ComparisonRow(str(R.string.text_tags_33e9c4d), current.tags.joinToString(", "), previous.tags.joinToString(", "))
            ComparisonRow(str(R.string.text_attachments_be5733e), current.files.joinToString("\n"), previous.files.joinToString("\n"))
            (current.fields.map { it.id } + previous.fields.map { it.id }).distinct().forEach { field ->
                val a = current.fields.find { it.id == field }; val b = previous.fields.find { it.id == field }
                ComparisonRow(str(R.string.text_field_settings_ab97288), a?.let { listOf(it.section, vaultFieldTitle(it.id, it.kind, it.label), fieldKindName(it.kind)).filter(String::isNotBlank).joinToString(" · ") } ?: "—", b?.let { listOf(it.section, vaultFieldTitle(it.id, it.kind, it.label), fieldKindName(it.kind)).filter(String::isNotBlank).joinToString(" · ") } ?: "—")
                if (a?.concealed == true || b?.concealed == true) { SectionTitle(a?.label?.ifBlank { field } ?: b?.label?.ifBlank { field } ?: field); Notice(str(R.string.text_hidden_values_stay_concealed_open_an_individual_version_to_reveal_e418210)) }
                else ComparisonRow(a?.label?.ifBlank { field } ?: field, a?.value ?: "—", b?.value ?: "—")
            }
            TextButton(onClick = { client.navigation.push(Route("vault-version", id, revision = current.revision)) }) { Text(str(R.string.text_inspect_current_version_18092d2)) }
            TextButton(onClick = { client.navigation.push(Route("vault-version", id, revision = revision)) }) { Text(str(R.string.text_inspect_historical_version_0a7ca5c)) }
        }
    }
}
@Composable private fun ComparisonRow(title: String, current: String, previous: String) {
    DetailCard {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            Text(title, Modifier.semantics { heading() }, style = MaterialTheme.typography.titleMedium)
            if (current != previous) Text(str(R.string.text_updated_9306ca8), style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary)
        }
        SelectionContainer { LabelValue(str(R.string.text_current_version_b3f03bb), current.ifEmpty { "—" }) }
        HorizontalDivider()
        SelectionContainer { LabelValue(str(R.string.text_historical_version_119b50c), previous.ifEmpty { "—" }) }
    }
}

@Composable fun VaultConflictsScreen(client: ClientController, selected: String?, select: (String) -> Unit) {
    val read by client.vaultHistory.conflicts.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val conflict = read.valueOrNull()?.find { it.key == selected }
    val choices by client.vaultHistory.choices.collectAsStateWithLifecycle()
    val choice = conflict?.let { value -> choices[value.key]?.takeIf { it.expected == value.current.revision } ?: ConflictChoice(value.current.revision) }
    val picks = choice?.picks.orEmpty()
    val merging = choice?.merging == true
    var confirmation by remember(selected, conflict?.current?.revision) { mutableStateOf<String?>(null) }
    LaunchedEffect(state.epoch) { client.vaultHistory.conflicts() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(str(R.string.text_version_conflicts_0df1b3c), modifier = Modifier.openedReminderVisible(client, read is ReadState.Ready || read is ReadState.Empty) { it.kind == "task" && it.id?.startsWith("conflicts:") == true }, style = MaterialTheme.typography.titleMedium)
        ReadFeedback(read, client.vaultHistory::conflicts)
        if (conflict == null) {
            if (read is ReadState.Empty) EmptyPanel(str(R.string.text_no_conflicts_f2a54d9))
            read.valueOrNull().orEmpty().forEach { value -> AccountRow(value.current.title + " · " + str(R.string.text_two_versions_need_review_df5bf89)) { select(value.key) } }
        } else {
            Notice(str(R.string.text_review_both_versions_the_selected_parts_replace_current_values_ex_042fcc1))
            Text(conflict.current.title, style = MaterialTheme.typography.headlineSmall)
            TextButton(onClick = { client.navigation.push(Route("vault-version", conflict.current.id, revision = conflict.current.revision)) }) { Text(str(R.string.text_inspect_current_version_18092d2)) }
            TextButton(onClick = { client.navigation.push(Route("vault-version", conflict.copy.id, revision = conflict.copy.revision)) }) { Text(str(R.string.text_inspect_conflicting_version_1159e10)) }
            if (conflict.current.deleted || conflict.copy.deleted) Notice(str(R.string.text_one_version_is_a_deletion_choose_the_complete_version_to_retain_955b569))
            else SettingToggle(str(R.string.text_merge_individual_fields_5404741), merging, { client.vaultHistory.choose(conflict, choice!!.copy(merging = it)) })
            if (merging) conflictParts(conflict).forEach { part ->
                Column {
                    SelectionRow(part.id.removePrefix("field:"), part.id in picks) { checked -> client.vaultHistory.choose(conflict, choice!!.copy(picks = if (checked) picks + part.id else picks - part.id)) }
                    Text(str(R.string.text_current_ce9f53f) + part.current)
                    Text(str(R.string.text_conflicting_c6e8ea3) + part.other)
                    if (part.hidden) Text(str(R.string.text_hidden_values_differ_f604201), style = MaterialTheme.typography.labelSmall)
                    HorizontalDivider()
                }
            }
            val enabled = !state.working("vault:resolve:" + conflict.key) && read is ReadState.Ready
            if (merging) Button(onClick = { confirmation = "merge" }, enabled = enabled) { Text(str(R.string.text_save_merged_version_b601e29)) }
            else { Button(onClick = { confirmation = "current" }, enabled = enabled) { Text(str(R.string.text_keep_current_version_a9f327b)) }; OutlinedButton(onClick = { confirmation = "other" }, enabled = enabled) { Text(str(R.string.text_use_conflicting_version_dff218a)) } }
        }
    }
    if (confirmation != null && conflict != null) AlertDialog(onDismissRequest = { confirmation = null }, title = { Text(str(R.string.text_save_the_reviewed_version_a2ead70)) }, confirmButton = { TextButton(onClick = {
        if (confirmation == "merge") client.vaultHistory.merge(conflict, picks) { confirmation = null; select("") }
        else client.vaultHistory.resolve(conflict, confirmation == "current") { confirmation = null; select("") }
    }) { Text(str(R.string.text_save_ec8e6d5)) } }, dismissButton = { TextButton(onClick = { confirmation = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
