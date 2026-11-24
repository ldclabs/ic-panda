package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import androidx.compose.ui.res.painterResource
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
import androidx.compose.material.icons.automirrored.outlined.DriveFileMove
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun FileBrowserScreen(client: ClientController, state: UiState, folder: String, trash: Boolean, select: (String) -> Unit) {
    val listing by client.library.listing.state.collectAsStateWithLifecycle()
    val globalRead by client.search.global.state.collectAsStateWithLifecycle()
    val tree by client.library.tree.state.collectAsStateWithLifecycle()
    val tagsRead by client.library.tags.state.collectAsStateWithLifecycle()
    val selection by client.library.selection.collectAsStateWithLifecycle()
    val results by client.library.results.collectAsStateWithLifecycle()
    val usage by client.library.usage.state.collectAsStateWithLifecycle()
    val local by client.accountWorkspace.local.state.collectAsStateWithLifecycle()
    var searchAll by remember { mutableStateOf(false) }
    val root = folder == "root" && !trash
    val listingReady = listing is ReadState.Ready || listing is ReadState.Empty
    LaunchedEffect(root, listingReady, state.epoch) {
        if (root && listingReady && state.session?.accountKind != "local") client.accountWorkspace.local()
    }
    val owner = remember(state.epoch) { client.currentStamp() }
    val savedPreferences by client.library.preferences.collectAsStateWithLifecycle()
    val preferences = savedPreferences["$trash:$folder"] ?: FileBrowsePreferences()
    val canBrowseUnbacked = state.session?.accountKind != "local" && !trash
    val scroll = remember(folder, trash) { LazyListState(preferences.first, preferences.offset) }
    fun update(value: FileBrowsePreferences) {
        if (value.query != preferences.query || value.favorites != preferences.favorites || value.sort != preferences.sort) scroll.requestScrollToItem(0)
        client.library.remember(folder, trash, value, owner)
    }
    var menu by remember { mutableStateOf(false) }
    var target by remember { mutableStateOf<FileActionTarget?>(null) }
    var renaming by remember { mutableStateOf<FileActionTarget?>(null) }
    var creating by remember { mutableStateOf(false) }
    var action by remember { mutableStateOf<String?>(null) }
    var reviewed by remember { mutableStateOf<List<FileActionTarget>>(emptyList()) }
    var showingResults by remember { mutableStateOf(false) }
    val current = listing.valueOrNull()?.takeIf { it.folder == folder && it.query == preferences.query && it.trash == trash && it.favorites == preferences.favorites && it.tag == preferences.tag }
    val searchingQuery = preferences.query.isNotBlank()
    val sortedFiles = current?.files.orEmpty().let { values -> when {
        searchingQuery && preferences.sort == "name" -> values
        preferences.sort == "updated" -> values.sortedByDescending { it.updatedAt }
        preferences.sort == "size" -> values.sortedByDescending { it.size }
        else -> values.sortedBy { it.name.lowercase() }
    } }
    val folders = if (preferences.favorites || preferences.tag != null) emptyList() else current?.folders.orEmpty().let { list ->
        if (searchingQuery && preferences.sort == "name") list else list.sortedBy { it.name.lowercase() }
    }
    val all = folders.map { FileActionTarget(it.id, it.name, true) } + sortedFiles.map { FileActionTarget(it.id, it.name, false) }
    val visible = all.take(preferences.limit)
    fun selectVisible() {
        visible.filterNot { it.id in selection }.forEach { client.library.choose(it.id) }
    }
    val filesById = sortedFiles.associateBy { it.id }
    val matchedFoldersById = folders.associateBy { it.id }
    val folderMap = tree.valueOrNull().orEmpty().associateBy { it.id }
    val availableTags = tagsRead.valueOrNull().orEmpty()
    val crumbs = buildList<LibraryFolder> {
        var id = folder
        while (id != "root" && size < 64 && none { it.id == id }) {
            val next = folderMap[id] ?: break
            add(next); id = next.parent
        }
    }.reversed()
    fun start(actionName: String, targets: List<FileActionTarget>) {
        reviewed = targets; action = actionName
    }
    fun finish(actionName: String) {
        if (client.library.results.value.any { it.error != null }) showingResults = true
        else client.notice(when (actionName) {
            "move" -> "FILE_MOVED"
            "trash" -> "FILE_TRASHED"
            "restore" -> "FILE_RESTORED"
            else -> "FILE_DELETED_LOCALLY"
        })
    }
    LaunchedEffect(folder, trash, preferences.query, preferences.favorites, preferences.tag, state.epoch) {
        client.library.browse(folder, preferences.query, trash, preferences.favorites, preferences.tag)
        if (trash) client.library.loadUsage()
    }
    LaunchedEffect(searchAll, preferences.query) {
        if (searchAll && preferences.query.isNotBlank()) client.search.searchAll(preferences.query)
    }
    DisposableEffect(folder, trash, owner) { onDispose {
        val latest = client.library.preferences.value["$trash:$folder"] ?: FileBrowsePreferences()
        client.library.remember(folder, trash, latest.copy(first = scroll.firstVisibleItemIndex, offset = scroll.firstVisibleItemScrollOffset), owner)
    } }
    key(folder, trash) {
        WorkspaceBrowser(str(if (trash) R.string.text_trash_8af1079 else R.string.text_files_c79ddc2), preferences.query,
            { client.library.clearSelection(); update(preferences.copy(query = it, limit = 100, first = 0, offset = 0)) },
            str(R.string.text_search_files_notes_and_tags_9013c74), actions = {
                Box {
                    IconButton(onClick = { menu = true }) { Icon(Icons.Outlined.MoreVert, str(R.string.text_more_file_actions_65e84ee)) }
                    DropdownMenu(menu, { menu = false }) {
                        if (!trash) DropdownMenuItem(
                            text = { Text(str(R.string.text_new_folder_3b1862d)) },
                            leadingIcon = { Icon(Icons.Outlined.CreateNewFolder, null) },
                            onClick = { menu = false; creating = true },
                        )
                        DropdownMenuItem(
                            text = { Text(str(R.string.text_select_visible_items_5b34cd2)) },
                            leadingIcon = { Icon(Icons.Outlined.Checklist, null) },
                            onClick = { menu = false; selectVisible() },
                            enabled = all.isNotEmpty(),
                        )
                        HorizontalDivider(Modifier.padding(vertical = 4.dp))
                        if (canBrowseUnbacked) DropdownMenuItem(
                            text = { Text(str(R.string.text_unbacked_file_versions_f59bc2a)) },
                            leadingIcon = { Icon(painterResource(R.drawable.glyph_cloud_off), null) },
                            trailingIcon = local.valueOrNull()?.unbacked?.takeIf { it > 0u }?.let { count ->
                                { Badge { Text(if (count > 99u) "99+" else localizedNumber(count)) } }
                            },
                            onClick = { menu = false; client.navigation.push(Route("file-unbacked")) },
                        )
                        if (state.session?.accountKind != "local") DropdownMenuItem(
                            text = { Text(str(R.string.text_share_links_fe69aa5)) },
                            leadingIcon = { Icon(painterResource(R.drawable.glyph_link), null) },
                            onClick = { menu = false; client.navigation.push(Route("file-links")) },
                        )
                        if (state.session?.accountKind != "local") DropdownMenuItem(
                            text = { Text(str(R.string.text_collect_files_3ad17e5)) },
                            leadingIcon = { Icon(Icons.Outlined.MoveToInbox, null) },
                            onClick = { menu = false; client.navigation.push(Route("file-requests")) },
                        )
                        DropdownMenuItem(
                            text = { Text(str(R.string.text_transfers_6fdfd95)) },
                            leadingIcon = { Icon(Icons.Outlined.SwapVert, null) },
                            onClick = { menu = false; client.navigation.push(Route("file-transfers")) },
                        )
                        if (!trash) {
                            HorizontalDivider(Modifier.padding(vertical = 4.dp))
                            DropdownMenuItem(
                                text = { Text(str(R.string.text_trash_8af1079)) },
                                leadingIcon = { Icon(Icons.Outlined.DeleteOutline, null) },
                                onClick = { menu = false; client.navigation.push(Route("file-trash")) },
                            )
                        }
                    }
                }
            }) { searching ->
            LaunchedEffect(searching) { if (!searching) searchAll = false }
            if (!trash && !searching) ScreenFab {
                val atTop by remember { derivedStateOf { scroll.firstVisibleItemIndex == 0 } }
                FileIntakeButton(client, folder = folder, fab = true, expanded = atTop)
            }
            LazyColumn(Modifier.fillMaxSize(), state = scroll, contentPadding = pagePadding(top = 0.dp, bottom = FabClearance)) {
                if (trash || folder != "root") item {
                    if (trash) Text(str(R.string.text_deleted_on_this_device_c40f3ba), style = MaterialTheme.typography.titleMedium)
                    if (!trash && folder != "root") Row(Modifier.horizontalScroll(rememberScrollState()), verticalAlignment = Alignment.CenterVertically) {
                        TextButton(onClick = { client.navigation.folder("root") }, modifier = Modifier.semantics { contentDescription = str(R.string.text_all_files_5d4019a) }) { Text(str(R.string.text_files_c79ddc2)) }
                        crumbs.forEach { crumb -> Text("/"); TextButton(onClick = { client.navigation.folder(crumb.id) }, enabled = crumb.id != folder) { Text(crumb.name) } }
                    }
                }
                browseScopesItem(searching) {
                    BrowseScopes(trailing = {
                        if (!searching || !searchAll) SortMenu(preferences.sort, buildList {
                            add("name" to if (searchingQuery) R.string.search_sort_relevance else R.string.text_name_dd4dc4c)
                            add("updated" to R.string.text_recent_723c894)
                            add("size" to R.string.text_largest_4d2cb3c)
                        }) {
                            update(preferences.copy(sort = it))
                        }
                    }) {
                        if (searching) {
                            BrowseChoice(str(R.string.search_scope_current), !searchAll) { searchAll = false }
                            BrowseChoice(str(R.string.search_scope_all), searchAll) { searchAll = true }
                        }
                        if (!searching || !searchAll) {
                            if (!trash && state.session?.accountKind != "local") {
                                BrowseChoice(str(R.string.text_on_this_device_6032118), true) {}
                                BrowseChoice(str(R.string.text_cloud_backups_ab4daf1), false) { client.navigation.push(Route("cloud-files")) }
                            }
                            BrowseChoice(str(R.string.text_favorites_960eb5d), preferences.favorites, toggle = true) {
                                client.library.clearSelection()
                                update(preferences.copy(favorites = !preferences.favorites))
                            }
                            if (!trash && availableTags.isNotEmpty()) {
                                MenuChoice(preferences.tag?.let { "#$it" } ?: str(R.string.text_tags_33e9c4d), preferences.tag != null) { close ->
                                    DropdownMenuItem(
                                        text = { Text(str(R.string.text_all_tags_b8ceb46)) },
                                        trailingIcon = if (preferences.tag == null) ({ Icon(Icons.Outlined.Check, null) }) else null,
                                        onClick = { close(); client.library.clearSelection(); update(preferences.copy(tag = null)) },
                                    )
                                    HorizontalDivider(Modifier.padding(vertical = 4.dp))
                                    availableTags.forEach { item ->
                                        DropdownMenuItem(
                                            text = { Text("#" + item.tag) },
                                            trailingIcon = {
                                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                                    Text(localizedNumber(item.files), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                                    if (preferences.tag == item.tag) Icon(Icons.Outlined.Check, null)
                                                }
                                            },
                                            onClick = { close(); client.library.clearSelection(); update(preferences.copy(tag = item.tag)) },
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
                if (searching && searchAll) {
                    globalSearchResults(client, preferences.query, globalRead) { client.search.searchAll(preferences.query) }
                } else {
                item {
                    if (searching && selection.isEmpty() && visible.isNotEmpty()) {
                        TextButton(onClick = ::selectVisible) { Text(str(R.string.text_select_visible_items_5b34cd2)) }
                    }
                    if (selection.isNotEmpty()) {
                        DetailCard(Modifier.padding(vertical = 6.dp)) {
                            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                                Text(str(R.string.text_selected_26244a5) + localizedNumber(selection.size), Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
                                TextButton(onClick = client.library::clearSelection) { Text(str(R.string.text_done_selecting_7faa60b)) }
                            }
                            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                OutlinedButton(onClick = ::selectVisible, enabled = visible.any { it.id !in selection }) {
                                    Text(str(R.string.text_select_visible_items_5b34cd2))
                                }
                                if (!trash) OutlinedButton(onClick = { start("move", all.filter { it.id in selection }) }, enabled = !state.working("files:batch")) {
                                    Text(str(R.string.text_move_207d5fe))
                                }
                                OutlinedButton(onClick = { start(if (trash) "restore" else "trash", all.filter { it.id in selection }) }, enabled = !state.working("files:batch")) {
                                    Text(if (trash) str(R.string.text_restore_b04c493) else str(R.string.text_move_to_trash_c4056f9))
                                }
                                if (trash) OutlinedButton(onClick = { start("purge", all.filter { it.id in selection }) }, enabled = !state.working("files:batch")) {
                                    Text(str(R.string.text_delete_permanently_994ca5c), color = MaterialTheme.colorScheme.error)
                                }
                            }
                        }
                    }
                    ReadFeedback(listing, { client.library.browse(folder, preferences.query, trash, preferences.favorites, preferences.tag) })
                    if (trash) {
                        ReadFeedback(usage, client.library::loadUsage)
                        Column(Modifier.padding(vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            usage.valueOrNull()?.let { Text(str(R.string.text_local_encrypted_storage_b4f4abb) + fileSize(it.bytes), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                            Notice(str(R.string.text_deleting_permanently_removes_all_local_versions_and_folder_conten_1d4b1ab),
                                action = if (preferences.query.isBlank() && !preferences.favorites && preferences.tag == null && all.isNotEmpty()) { {
                                    TextButton(onClick = { start("purge", all) }, enabled = !state.working("files:batch")) { Text(str(R.string.text_empty_local_trash_3566898), color = MaterialTheme.colorScheme.error) }
                                } } else null)
                        }
                    }
                }
                items(visible, key = { it.id }) { entry ->
                    val file = filesById[entry.id]
                    val matchedFolder = matchedFoldersById[entry.id]
                    val nameSpans = file?.nameSpans ?: matchedFolder?.nameSpans.orEmpty()
                    val pathSpans = file?.pathSpans ?: matchedFolder?.pathSpans.orEmpty()
                    BrowseRow(buildHighlightedText(entry.name, nameSpans), Modifier.animateItem().semantics { if (selection.isNotEmpty()) toggleableState = ToggleableState(entry.id in selection) }
                        .combinedClickable(role = if (selection.isNotEmpty()) Role.Checkbox else Role.Button,
                            onClick = { if (selection.isNotEmpty()) client.library.choose(entry.id) else if (trash) target = entry else if (entry.folder) client.navigation.folder(entry.id) else select(entry.id) }, onLongClick = { client.library.choose(entry.id) }), supporting = {
                            if (preferences.query.isNotBlank() || file != null) Column {
                                if (preferences.query.isNotBlank()) {
                                    val path = file?.path ?: matchedFolder?.path ?: folderMap[entry.id]?.path.orEmpty()
                                    val pathJoined = path.joinToString(" / ")
                                    if (pathSpans.isNotEmpty()) {
                                        Text(buildHighlightedText(pathJoined, pathSpans), maxLines = 2, overflow = TextOverflow.Ellipsis)
                                    } else {
                                        Text((listOf(str(R.string.text_files_c79ddc2)) + path).joinToString(" / "), maxLines = 2, overflow = TextOverflow.Ellipsis)
                                    }
                                }
                                file?.let {
                                    val tagHint = if (it.matchField == "tag" && !it.matchValue.isNullOrBlank()) "#${it.matchValue} · " else ""
                                    Text(tagHint + fileSize(it.size) + " · " + fileState(it))
                                }
                            }
                        },
                        leading = { if (selection.isNotEmpty()) Checkbox(entry.id in selection, onCheckedChange = null) else if (file != null) FileThumbnail(client, file) else BrowseIcon(Icons.Outlined.Folder) },
                        trailing = {
                            if (file?.starred == true) Icon(Icons.Outlined.Star, str(R.string.text_favorite_898f4c6), Modifier.size(16.dp))
                            IconButton(onClick = { target = entry }) { Icon(Icons.Outlined.MoreVert, str(R.string.text_actions_for_342986f) + entry.name) }
                        })
                }
                if (all.size > preferences.limit) item { TextButton(onClick = { update(preferences.copy(limit = preferences.limit + 100)) }) { Text(str(R.string.text_show_more_items_688023e)) } }
                val empty = current != null && all.isEmpty() && (listing is ReadState.Ready || listing is ReadState.Empty)
                val privacy = str(if (state.session?.accountKind == "local") R.string.saved_on_device else R.string.text_files_are_encrypted_locally_cloud_backup_is_a_separate_explicit_a_28bc23b)
                if (empty) item {
                    val filtered = preferences.query.isNotBlank() || preferences.favorites || preferences.tag != null
                    EmptyState(str(when { filtered -> R.string.text_no_matching_files_2fc3030; trash -> R.string.text_trash_is_empty_c562cd3; else -> R.string.empty_folder }),
                        icon = when { filtered -> Icons.Outlined.Search; trash -> Icons.Outlined.DeleteOutline; else -> null },
                        glyph = if (!filtered && !trash) R.drawable.glyph_folder else null,
                        detail = if (!filtered && !trash) privacy else null,
                        action = if (filtered) ({ TextButton(onClick = { update(preferences.copy(query = "", favorites = false, tag = null, limit = 100)) }) { Text(str(R.string.text_clear_filters_5ea61be)) } }) else null)
                }
                item {
                    // The encryption note is the empty folder's first-run hint; a filled list does not repeat it.
                    if (state.working("files:batch")) { LinearProgressIndicator(Modifier.fillMaxWidth()); TextButton(onClick = client.library::cancelBatch) { Text(str(R.string.text_stop_after_completed_items_d0aee2e)) } }
                    if (results.isNotEmpty()) TextButton(onClick = { showingResults = true }) { Text(str(R.string.text_review_operation_results_f79dc63)) }
                }
                }
            }
        }
    }
    target?.let { item ->
        ModalBottomSheet(onDismissRequest = { target = null }) {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(bottom = 16.dp)) {
                Row(Modifier.fillMaxWidth().padding(horizontal = pageMargin(), vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (item.folder) BrowseIcon(Icons.Outlined.Folder) else BrowseIcon(R.drawable.glyph_file)
                    Text(item.name, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
                HorizontalDivider(Modifier.padding(vertical = 4.dp))
                if (!trash) {
                    SheetAction(str(R.string.text_rename_dd3a2f7), Icons.Outlined.Edit) { renaming = item; target = null }
                    SheetAction(str(R.string.text_move_207d5fe), Icons.AutoMirrored.Outlined.DriveFileMove) { start("move", listOf(item)); target = null }
                }
                SheetAction(
                    label = if (trash) str(R.string.text_restore_b04c493) else str(R.string.text_move_to_trash_c4056f9),
                    icon = if (trash) Icons.Outlined.RestoreFromTrash else Icons.Outlined.DeleteOutline,
                    destructive = !trash,
                ) { start(if (trash) "restore" else "trash", listOf(item)); target = null }
                if (trash) SheetAction(str(R.string.text_delete_permanently_994ca5c), Icons.Outlined.DeleteForever, destructive = true) {
                    start("purge", listOf(item)); target = null
                }
            }
        }
    }
    if (creating || renaming != null) FileNameDialog(client, folder, renaming, { creating = false; renaming = null }) {
        if (creating) { client.library.clearSelection(); update(preferences.copy(query = "", favorites = false)) }
        creating = false; renaming = null
    }
    if (action == "move") {
        val sources = reviewed.map { filesById[it.id]?.folder ?: folderMap[it.id]?.parent }.distinct()
        FileFolderDialog(client, str(R.string.text_move_selected_items_6d7db23), { action = null },
            action = str(R.string.text_move_here_2a2705e), unchanged = sources.singleOrNull(),
            exclude = reviewed.filter { it.folder }.map { it.id }.toSet(), review = { reviewed.forEach { Text(it.name) } }) { path ->
            client.library.batch("move", reviewed, path.folderId) { finish("move") }; action = null
        }
    } else action?.let { chosen ->
        val label = when (chosen) { "purge" -> R.string.text_delete_permanently_994ca5c; "restore" -> R.string.text_restore_b04c493; else -> R.string.text_move_to_trash_c4056f9 }
        AlertDialog(onDismissRequest = { action = null }, title = { Text(str(when (chosen) { "restore" -> R.string.text_restore_selected_items_26adad4; "purge" -> R.string.text_delete_permanently_37eaf8d; else -> R.string.text_move_to_local_trash_d27df84 })) }, text = {
            Column(Modifier.heightIn(max = 400.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                reviewed.forEach { Text(it.name + if (it.folder) str(R.string.text_includes_folder_contents_8e9a5e9) else "") }
                if (chosen == "purge") Notice(str(R.string.text_all_local_versions_will_be_removed_unbacked_versions_cannot_be_re_0e233a3), true)
            }
        }, confirmButton = { TextButton(onClick = { client.library.batch(chosen, reviewed) { finish(chosen) }; action = null }, enabled = !state.working("files:batch")) {
            Text(str(label), color = if (chosen == "purge") MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
        } }, dismissButton = { TextButton(onClick = { action = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
    }
    if (showingResults) AlertDialog(onDismissRequest = { showingResults = false }, title = { Text(str(R.string.text_operation_results_71f0b9e)) }, text = {
        Column(Modifier.heightIn(max = 400.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) { results.forEach { result -> Text(result.name, style = MaterialTheme.typography.titleSmall); Text(result.error?.let(::errorText) ?: str(R.string.text_completed_6ff49bc), color = if (result.error == null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error) } }
    }, confirmButton = { TextButton(onClick = { showingResults = false }) { Text(str(R.string.text_done_9ffd599)) } })
}
