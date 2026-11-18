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
import java.io.File

@Composable fun FilesScreen(client: ClientController, state: UiState, selected: String, select: (String) -> Unit, share: (File, String) -> Unit, trash: Boolean = false) {
    val navigation by client.navigation.state.collectAsStateWithLifecycle()
    val details by client.library.detail.state.collectAsStateWithLifecycle()
    val destination by client.library.destinations.state.collectAsStateWithLifecycle()
    val folder = navigation.current.folder
    var name by remember { mutableStateOf("") }
    var selectedVersion by remember(selected, navigation.current.revision) { mutableStateOf(navigation.current.revision) }
    var rename by remember { mutableStateOf(false) }
    var metadata by remember(selected) { mutableStateOf(false) }
    var versionAction by remember(selected) { mutableStateOf<String?>(null) }
    var move by remember { mutableStateOf(false) }
    var parent by remember { mutableStateOf("root") }
    var parents by remember { mutableStateOf(listOf("root")) }
    var confirmation by remember(selected) { mutableStateOf<String?>(null) }
    LaunchedEffect(selected, state.epoch) { if (selected.isNotEmpty()) client.library.select(selected) }
    val current = details.valueOrNull()?.takeIf { it.file.id == selected }
    val historical = current?.versions?.find { it.id == selectedVersion }
    val file = current?.file?.let { if (selectedVersion != null && historical == null) null else if (historical == null) it else it.atVersion(historical) }
    if (selected.isEmpty()) FileBrowserScreen(client, state, trash, select) else Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(details, { client.library.select(selected) })
        if (current != null && selectedVersion != null && historical == null) {
            Notice(str(R.string.text_the_requested_version_is_unavailable_on_this_device_choose_anothe_4d07481), true)
            TextButton(onClick = { selectedVersion = null }) { Text(str(R.string.text_view_the_current_version_86f22e5)) }
        }
        if (file != null) {
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.Description, null, Modifier.size(40.dp), tint = MaterialTheme.colorScheme.primary)
                Column { Text(file.name, style = MaterialTheme.typography.headlineSmall); Text(fileSize(file.size) + " · " + fileState(file), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
            if (file.trashedAt != null) {
                Notice(str(R.string.text_restore_this_file_before_opening_it_5bf614e))
                Button(onClick = { client.library.restore(file.id) { select("") } }, enabled = !state.working("files:restore:" + file.id)) { Text(str(R.string.text_restore_b04c493)) }
            } else {
                if (file.local) Button(onClick = { client.navigation.push(Route("file-preview", id = file.id, revision = file.version)) }, modifier = Modifier.fillMaxWidth()) { Icon(Icons.Outlined.Visibility, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_preview_verified_contents_49f2c13)) }
                if (file.local) OutlinedButton(onClick = { confirmation = "share" }, enabled = !state.working("file:open:" + file.id), modifier = Modifier.fillMaxWidth()) { Icon(Icons.Outlined.Share, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_share_a_verified_copy_fc77423)) }
                else Notice(str(R.string.text_this_version_has_no_local_copy_restore_its_verified_cloud_backup_4068c35))
                if (file.local && file.backup == null && file.version == current.file.version) OutlinedButton(onClick = { client.navigation.push(Route("file-backup", id = file.id, revision = file.version)) }, enabled = !state.working("file:backup:" + file.id), modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_back_up_this_version_f74abb5)) }
                if (!file.local && file.backup != null) OutlinedButton(onClick = {
                    client.fileWork.restore(file.backup!!.upload, file.folder)
                }, enabled = !state.working("file:restore:" + file.id)) { Text(str(R.string.text_restore_to_this_device_802102f)) }
                if (file.local && file.backup != null) TextButton(onClick = { confirmation = "evict" }) { Text(str(R.string.text_free_local_space_89b2267)) }
                if (file.local || file.backup != null) OutlinedButton(onClick = { client.navigation.push(Route("file-link-compose", id = file.id, revision = file.version)) }, enabled = file.size <= 100uL * 1024u * 1024u, modifier = Modifier.fillMaxWidth()) { Icon(Icons.Outlined.Link, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_share_by_link_05aafef)) }
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    TextButton(onClick = { name = file.name; rename = true }) { Text(str(R.string.text_rename_dd3a2f7)) }
                    TextButton(onClick = { parent = "root"; parents = listOf("root"); client.library.destination(parent); move = true }) { Text(str(R.string.text_move_207d5fe)) }
                }
            }
            if (file.trashedAt == null) TextButton(onClick = { client.library.beginMetadata(current!!.file); metadata = true }) { Text(str(R.string.text_edit_notes_tags_and_favorite_25ab2c0)) }
            SectionTitle(str(R.string.text_file_details_27e9099))
            Text(file.mime, style = MaterialTheme.typography.bodySmall)
            if (file.path.isNotEmpty()) Text(file.path.joinToString(" / "))
            if (file.tags.isNotEmpty()) Text(file.tags.joinToString(" · "))
            if (file.note.isNotEmpty()) SelectionContainer { Text(file.note) }
            SectionTitle("SHA-256"); SelectionContainer { Text(file.sha256, style = MaterialTheme.typography.bodySmall) }
            file.backup?.let { backup ->
                Text(str(R.string.text_backup_saved_0769b59) + messageTime(backup.backedUpAt), style = MaterialTheme.typography.bodySmall)
                Text(if (backup.verified) str(R.string.text_all_chunks_read_back_and_verified_153e284) else str(R.string.text_backup_has_not_been_fully_read_back_30e3810), style = MaterialTheme.typography.bodySmall)
            }
            SectionTitle(str(R.string.text_versions_0ff2bdb))
            current.versions.reversed().forEach { version ->
                RadioChoiceRow(messageTime(version.createdAt) + if (version.current) str(R.string.text_current_de083b6) else "", file.version == version.id,
                    subtitle = fileSize(version.size) + " · " + version.device) { selectedVersion = version.id }
            }
            if (file.version != current.file.version && file.trashedAt == null) {
                OutlinedButton(onClick = { versionAction = "use" }, enabled = !state.working("files:version:${file.id}")) { Text(str(R.string.text_make_this_the_current_version_ddcd50e)) }
                TextButton(onClick = { versionAction = "delete" }, enabled = !state.working("files:version:${file.id}")) { Text(str(R.string.text_delete_this_local_historical_version_f626908), color = MaterialTheme.colorScheme.error) }
                Text(str(R.string.text_selected_sha_256_c773aab) + file.sha256, style = MaterialTheme.typography.labelSmall)
                Text(str(R.string.text_current_sha_256_2b15436) + current.file.sha256, style = MaterialTheme.typography.labelSmall)
                Text(if (file.sha256 == current.file.sha256 && file.size == current.file.size) str(R.string.text_both_recorded_content_digests_match_32aeeb6) else str(R.string.text_the_recorded_content_differs_between_these_versions_b1657eb), style = MaterialTheme.typography.bodySmall)
            }
            if (file.trashedAt == null) TextButton(onClick = { confirmation = "trash" }) { Text(str(R.string.text_move_local_file_to_trash_9600310), color = MaterialTheme.colorScheme.error) }
        }
    }
    if (metadata && current != null) FileMetadataEditor(client, current.file) { metadata = false }
    if (versionAction != null && current != null && file != null) AlertDialog(onDismissRequest = { versionAction = null }, title = { Text(if (versionAction == "use") str(R.string.text_use_this_version_628b39c) else str(R.string.text_delete_this_historical_version_0144ce0)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(file.name); Text(fileSize(file.size)); Text(file.version, style = MaterialTheme.typography.bodySmall)
            Text(if (versionAction == "use") str(R.string.text_the_selected_immutable_version_becomes_current_other_versions_rem_558f51b) else str(R.string.text_this_permanently_removes_this_version_from_the_local_library_its_4a332f6))
        }
    }, confirmButton = { TextButton(onClick = { if (versionAction == "use") client.library.useVersion(current.file, file.version) else client.library.deleteVersion(current.file, file.version); versionAction = null }) { Text(str(R.string.text_confirm_563fd5a)) } }, dismissButton = { TextButton(onClick = { versionAction = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (rename && file != null) AlertDialog(onDismissRequest = { rename = false }, title = { Text(str(R.string.text_rename_dd3a2f7)) }, text = { OutlinedTextField(name, { name = it }, label = { Text(str(R.string.text_name_dd4dc4c)) }) }, confirmButton = {
        TextButton(enabled = name.isNotBlank() && !name.contains('/') && !name.contains('\\') && !state.working("files:rename:${file.id}"), onClick = { client.library.rename(file.id, name) { rename = false } }) { Text(str(R.string.text_save_ec8e6d5)) }
    }, dismissButton = { TextButton(onClick = { rename = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (move && file != null) AlertDialog(onDismissRequest = { move = false }, title = { Text(str(R.string.text_move_to_folder_1c1670d)) }, text = {
        Column(Modifier.heightIn(max = 350.dp).verticalScroll(rememberScrollState())) {
            Text(parents.joinToString(" / "), style = MaterialTheme.typography.labelSmall)
            if (parents.size > 1) TextButton(onClick = { parents = parents.dropLast(1); parent = parents.last(); client.library.destination(parent) }) { Text(str(R.string.text_parent_folder_3af9479)) }
            ReadFeedback(destination, { client.library.destination(parent) })
            destination.valueOrNull()?.folders.orEmpty().forEach { entry -> TextButton(onClick = { parent = entry.id; parents += parent; client.library.destination(parent) }) { Icon(Icons.Outlined.Folder, null); Text(entry.name) } }
        }
    }, confirmButton = { TextButton(onClick = { client.library.move(file.id, parent) { move = false } }, enabled = parent != file.folder && !state.working("files:move:" + file.id)) { Text(str(R.string.text_move_here_2a2705e)) } }, dismissButton = { TextButton(onClick = { move = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (confirmation != null && file != null) AlertDialog(onDismissRequest = { confirmation = null }, title = { Text(when (confirmation) { "share" -> str(R.string.text_share_a_copy_4b1b25d); "backup" -> str(R.string.text_back_up_this_version_179ee9c); "evict" -> str(R.string.text_free_local_space_e8ccb7a); else -> str(R.string.text_move_to_trash_79243d8) }) }, text = {
        Text(when (confirmation) {
            "share" -> str(R.string.text_shared_copies_leave_dmsg_s_protection_and_cannot_be_recalled_84ddf5e)
            "backup" -> str(R.string.text_save_an_encrypted_cloud_copy_of_this_immutable_version_your_local_3f2e4a7)
            "evict" -> str(R.string.text_keep_the_catalog_and_verified_cloud_copy_this_version_must_be_dow_67e3ca1)
            else -> str(R.string.text_this_changes_the_local_library_cloud_backups_are_unchanged_2cc3422)
        })
    }, confirmButton = { TextButton(onClick = {
        when (confirmation) {
            "share" -> client.transfers.share(file, share)
            "backup" -> client.fileWork.backup(file)
            "evict" -> client.perform({ core -> client.operation { core.evictFile(file.id, file.version, it) } }, key = "file:evict:" + file.id) { client.library.select(file.id); client.library.browse() }
            else -> client.library.remove(file.id) { select("") }
        }; confirmation = null
    }) { Text(str(R.string.text_continue_fd19155)) } }, dismissButton = { TextButton(onClick = { confirmation = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

internal fun fileSize(value: ULong): String = when { value >= 1024uL * 1024uL -> "%.1f MiB".format(value.toDouble() / 1048576.0); value >= 1024uL -> "%.0f KiB".format(value.toDouble() / 1024.0); else -> net.dmsg.android.platform.localizedNumber(value) + " B" }
@Composable internal fun fileState(value: LibraryFile): String = when {
    value.trashedAt != null -> str(R.string.text_in_trash_3b5e1bf)
    !value.local && value.backup != null -> str(R.string.text_cloud_only_535ba57)
    value.local && value.backup != null -> str(R.string.text_local_and_backed_up_fdfb784)
    value.local -> str(R.string.text_local_copy_0f3d263)
    else -> str(R.string.text_copy_unavailable_deb9163)
}
