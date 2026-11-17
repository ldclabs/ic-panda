package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun FileBrowserScreen(client: ClientController, state: UiState, trash: Boolean, select: (String) -> Unit) {
    val navigation by client.navigation.state.collectAsStateWithLifecycle()
    val listing by client.library.listing.state.collectAsStateWithLifecycle()
    val tree by client.library.tree.state.collectAsStateWithLifecycle()
    val selection by client.library.selection.collectAsStateWithLifecycle()
    val results by client.library.results.collectAsStateWithLifecycle()
    val usage by client.library.usage.state.collectAsStateWithLifecycle()
    val destinations by client.library.destinations.state.collectAsStateWithLifecycle()
    val folder = navigation.current.folder
    val owner = remember(state.epoch) { client.currentStamp() }
    var preferences by remember(folder, trash) { mutableStateOf(client.library.preferences(folder, trash)) }
    val latestPreferences by rememberUpdatedState(preferences)
    val scroll = remember(folder, trash) { LazyListState(preferences.first, preferences.offset) }
    var menu by remember { mutableStateOf(false) }
    var target by remember { mutableStateOf<FileActionTarget?>(null) }
    var renaming by remember { mutableStateOf(false) }
    var creating by remember { mutableStateOf(false) }
    var name by remember { mutableStateOf("") }
    var action by remember { mutableStateOf<String?>(null) }
    var reviewed by remember { mutableStateOf<List<FileActionTarget>>(emptyList()) }
    var destination by remember { mutableStateOf("root") }
    var parents by remember { mutableStateOf(listOf("root")) }
    var showingResults by remember { mutableStateOf(false) }
    val current = listing.valueOrNull()?.takeIf { it.folder == folder && it.query == preferences.query && it.trash == trash }
    val sortedFiles = current?.files.orEmpty().filter { !preferences.favorites || it.starred }.let { values -> when (preferences.sort) {
        "updated" -> values.sortedByDescending { it.updatedAt }
        "size" -> values.sortedByDescending { it.size }
        else -> values.sortedBy { it.name.lowercase() }
    } }
    val folders = if (preferences.favorites) emptyList() else current?.folders.orEmpty().sortedBy { it.name.lowercase() }
    val all = folders.map { FileActionTarget(it.id, it.name, true) } + sortedFiles.map { FileActionTarget(it.id, it.name, false) }
    val filesById = sortedFiles.associateBy { it.id }
    val folderMap = tree.valueOrNull().orEmpty().associateBy { it.id }
    val crumbs = buildList<LibraryFolder> {
        var id = folder
        while (id != "root" && size < 64 && none { it.id == id }) {
            val next = folderMap[id] ?: break
            add(next); id = next.parent
        }
    }.reversed()
    fun start(actionName: String, targets: List<FileActionTarget>) {
        reviewed = targets; action = actionName
        if (actionName == "move") { destination = "root"; parents = listOf("root"); client.library.destination(destination) }
    }
    LaunchedEffect(folder, trash, preferences.query, state.epoch) { client.library.browse(folder, preferences.query, trash); if (trash) client.library.loadUsage() }
    DisposableEffect(folder, trash, owner) { onDispose { client.library.remember(folder, trash, latestPreferences.copy(first = scroll.firstVisibleItemIndex, offset = scroll.firstVisibleItemScrollOffset), owner) } }
    LazyColumn(Modifier.fillMaxSize(), state = scroll, contentPadding = PaddingValues(20.dp, 12.dp, 20.dp, 110.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        item {
            if (trash) Text(str(R.string.text_deleted_on_this_device_c40f3ba), style = MaterialTheme.typography.titleMedium)
            if (!trash && folder != "root") Row(Modifier.horizontalScroll(rememberScrollState()), verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = { client.navigation.folder("root") }, modifier = Modifier.semantics { contentDescription = str(R.string.text_all_files_5d4019a) }) { Text(str(R.string.text_files_c79ddc2)) }
                crumbs.forEach { crumb -> Text("/"); TextButton(onClick = { client.navigation.folder(crumb.id) }, enabled = crumb.id != folder) { Text(crumb.name) } }
            }
            WorkspaceSearchField(preferences.query, { preferences = preferences.copy(query = it, limit = 100, first = 0, offset = 0) }, str(R.string.text_search_files_notes_and_tags_9013c74))
            FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                if (!trash) {
                    FilterChip(true, { if (folder != "root") client.navigation.folder("root") }, { Text(str(R.string.text_on_this_device_6032118)) },
                        leadingIcon = { Icon(Icons.Outlined.PhoneAndroid, null, Modifier.size(18.dp)) })
                    FilterChip(false, { client.navigation.push(Route("cloud-files")) }, { Text(str(R.string.text_cloud_backups_ab4daf1)) },
                        leadingIcon = { Icon(Icons.Outlined.Cloud, null, Modifier.size(18.dp)) })
                }
                Box {
                    IconButton(onClick = { menu = true }) { Icon(Icons.Outlined.MoreVert, str(R.string.text_more_file_actions_65e84ee)) }
                    DropdownMenu(menu, { menu = false }) {
                        if (!trash) DropdownMenuItem(text = { Text(str(R.string.text_new_folder_3b1862d)) }, leadingIcon = { Icon(Icons.Outlined.CreateNewFolder, null) },
                            onClick = { menu = false; name = ""; creating = true })
                        DropdownMenuItem(text = { Text(str(R.string.text_select_visible_items_5b34cd2)) }, onClick = { menu = false; all.take(preferences.limit).filterNot { it.id in selection }.forEach { client.library.choose(it.id) } })
                        DropdownMenuItem(text = { Text(str(R.string.text_share_links_fe69aa5)) }, onClick = { menu = false; client.navigation.push(Route("file-links")) })
                        DropdownMenuItem(text = { Text(str(R.string.text_collect_files_3ad17e5)) }, onClick = { menu = false; client.navigation.push(Route("file-requests")) })
                        DropdownMenuItem(text = { Text(str(R.string.text_transfers_6fdfd95)) }, onClick = { menu = false; client.navigation.push(Route("file-transfers")) })
                        if (!trash) DropdownMenuItem(text = { Text(str(R.string.text_trash_8af1079)) }, onClick = { menu = false; client.navigation.push(Route("file-trash")) })
                    }
                }
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("name", "updated", "size").forEach { sort -> FilterChip(preferences.sort == sort, { preferences = preferences.copy(sort = sort) }, { Text(when (sort) { "updated" -> str(R.string.text_recent_723c894); "size" -> str(R.string.text_largest_4d2cb3c); else -> str(R.string.text_name_dd4dc4c) }) }) }
                FilterChip(preferences.favorites, { preferences = preferences.copy(favorites = !preferences.favorites) }, { Text(str(R.string.text_favorites_960eb5d)) })
            }
            if (selection.isNotEmpty()) {
                Text(str(R.string.text_selected_26244a5) + localizedNumber(selection.size), style = MaterialTheme.typography.labelLarge)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (!trash) TextButton(onClick = { start("move", all.filter { it.id in selection }) }, enabled = !state.working("files:batch")) { Text(str(R.string.text_move_207d5fe)) }
                    TextButton(onClick = { start(if (trash) "restore" else "trash", all.filter { it.id in selection }) }, enabled = !state.working("files:batch")) { Text(if (trash) str(R.string.text_restore_b04c493) else str(R.string.text_move_to_trash_c4056f9)) }
                    if (trash) TextButton(onClick = { start("purge", all.filter { it.id in selection }) }, enabled = !state.working("files:batch")) { Text(str(R.string.text_delete_permanently_994ca5c), color = MaterialTheme.colorScheme.error) }
                    TextButton(onClick = client.library::clearSelection) { Text(str(R.string.text_done_selecting_7faa60b)) }
                }
            }
            ReadFeedback(listing, { client.library.browse(folder, preferences.query, trash) })
            if (trash) {
                ReadFeedback(usage, client.library::loadUsage)
                usage.valueOrNull()?.let { Text(str(R.string.text_local_encrypted_storage_b4f4abb) + fileSize(it.bytes), style = MaterialTheme.typography.bodySmall) }
                Notice(str(R.string.text_deleting_permanently_removes_all_local_versions_and_folder_conten_1d4b1ab))
                if (preferences.query.isBlank() && !preferences.favorites && all.isNotEmpty()) TextButton(onClick = { start("purge", all) }, enabled = !state.working("files:batch")) { Text(str(R.string.text_empty_local_trash_3566898), color = MaterialTheme.colorScheme.error) }
            }
        }
        items(all.take(preferences.limit), key = { it.id }) { entry ->
            val file = filesById[entry.id]
            ListItem(modifier = Modifier.semantics { if (selection.isNotEmpty()) toggleableState = ToggleableState(entry.id in selection) }
                .combinedClickable(role = if (selection.isNotEmpty()) Role.Checkbox else Role.Button,
                    onClick = { if (selection.isNotEmpty()) client.library.choose(entry.id) else if (trash) target = entry else if (entry.folder) client.navigation.folder(entry.id) else select(entry.id) }, onLongClick = { client.library.choose(entry.id) }),
                headlineContent = { Text(entry.name, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                supportingContent = { file?.let { Text(fileSize(it.size) + " · " + fileState(it)) } },
                leadingContent = { if (selection.isNotEmpty()) Checkbox(entry.id in selection, onCheckedChange = null) else if (file != null) FileThumbnail(client, file) else Icon(Icons.Outlined.Folder, null) },
                trailingContent = { Row(verticalAlignment = Alignment.CenterVertically) { if (file?.starred == true) Icon(Icons.Outlined.Star, str(R.string.text_favorite_898f4c6)); IconButton(onClick = { target = entry }) { Icon(Icons.Outlined.MoreVert, str(R.string.text_actions_for_342986f) + entry.name) } } })
            HorizontalDivider()
        }
        if (all.size > preferences.limit) item { TextButton(onClick = { preferences = preferences.copy(limit = preferences.limit + 100) }) { Text(str(R.string.text_show_more_items_688023e)) } }
        if (listing is ReadState.Empty || current != null && all.isEmpty()) item { EmptyPanel(if (trash) str(R.string.text_trash_is_empty_c562cd3) else str(R.string.text_no_matching_files_2fc3030)) }
        item {
            if (!trash) Notice(str(R.string.text_files_are_encrypted_locally_cloud_backup_is_a_separate_explicit_a_28bc23b))
            if (state.working("files:batch")) { LinearProgressIndicator(Modifier.fillMaxWidth()); TextButton(onClick = client.library::cancelBatch) { Text(str(R.string.text_stop_after_completed_items_d0aee2e)) } }
            if (results.isNotEmpty()) TextButton(onClick = { showingResults = true }) { Text(str(R.string.text_review_operation_results_f79dc63)) }
        }
    }
    target?.let { item -> AlertDialog(onDismissRequest = { target = null }, title = { Text(item.name) }, text = {
        Column {
            if (!trash) {
                TextButton(onClick = { name = item.name; renaming = true }) { Text(str(R.string.text_rename_dd3a2f7)) }
                TextButton(onClick = { start("move", listOf(item)); target = null }) { Text(str(R.string.text_move_207d5fe)) }
            }
            TextButton(onClick = { start(if (trash) "restore" else "trash", listOf(item)); target = null }) { Text(if (trash) str(R.string.text_restore_b04c493) else str(R.string.text_move_to_trash_c4056f9)) }
            if (trash) TextButton(onClick = { start("purge", listOf(item)); target = null }) { Text(str(R.string.text_delete_permanently_994ca5c), color = MaterialTheme.colorScheme.error) }
        }
    }, confirmButton = { TextButton(onClick = { target = null }) { Text(str(R.string.text_close_6cf4a77)) } }) }
    if (creating || renaming) AlertDialog(onDismissRequest = { creating = false; renaming = false }, title = { Text(if (creating) str(R.string.text_new_folder_3b1862d) else str(R.string.text_rename_dd3a2f7)) }, text = { OutlinedTextField(name, { name = it }, label = { Text(str(R.string.text_name_dd4dc4c)) }) },
        confirmButton = { TextButton(onClick = {
            if (creating) client.perform({ it.createFolder(folder, name) }, key = "files:create-folder") { client.library.changed(); creating = false }
            else target?.let { item -> client.library.rename(item.id, name) { renaming = false; target = null } }
        }, enabled = name.isNotBlank() && !name.contains('/') && !name.contains('\\') && !state.working("files:create-folder")) { Text(str(R.string.text_save_ec8e6d5)) } }, dismissButton = { TextButton(onClick = { creating = false; renaming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    action?.let { chosen -> AlertDialog(onDismissRequest = { action = null }, title = { Text(when (chosen) { "move" -> str(R.string.text_move_selected_items_6d7db23); "restore" -> str(R.string.text_restore_selected_items_26adad4); "purge" -> str(R.string.text_delete_permanently_37eaf8d); else -> str(R.string.text_move_to_local_trash_d27df84) }) }, text = {
        Column(Modifier.heightIn(max = 400.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            reviewed.forEach { Text(it.name + if (it.folder) str(R.string.text_includes_folder_contents_8e9a5e9) else "") }
            if (chosen == "purge") Notice(str(R.string.text_all_local_versions_will_be_removed_unbacked_versions_cannot_be_re_0e233a3), true)
            if (chosen == "move") {
                ReadFeedback(destinations, { client.library.destination(destination) })
                if (parents.size > 1) TextButton(onClick = { parents = parents.dropLast(1); destination = parents.last(); client.library.destination(destination) }) { Text(str(R.string.text_parent_folder_3af9479)) }
                destinations.valueOrNull()?.takeIf { it.folder == destination }?.folders.orEmpty().filter { folder -> reviewed.none { it.id == folder.id } }.forEach { folder -> TextButton(onClick = { destination = folder.id; parents += destination; client.library.destination(destination) }) { Icon(Icons.Outlined.Folder, null); Text(folder.name) } }
            }
        }
    }, confirmButton = { TextButton(onClick = { client.library.batch(chosen, reviewed, destination.takeIf { chosen == "move" }) { showingResults = true }; action = null }, enabled = !state.working("files:batch")) { Text(str(R.string.text_confirm_563fd5a)) } }, dismissButton = { TextButton(onClick = { action = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
    if (showingResults) AlertDialog(onDismissRequest = { showingResults = false }, title = { Text(str(R.string.text_operation_results_71f0b9e)) }, text = {
        Column(Modifier.heightIn(max = 400.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) { results.forEach { result -> Text(result.name, style = MaterialTheme.typography.titleSmall); Text(result.error?.let(::errorText) ?: str(R.string.text_completed_6ff49bc), color = if (result.error == null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error) } }
    }, confirmButton = { TextButton(onClick = { showingResults = false }) { Text(str(R.string.text_done_9ffd599)) } })
}
