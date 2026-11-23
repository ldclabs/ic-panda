package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import net.dmsg.android.platform.localizedNumber
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun CloudFilesScreen(client: ClientController, selected: String, showScopes: Boolean) {
    val state by client.state.collectAsStateWithLifecycle()
    val inventory by client.cloudFiles.inventory.state.collectAsStateWithLifecycle()
    var query by remember { mutableStateOf("") }
    var sort by remember { mutableStateOf("date") }
    var destination by remember(selected) { mutableStateOf(emptyList<LibraryFolder>()) }
    val folder = destination.folderId
    var choosing by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    val saved = inventory.valueOrNull()
    val current = saved?.items?.find { it.id == selected }
    LaunchedEffect(state.epoch) { client.cloudFiles.browse(false, refreshAfter = true) }
    val entries = saved?.items.orEmpty().filter { query.isBlank() || it.name.orEmpty().contains(query, true) || it.path.joinToString("/").contains(query, true) }
        .let { values -> when (sort) { "name" -> values.sortedBy { it.name.orEmpty().lowercase() }; "size" -> values.sortedByDescending { it.size }; else -> values.sortedByDescending { it.committedAt ?: it.createdAt } } }
    val refresh: @Composable () -> Unit = {
        IconButton(onClick = { client.cloudFiles.browse() }, enabled = inventory !is ReadState.Loading) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_backup_list_3104cef))
        }
    }
    val content: @Composable (Boolean) -> Unit = { searching ->
        LazyColumn(Modifier.fillMaxSize(), state = if (selected.isEmpty()) workspaceListState(client.navigation, "cloud-files:list") else rememberLazyListState(),
            contentPadding = pagePadding(top = 0.dp, bottomInset = LocalWorkspaceBarInset.current)) {
            if (selected.isEmpty()) browseScopesItem(searching) {
                BrowseScopes(trailing = {
                    SortMenu(sort, listOf("date" to R.string.text_newest_31c84cc, "name" to R.string.text_name_dd4dc4c, "size" to R.string.text_largest_4d2cb3c)) { sort = it }
                }) {
                    if (showScopes) BrowseChoice(str(R.string.text_on_this_device_6032118), false, onClick = client.navigation::back)
                    BrowseChoice(str(R.string.text_cloud_backups_ab4daf1), true) {}
                }
            }
            item {
                if (selected.isNotEmpty()) Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(str(R.string.text_encrypted_cloud_copies_9caa6c8), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                    refresh()
                }
                saved?.observedAt?.let { Text(str(R.string.text_last_refreshed_fb86c19) + messageTime(it), style = MaterialTheme.typography.labelSmall) }
                ReadFeedback(inventory, { client.cloudFiles.browse() })
                if (saved?.complete != true) Notice(str(R.string.text_the_cloud_inventory_has_not_been_fully_read_yet_an_empty_list_doe_b6ae09e))
            }
            if (selected.isEmpty()) {
                items(entries, key = { it.id }) { item ->
                    BrowseRow(item.name ?: str(R.string.text_encrypted_backup_2ad5848), Modifier.clickable { client.navigation.select(item.id) },
                        leading = { BrowseIcon(if (item.needsRoot) R.drawable.glyph_key else R.drawable.glyph_sync) }) {
                        Text(listOfNotNull(item.size?.let(::fileSize), str(if (item.needsRoot) R.string.text_waiting_for_the_required_content_key_1d9169d
                            else if (item.local) R.string.text_also_on_this_device_7d07aa6 else R.string.text_only_in_cloud_6a03abd)).joinToString(" · "))
                    }
                }
                if (entries.isEmpty() && saved?.complete == true && inventory !is ReadState.Failed) item {
                    EmptyState(if (query.isBlank()) str(R.string.text_no_cloud_backups_27be5f2) else str(R.string.text_no_matching_backups_575e7e5), glyph = if (query.isBlank()) R.drawable.glyph_sync else null)
                }
                item { Notice(str(R.string.text_back_up_selected_files_explicitly_your_local_library_is_not_uploa_0e590c9)) }
            } else if (current != null) {
                item {
                    Text(current.name ?: str(R.string.text_encrypted_backup_2ad5848), style = MaterialTheme.typography.headlineSmall)
                    current.size?.let { Text(fileSize(it)) }
                    Text(current.path.joinToString(" / "), style = MaterialTheme.typography.bodySmall)
                    Text(str(R.string.text_source_device_abc1e1a) + current.device, style = MaterialTheme.typography.bodySmall)
                    Text(str(R.string.text_created_ae8a09c) + messageTime(current.createdAt), style = MaterialTheme.typography.bodySmall)
                    current.sha256?.let { SelectionContainer { Text("SHA-256\n$it", style = MaterialTheme.typography.bodySmall) } }
                }
                item {
                    when {
                        current.needsRoot -> {
                            Notice(str(R.string.text_synchronize_the_account_to_obtain_the_content_key_before_restorin_ac85e0e))
                            TextButton(onClick = { client.navigation.push(Route("sync")) }) { Text(str(R.string.text_account_synchronization_77ccb72)) }
                        }
                        current.local && current.file != null -> Button(onClick = { client.navigation.push(Route("files", id = current.file, revision = current.version)) }) { Text(str(R.string.text_open_local_details_ccd7b04)) }
                        else -> {
                            TextButton(onClick = { choosing = true }) { Text(str(R.string.text_restore_into_9068fec) + destination.folderLabel) }
                            OutlinedButton(onClick = { client.backgroundFiles.download(current.id, folder) }, enabled = !state.working("cipher:prepare")) { Text(str(R.string.text_download_ciphertext_in_background_bd6e2c4)) }
                            Button(onClick = { client.fileWork.restore(current.id, folder) }, enabled = !state.working("file:restore:${current.id}")) { Text(str(R.string.text_restore_this_copy_5a61a44)) }
                        }
                    }
                }
                item {
                    if (!current.needsRoot && current.size != null && current.size <= 100uL * 1024u * 1024u) OutlinedButton(onClick = { client.navigation.push(Route("backup-link-compose", id = current.id)) }) { Icon(Icons.Outlined.Link, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_share_by_link_05aafef)) }
                    TextButton(onClick = { client.navigation.push(Route("file-transfers")) }) { Text(str(R.string.text_file_transfers_2b83297)) }
                    TextButton(onClick = { deleting = true }, enabled = !state.working("cloud:delete:${current.id}")) { Text(str(R.string.text_delete_cloud_copy_2cefb18), color = MaterialTheme.colorScheme.error) }
                }
            } else if (inventory !is ReadState.Loading) item { Notice(str(R.string.text_this_backup_is_no_longer_in_the_current_inventory_090f582)) }
        }
    }
    if (selected.isEmpty()) WorkspaceBrowser(str(R.string.text_cloud_backups_ab4daf1), query, { query = it },
        str(R.string.text_search_name_or_path_55a6b79), actions = { refresh() }, content = content)
    else content(false)
    if (choosing) FileFolderDialog(client, str(R.string.text_restore_destination_431edf6), { choosing = false }, initial = destination) {
        destination = it; choosing = false
    }
    if (deleting && current != null) AlertDialog(onDismissRequest = { deleting = false }, title = { Text(str(R.string.text_delete_this_cloud_copy_79eadc7)) }, text = {
        Text(if (current.local) str(R.string.text_this_deletes_only_the_selected_cloud_copy_local_copies_remain_on_e5dfe9c) else str(R.string.text_this_device_has_no_local_copy_this_may_be_your_only_recoverable_c_e3959bb))
    }, confirmButton = { TextButton(onClick = { client.cloudFiles.delete(current) { deleting = false; client.navigation.back() } }, enabled = !state.working("cloud:delete:${current.id}")) { Text(str(R.string.text_delete_cloud_copy_2cefb18)) } }, dismissButton = { TextButton(onClick = { deleting = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable fun UnbackedFilesScreen(client: ClientController) {
    val state by client.cloudFiles.unbacked.state.collectAsStateWithLifecycle()
    val batch by client.cloudFiles.batch.collectAsStateWithLifecycle()
    val clientState by client.state.collectAsStateWithLifecycle()
    val choosing = state is ReadState.Ready && !clientState.working("cloud:backup-batch")
    val groups = state.valueOrNull().orEmpty()
    val selected = groups.flatMap { group -> group.versions.filter { BackupVersionKey(group.file.id, it.id) in batch.selected } }
    LaunchedEffect(Unit) { client.cloudFiles.listUnbacked() }
    Column(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.weight(1f), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            item { ReadFeedback(state, client.cloudFiles::listUnbacked) }
            if (state is ReadState.Empty) item { EmptyState(str(R.string.backup_all_caught_up)) }
            groups.forEach { group ->
                item { SectionTitle((group.file.path + group.file.name).joinToString(" / ")) }
                items(group.versions.asReversed(), key = { group.file.id + ":" + it.id }) { v ->
                    val key = BackupVersionKey(group.file.id, v.id)
                    val version = versionLabel(v.current, v.createdAt)
                    val details = listOfNotNull(fileSize(v.size), str(R.string.text_in_progress_6906b67).takeIf { v.status == "backing_up" }).joinToString(" · ")
                    SelectionRow(title = version, checked = key in batch.selected, subtitle = details,
                        enabled = choosing, modifier = Modifier.testTag("unbacked-version:" + group.file.id + ":" + v.id),
                        accessibilityLabel = listOf((group.file.path + group.file.name).joinToString(" / "), version, details).joinToString(" · ")) { client.cloudFiles.choose(key, it) }
                }
            }
        }
        if (groups.isNotEmpty()) Button(onClick = client.cloudFiles::reviewSelection, enabled = selected.isNotEmpty() && choosing,
            modifier = Modifier.fillMaxWidth().padding(pagePadding()).testTag("backup-review-selection")) {
            Text(str(R.string.text_back_up_to_cloud_ee48ac0) + if (selected.isEmpty()) "" else " · " + localizedNumber(selected.size) + " · " + fileSize(selected.sumOf { it.size }))
        }
    }
}

private fun versionLabel(current: Boolean, at: ULong) =
    str(if (current) R.string.text_current_version_b3f03bb else R.string.text_historical_version_119b50c) + " · " + messageTime(at)

@Composable fun BatchBackupScreen(client: ClientController) {
    val batch by client.cloudFiles.batch.collectAsStateWithLifecycle()
    val capacity by client.cloudFiles.capacity.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val busy = state.working("cloud:backup-batch")
    val limits = (capacity as? ReadState.Ready)?.value
    LaunchedEffect(Unit) { client.cloudFiles.capacity() }
    Column(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.weight(1f), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item {
                Notice(str(R.string.backup_selected_help))
                LabelValue(str(R.string.text_versions_0ff2bdb), localizedNumber(batch.review.size))
                LabelValue(str(R.string.backup_total_size), fileSize(batch.review.sumOf { it.file.size }))
                ReadFeedback(capacity, client.cloudFiles::capacity)
                limits?.let { value ->
                    Text(str(R.string.text_maximum_per_file_5718825) + (value.fileLimit?.let(::fileSize) ?: str(R.string.text_not_verified_e9586ea)))
                    Text(str(R.string.text_cloud_space_remaining_c2a586d) + (value.remaining?.let(::fileSize) ?: str(R.string.text_unknown_d748bba)))
                    value.remaining?.takeIf { batch.bytesToReserve > it }?.let { remaining ->
                        Notice(str(R.string.backup_total_size) + " · " + fileSize(batch.bytesToReserve) + "\n" +
                            str(R.string.text_cloud_space_remaining_c2a586d) + fileSize(remaining), true)
                    }
                }
            }
            items(batch.review, key = { it.file.id + ":" + it.file.version }) { row ->
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(row.path, style = MaterialTheme.typography.titleSmall)
                    Text(versionLabel(row.current, row.at) + " · " + fileSize(row.file.size))
                    if (row.file.status != "backing_up" && limits?.fileLimit?.let { row.file.size > it } == true) Notice(str(R.string.text_this_version_exceeds_your_current_backup_limit_9a1b414), true)
                    batch.outcomes[row.key]?.let { outcome -> Text(when (outcome.code) {
                        "queued" -> str(R.string.text_queued_2afa45a)
                        "backed_up" -> str(R.string.backup_already_saved)
                        "in_progress" -> str(R.string.text_in_progress_6906b67)
                        else -> errorText(outcome.code)
                    }, color = if (outcome.accepted) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error) }
                }
            }
            item { TextButton(onClick = { client.navigation.push(Route("file-transfers")) }) { Text(str(R.string.text_file_transfers_2b83297)) } }
        }
        if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
        if (batch.pending.isNotEmpty()) Button(onClick = client.cloudFiles::queueReviewed, enabled = !busy && limits?.let(batch::fits) == true,
            modifier = Modifier.fillMaxWidth().padding(pagePadding()).testTag("backup-queue-selection")) {
            Text(str(if (batch.outcomes.isEmpty()) R.string.text_queue_encrypted_backup_bd8aaaf else R.string.text_retry_563171c))
        }
    }
}

@Composable fun FileBackupScreen(client: ClientController, id: String, version: String?, immutableRevision: Boolean = false) {
    val state by client.state.collectAsStateWithLifecycle()
    val capacity by client.cloudFiles.capacity.state.collectAsStateWithLifecycle()
    val details by client.library.detail.state.collectAsStateWithLifecycle()
    val reviewed = details.valueOrNull()?.takeIf { it.file.id == id && it.file.trashedAt == null }
    val revision = reviewed?.versions?.find { it.id == version }
    val file = if (immutableRevision) revision?.let { reviewed.file.atVersion(it) }?.takeIf { it.local && it.backup == null }
        else reviewed?.file?.takeIf { it.version == version && it.local && it.backup == null }
    val limits = capacity.valueOrNull()
    LaunchedEffect(id, state.epoch) { client.library.select(id); client.cloudFiles.capacity() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Notice(str(R.string.text_save_an_encrypted_cloud_copy_of_this_version_your_local_copy_stay_b3a0064))
        ReadFeedback(details, { client.library.select(id) }); ReadFeedback(capacity, client.cloudFiles::capacity)
        if (file != null) { Text(file.name, style = MaterialTheme.typography.headlineSmall); Text(fileSize(file.size))
            revision?.let { Text(str(if (it.current) R.string.text_current_version_b3f03bb else R.string.text_historical_version_119b50c) + " · " + messageTime(it.createdAt)) }
        } else if (details is ReadState.Ready) Notice(str(R.string.text_the_requested_version_is_unavailable_on_this_device_choose_anothe_4d07481))
        Text(str(R.string.text_maximum_per_file_5718825) + (limits?.fileLimit?.let(::fileSize) ?: str(R.string.text_not_verified_e9586ea)))
        Text(str(R.string.text_cloud_space_remaining_c2a586d) + (limits?.remaining?.let(::fileSize) ?: str(R.string.text_unknown_d748bba)))
        if (file != null && limits?.fileLimit != null && file.size > limits.fileLimit) Notice(str(R.string.text_this_version_exceeds_your_current_backup_limit_9a1b414), true)
        Button(onClick = { file?.let { client.fileWork.backup(it, immutableRevision) } }, enabled = file != null && limits?.fileLimit != null && file.size <= limits.fileLimit && capacity !is ReadState.Loading && capacity !is ReadState.Stale && capacity !is ReadState.Failed && !state.working("file:backup:$id"), modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_queue_encrypted_backup_bd8aaaf)) }
        if (!immutableRevision || revision?.current == true) OutlinedButton(onClick = { file?.let(client.backgroundFiles::upload) }, enabled = file != null && limits?.fileLimit != null && file.size <= limits.fileLimit && capacity is ReadState.Ready && !state.working("cipher:prepare"), modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_upload_ciphertext_in_background_7f3ae2e)) }
        Text(str(R.string.text_background_tasks_can_continue_after_locking_the_system_receives_o_80cbbad), style = MaterialTheme.typography.bodySmall)
        TextButton(onClick = { client.navigation.push(Route("file-transfers")) }) { Text(str(R.string.text_file_transfers_2b83297)) }
    }
}
