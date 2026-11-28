package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import androidx.compose.ui.res.painterResource
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.numericExpression
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.DriveFileMove
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

/** Each page reads its own [page] route, so a page leaving the screen keeps browsing its folder until it is gone. */
@Composable fun FilesScreen(client: ClientController, state: UiState, page: Route, select: (String) -> Unit, share: (File, String) -> Unit, trash: Boolean = false) {
    val details by client.library.detail.state.collectAsStateWithLifecycle()
    val selected = page.id.orEmpty()
    val folder = page.folder
    var selectedVersion by remember(selected, page.revision) { mutableStateOf(page.revision) }
    var rename by remember { mutableStateOf(false) }
    var metadata by remember(selected) { mutableStateOf(false) }
    var versionAction by remember(selected) { mutableStateOf<String?>(null) }
    var move by remember { mutableStateOf(false) }
    var moveError by remember(selected) { mutableStateOf<Pair<String, String>?>(null) }
    var confirmation by remember(selected) { mutableStateOf<String?>(null) }
    LaunchedEffect(selected, state.epoch) { if (selected.isNotEmpty()) client.library.select(selected) }
    val current = details.valueOrNull()?.takeIf { it.file.id == selected }
    val historical = current?.versions?.find { it.id == selectedVersion }
    val file = current?.file?.let { if (selectedVersion != null && historical == null) null else if (historical == null) it else it.atVersion(historical) }
    if (selected.isEmpty()) FileBrowserScreen(client, state, folder, trash, select) else Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(details, { client.library.select(selected) })
        if (current != null && selectedVersion != null && historical == null) {
            Notice(str(R.string.text_the_requested_version_is_unavailable_on_this_device_choose_anothe_4d07481), true) {
                TextButton(onClick = { selectedVersion = null },
                    colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onErrorContainer)) {
                    Text(str(R.string.text_view_the_current_version_86f22e5))
                }
            }
        }
        if (file != null) {
            DetailCard {
                Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
                    RowIcon(painterResource(R.drawable.glyph_file), MaterialTheme.colorScheme.secondaryContainer)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(file.name, style = MaterialTheme.typography.titleLarge)
                        Text(fileSize(file.size) + " · " + fileState(file),
                            style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    if (file.starred) Icon(Icons.Outlined.Star, str(R.string.text_favorite_1fd85c7), tint = MaterialTheme.colorScheme.primary)
                }
            }
            if (file.trashedAt != null) {
                Notice(str(R.string.text_restore_this_file_before_opening_it_5bf614e))
                Button(onClick = { client.library.restore(file.id) { select("") } },
                    enabled = !state.working("files:restore:" + file.id), modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.RestoreFromTrash, null)
                    Spacer(Modifier.width(8.dp))
                    Text(str(R.string.text_restore_b04c493))
                }
            } else {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    if (file.local) {
                        Button(onClick = {
                            val label = if (file.version != current.file.version) historical?.createdAt?.let(::messageTime) else null
                            client.fullscreenPreview.openLibrary(
                                files = listOf(file),
                                initialId = file.id,
                                version = file.version,
                                versionLabel = label,
                                showFilter = false,
                            )
                        }, modifier = Modifier.fillMaxWidth()) {
                            Icon(Icons.Outlined.Visibility, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_preview_verified_contents_49f2c13))
                        }
                    } else {
                        Notice(str(R.string.text_this_version_has_no_local_copy_restore_its_verified_cloud_backup_4068c35))
                        file.backup?.takeIf { state.session?.accountKind != "local" }?.let { backup ->
                            Button(onClick = { client.fileWork.restore(backup.upload, file.folder) },
                                enabled = !state.working("file:restore:" + file.id), modifier = Modifier.fillMaxWidth()) {
                                Icon(painterResource(R.drawable.glyph_cloud_down), null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_restore_to_this_device_802102f))
                            }
                        }
                    }
                }
                val isCloud = state.session?.accountKind != "local"
                Section {
                    if (file.local) {
                        ListRow(str(R.string.nearby_share), icon = Icons.Outlined.WifiTethering) {
                            client.nearby.prepareSendFiles(listOf(NearbySelectedFile(file.id, file.version, file.name, file.size)))
                            client.navigation.push(Route("nearby-send-files"))
                        }
                    }
                    if (isCloud && (file.local || file.backup != null)) {
                        ListRow(str(R.string.text_share_by_link_05aafef), glyph = R.drawable.glyph_link,
                            enabled = file.size <= 100uL * 1024u * 1024u) {
                            client.navigation.push(Route("file-link-compose", id = file.id, revision = file.version))
                        }
                    }
                    if (file.local) {
                        ListRow(str(R.string.text_share_a_verified_copy_fc77423), icon = Icons.Outlined.Share,
                            enabled = !state.working("file:open:" + file.id)) {
                            confirmation = "share"
                        }
                    }
                    if (isCloud && file.local && file.backup == null && file.version == current.file.version) {
                        ListRow(str(R.string.text_back_up_this_version_f74abb5), glyph = R.drawable.glyph_cloud_up,
                            enabled = !state.working("file:backup:" + file.id)) {
                            client.navigation.push(Route("file-backup", id = file.id, revision = file.version))
                        }
                    }
                    if (isCloud && file.local && file.backup != null) {
                        ListRow(str(R.string.text_free_local_space_89b2267), glyph = R.drawable.glyph_cloud_off) { confirmation = "evict" }
                    }
                    ListRow(str(R.string.text_edit_notes_tags_and_favorite_25ab2c0), icon = Icons.Outlined.EditNote) {
                        client.library.beginMetadata(current.file); metadata = true
                    }
                    ListRow(str(R.string.text_rename_dd3a2f7), icon = Icons.Outlined.Edit) { rename = true }
                    ListRow(str(R.string.text_move_207d5fe), icon = Icons.AutoMirrored.Outlined.DriveFileMove) { moveError = null; move = true }
                }
            }
            Section(title = str(R.string.text_file_details_27e9099)) {
                InfoRow(str(R.string.text_type_6172196), file.mime, code = true)
                if (file.path.isNotEmpty()) InfoRow(str(R.string.text_folder_b0d1441), file.path.joinToString(" / "))
                if (file.tags.isNotEmpty()) InfoRow(str(R.string.text_tags_33e9c4d), file.tags.joinToString(" · "))
                if (file.note.isNotEmpty()) InfoRow(str(R.string.text_note_a907cbf), file.note, selectable = true)
                file.backup?.let { backup ->
                    ListRow(str(R.string.text_cloud_backups_ab4daf1),
                        supporting = str(R.string.text_backup_saved_0769b59) + messageTime(backup.backedUpAt) + "\n" +
                            if (backup.verified) str(R.string.text_all_chunks_read_back_and_verified_153e284)
                            else str(R.string.text_backup_has_not_been_fully_read_back_30e3810))
                }
                CodeRow("SHA-256", file.sha256) { client.clipboard.copy(file.sha256) }
            }
            Section(title = str(R.string.text_versions_0ff2bdb)) {
                current.versions.reversed().forEach { version ->
                    RadioChoiceRow(messageTime(version.createdAt) + if (version.current) str(R.string.text_current_de083b6) else "", file.version == version.id,
                        subtitle = fileSize(version.size) + " · " + version.device) { selectedVersion = version.id }
                }
            }
            if (file.version != current.file.version && file.trashedAt == null) {
                DetailCard {
                    Text(if (file.sha256 == current.file.sha256 && file.size == current.file.size) str(R.string.text_both_recorded_content_digests_match_32aeeb6)
                        else str(R.string.text_the_recorded_content_differs_between_these_versions_b1657eb),
                        style = MaterialTheme.typography.bodyMedium)
                    CodeRow(str(R.string.text_selected_sha_256_c773aab).trimEnd(' ', '·', ':', '：'), file.sha256)
                    CodeRow(str(R.string.text_current_sha_256_2b15436).trimEnd(' ', '·', ':', '：'), current.file.sha256)
                    OutlinedButton(onClick = { versionAction = "use" }, enabled = !state.working("files:version:${file.id}"),
                        modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_make_this_the_current_version_ddcd50e)) }
                    TextButton(onClick = { versionAction = "delete" }, enabled = !state.working("files:version:${file.id}"),
                        modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_delete_this_local_historical_version_f626908), color = MaterialTheme.colorScheme.error)
                    }
                }
            }
            if (file.trashedAt == null) {
                Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                    ListRow(str(R.string.text_move_local_file_to_trash_9600310), icon = Icons.Outlined.DeleteOutline,
                        destructive = true) { confirmation = "trash" }
                }
            }
        }
    }
    if (metadata && current != null) FileMetadataEditor(client, current.file) { metadata = false }
    if (versionAction != null && current != null && file != null) AlertDialog(onDismissRequest = { versionAction = null }, title = { Text(if (versionAction == "use") str(R.string.text_use_this_version_628b39c) else str(R.string.text_delete_this_historical_version_0144ce0)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(file.name); Text(fileSize(file.size)); Text(file.version, style = MaterialTheme.typography.bodySmall)
            Text(if (versionAction == "use") str(R.string.text_the_selected_immutable_version_becomes_current_other_versions_rem_558f51b) else str(R.string.text_this_permanently_removes_this_version_from_the_local_library_its_4a332f6))
        }
    }, confirmButton = { TextButton(onClick = { if (versionAction == "use") client.library.useVersion(current.file, file.version) else client.library.deleteVersion(current.file, file.version); versionAction = null }) { Text(str(R.string.text_confirm_563fd5a)) } }, dismissButton = { TextButton(onClick = { versionAction = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (rename && file != null) FileNameDialog(client, file.folder, FileActionTarget(file.id, file.name, false), { rename = false }) { rename = false }
    if (move && file != null) FileFolderDialog(client, str(R.string.text_move_to_folder_1c1670d), { move = false },
        action = str(R.string.text_move_here_2a2705e), unchanged = file.folder, busy = state.working("files:move:" + file.id),
        review = { folderId -> Text(file.name); moveError?.takeIf { it.first == folderId }?.let { Notice(fileActionError(it.second), true) } }) { path ->
        moveError = null
        client.library.move(file.id, path.folderId, { moveError = path.folderId to it }) { move = false; client.notice("FILE_MOVED") }
    }
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
    }) { Text(str(when (confirmation) { "share" -> R.string.text_share_a_verified_copy_fc77423; "evict" -> R.string.text_free_local_space_89b2267; "backup" -> R.string.text_back_up_this_version_f74abb5; else -> R.string.text_move_to_trash_c4056f9 })) } }, dismissButton = { TextButton(onClick = { confirmation = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

internal fun fileSize(value: ULong): String {
    val units = listOf("B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB")
    var amount = value.toString().toBigDecimal()
    var unit = 0
    val step = 1024.toBigDecimal()
    while (amount >= step) { amount = amount.divide(step); unit++ }
    val format = java.text.NumberFormat.getNumberInstance(net.dmsg.android.platform.AppLanguage.locale())
    format.maximumFractionDigits = 1
    return numericExpression(format.format(amount) + " " + units[unit])
}
@Composable internal fun fileState(value: LibraryFile): String = when {
    value.trashedAt != null -> str(R.string.text_in_trash_3b5e1bf)
    !value.local && value.backup != null -> str(R.string.text_cloud_only_535ba57)
    value.local && value.backup != null -> str(R.string.text_local_and_backed_up_fdfb784)
    value.local -> str(R.string.text_local_copy_0f3d263)
    else -> str(R.string.text_copy_unavailable_deb9163)
}
