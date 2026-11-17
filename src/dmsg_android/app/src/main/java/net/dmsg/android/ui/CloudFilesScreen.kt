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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun CloudFilesScreen(client: ClientController, selected: String) {
    val state by client.state.collectAsStateWithLifecycle()
    val inventory by client.cloudFiles.inventory.state.collectAsStateWithLifecycle()
    val destinations by client.library.destinations.state.collectAsStateWithLifecycle()
    var query by remember { mutableStateOf("") }
    var sort by remember { mutableStateOf("date") }
    var folder by remember(selected) { mutableStateOf("root") }
    var parents by remember(selected) { mutableStateOf(listOf("root")) }
    var choosing by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    val saved = inventory.valueOrNull()
    val current = saved?.items?.find { it.id == selected }
    LaunchedEffect(state.epoch) { client.cloudFiles.browse(false, refreshAfter = true) }
    val entries = saved?.items.orEmpty().filter { query.isBlank() || it.name.orEmpty().contains(query, true) || it.path.joinToString("/").contains(query, true) }
        .let { values -> when (sort) { "name" -> values.sortedBy { it.name.orEmpty().lowercase() }; "size" -> values.sortedByDescending { it.size }; else -> values.sortedByDescending { it.committedAt ?: it.createdAt } } }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(str(R.string.text_encrypted_cloud_copies_9caa6c8), style = MaterialTheme.typography.titleLarge)
                    saved?.observedAt?.let { Text(str(R.string.text_last_refreshed_fb86c19) + messageTime(it), style = MaterialTheme.typography.labelSmall) }
                }
                IconButton(onClick = { client.cloudFiles.browse() }, enabled = inventory !is ReadState.Loading) { Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_backup_list_3104cef)) }
            }
            ReadFeedback(inventory, { client.cloudFiles.browse() })
            if (saved?.complete != true) Notice(str(R.string.text_the_cloud_inventory_has_not_been_fully_read_yet_an_empty_list_doe_b6ae09e))
        }
        if (selected.isEmpty()) {
            item {
                OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(str(R.string.text_search_name_or_path_55a6b79)) }, leadingIcon = { Icon(Icons.Outlined.Search, null) })
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf("date", "name", "size").forEach { option ->
                        FilterChip(sort == option, { sort = option }, {
                            Text(when (option) { "name" -> str(R.string.text_name_dd4dc4c); "size" -> str(R.string.text_largest_4d2cb3c); else -> str(R.string.text_newest_31c84cc) })
                        })
                    }
                }
            }
            items(entries, key = { it.id }) { item ->
                ListItem(modifier = Modifier.clickable { client.navigation.select(item.id) }, headlineContent = { Text(item.name ?: str(R.string.text_encrypted_backup_2ad5848), maxLines = 2, overflow = TextOverflow.Ellipsis) },
                    leadingContent = { Icon(if (item.needsRoot) Icons.Outlined.Key else Icons.Outlined.Cloud, null) },
                    supportingContent = { Column {
                        item.size?.let { Text(fileSize(it)) }
                        Text(if (item.needsRoot) str(R.string.text_waiting_for_the_required_content_key_1d9169d) else if (item.local) str(R.string.text_also_on_this_device_7d07aa6) else str(R.string.text_only_in_cloud_6a03abd))
                    } })
                HorizontalDivider()
            }
            if (entries.isEmpty() && saved?.complete == true && inventory !is ReadState.Failed) item {
                EmptyPanel(if (query.isBlank()) str(R.string.text_no_cloud_backups_27be5f2) else str(R.string.text_no_matching_backups_575e7e5))
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
                        TextButton(onClick = { client.library.destination(folder); choosing = true }) { Text(str(R.string.text_restore_into_9068fec) + if (folder == "root") str(R.string.text_files_c79ddc2) else folder) }
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
    if (choosing) AlertDialog(onDismissRequest = { choosing = false }, title = { Text(str(R.string.text_restore_destination_431edf6)) }, text = {
        Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState())) {
            if (parents.size > 1) TextButton(onClick = { parents = parents.dropLast(1); folder = parents.last(); client.library.destination(folder) }) { Text(str(R.string.text_parent_folder_3af9479)) }
            ReadFeedback(destinations, { client.library.destination(folder) })
            destinations.valueOrNull()?.takeIf { it.folder == folder }?.folders.orEmpty().forEach { entry -> TextButton(onClick = { folder = entry.id; parents += folder; client.library.destination(folder) }) { Icon(Icons.Outlined.Folder, null); Text(entry.name) } }
        }
    }, confirmButton = { TextButton(onClick = { choosing = false }) { Text(str(R.string.text_use_this_folder_4a67a24)) } })
    if (deleting && current != null) AlertDialog(onDismissRequest = { deleting = false }, title = { Text(str(R.string.text_delete_this_cloud_copy_79eadc7)) }, text = {
        Text(if (current.local) str(R.string.text_this_deletes_only_the_selected_cloud_copy_local_copies_remain_on_e5dfe9c) else str(R.string.text_this_device_has_no_local_copy_this_may_be_your_only_recoverable_c_e3959bb))
    }, confirmButton = { TextButton(onClick = { client.cloudFiles.delete(current) { deleting = false; client.navigation.back() } }, enabled = !state.working("cloud:delete:${current.id}")) { Text(str(R.string.text_delete_cloud_copy_2cefb18)) } }, dismissButton = { TextButton(onClick = { deleting = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable fun FileBackupScreen(client: ClientController, id: String, version: String?) {
    val state by client.state.collectAsStateWithLifecycle()
    val capacity by client.cloudFiles.capacity.state.collectAsStateWithLifecycle()
    val details by client.library.detail.state.collectAsStateWithLifecycle()
    val file = details.valueOrNull()?.file?.takeIf { it.id == id && it.version == version }
    val limits = capacity.valueOrNull()
    LaunchedEffect(id, state.epoch) { client.library.select(id); client.cloudFiles.capacity() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Notice(str(R.string.text_save_an_encrypted_cloud_copy_of_this_version_your_local_copy_stay_b3a0064))
        ReadFeedback(details, { client.library.select(id) }); ReadFeedback(capacity, client.cloudFiles::capacity)
        if (file != null) { Text(file.name, style = MaterialTheme.typography.headlineSmall); Text(fileSize(file.size)) }
        Text(str(R.string.text_maximum_per_file_5718825) + (limits?.fileLimit?.let(::fileSize) ?: str(R.string.text_not_verified_e9586ea)))
        Text(str(R.string.text_cloud_space_remaining_c2a586d) + (limits?.remaining?.let(::fileSize) ?: str(R.string.text_unknown_d748bba)))
        if (file != null && limits?.fileLimit != null && file.size > limits.fileLimit) Notice(str(R.string.text_this_version_exceeds_your_current_backup_limit_9a1b414), true)
        Button(onClick = { file?.let(client.fileWork::backup) }, enabled = file != null && limits?.fileLimit != null && file.size <= limits.fileLimit && capacity !is ReadState.Loading && capacity !is ReadState.Stale && capacity !is ReadState.Failed && !state.working("file:backup:$id"), modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_queue_encrypted_backup_bd8aaaf)) }
        OutlinedButton(onClick = { file?.let(client.backgroundFiles::upload) }, enabled = file != null && limits?.fileLimit != null && file.size <= limits.fileLimit && capacity is ReadState.Ready && !state.working("cipher:prepare"), modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_upload_ciphertext_in_background_7f3ae2e)) }
        Text(str(R.string.text_background_tasks_can_continue_after_locking_the_system_receives_o_80cbbad), style = MaterialTheme.typography.bodySmall)
        TextButton(onClick = { client.navigation.push(Route("file-transfers")) }) { Text(str(R.string.text_file_transfers_2b83297)) }
    }
}
