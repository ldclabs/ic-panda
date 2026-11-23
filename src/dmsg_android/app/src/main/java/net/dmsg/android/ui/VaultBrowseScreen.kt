package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.quantity
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str
import androidx.compose.foundation.clickable
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import net.dmsg.core.vaultKinds

fun vaultKindName(kind: String): String = when (kind) {
    "login" -> str(R.string.text_password_347d7fc); "api" -> str(R.string.text_api_credential_184807d); "key" -> str(R.string.text_key_4b6b6ae)
    "ssh" -> str(R.string.text_ssh_key_b104eb4); "wallet" -> str(R.string.text_wallet_secret_58fc026); "card" -> str(R.string.text_payment_card_503ea8d)
    "identity" -> str(R.string.text_identity_1cadaac); "server" -> str(R.string.text_server_9770cd5); "database" -> str(R.string.text_database_7bc9ec0)
    "license" -> str(R.string.text_software_license_12e336e); else -> str(R.string.text_note_9917199)
}

@Composable internal fun SecretIcon(kind: String) {
    BrowseIcon(vaultGlyph(kind))
}

@Composable fun VaultScreen(client: ClientController, select: (String) -> Unit) {
    val localMode = client.state.collectAsStateWithLifecycle().value.session?.accountKind == "local"
    val read by client.vault.listing.state.collectAsStateWithLifecycle()
    val page = read.valueOrNull()
    val query by client.vault.filters.collectAsStateWithLifecycle()
    val scroll = workspaceListState(client.navigation, "vault:list")
    fun changeQuery(next: VaultQuery) {
        if (next != query) scroll.requestScrollToItem(0)
        client.vault.browse(next)
    }
    var filters by remember { mutableStateOf(false) }
    var tools by remember { mutableStateOf(false) }
    val savedAccounts by client.accountWorkspace.accounts.state.collectAsStateWithLifecycle()
    LaunchedEffect(tools, localMode) { if (tools && !localMode) client.accountWorkspace.accounts(resolveNames = false) }
    LaunchedEffect(Unit) { client.vault.browse() }
    BackHandler(enabled = LocalPageActive.current && query.trash, onBack = client.vault::toggleTrash)
    key(query.trash) {
        WorkspaceBrowser(str(if (query.trash) R.string.text_trash_8af1079 else R.string.text_vault_ede2a06),
            query.text, { changeQuery(query.copy(text = it.take(1000))) }, str(R.string.text_search_vault_f6e42b2),
            actions = {
                if (query.trash) TextButton(onClick = client.vault::toggleTrash) { Text(str(R.string.text_vault_ede2a06)) }
                else {
                    Box {
                        IconButton(onClick = { tools = true }) { Icon(Icons.Outlined.MoreVert, str(R.string.text_vault_tools_14fe2cc)) }
                        DropdownMenu(tools, { tools = false }) {
                            DropdownMenuItem(
                                text = { Text(str(R.string.text_local_security_review_b7f4013)) },
                                leadingIcon = { Icon(painterResource(R.drawable.glyph_security), null) },
                                onClick = { tools = false; client.navigation.open("secret-health") },
                            )
                            if (!localMode) DropdownMenuItem(
                                text = { Text(str(R.string.text_version_conflicts_0df1b3c)) },
                                leadingIcon = { Icon(Icons.Outlined.WarningAmber, null) },
                                onClick = { tools = false; client.navigation.open("conflicts") },
                            )
                            if (!localMode) DropdownMenuItem(
                                text = { Text(str(R.string.text_activity_on_this_device_288740a)) },
                                leadingIcon = { Icon(painterResource(R.drawable.glyph_audit), null) },
                                onClick = { tools = false; client.navigation.open("secret-activity") },
                            )
                            HorizontalDivider(Modifier.padding(vertical = 4.dp))
                            DropdownMenuItem(
                                text = { Text(str(R.string.vault_backup_title)) },
                                leadingIcon = { Icon(Icons.Outlined.Backup, null) },
                                onClick = { tools = false; client.navigation.open("vault-backup") },
                            )
                            DropdownMenuItem(
                                text = { Text(str(R.string.text_import_passwords_16088a6)) },
                                leadingIcon = { Icon(Icons.Outlined.FileDownload, null) },
                                onClick = { tools = false; client.navigation.open("vault-import") },
                            )
                            if (!localMode && savedAccounts.valueOrNull()?.any { it.id == LOCAL_ACCOUNT } == true) {
                                DropdownMenuItem(
                                    text = { Text(str(R.string.import_local_account)) },
                                    leadingIcon = { Icon(Icons.Outlined.MoveToInbox, null) },
                                    onClick = { tools = false; client.navigation.open("local-import") },
                                )
                            }
                            if (!localMode) DropdownMenuItem(
                                text = { Text(str(R.string.text_import_secret_package_51433fa)) },
                                leadingIcon = { Icon(Icons.Outlined.Shield, null) },
                                onClick = { tools = false; client.navigation.open("secret-import") },
                            )
                            if (!localMode) DropdownMenuItem(
                                text = { Text(str(R.string.text_shared_snapshots_08679a9)) },
                                leadingIcon = { Icon(painterResource(R.drawable.glyph_link), null) },
                                onClick = { tools = false; client.navigation.open("secret-shares") },
                            )
                            HorizontalDivider(Modifier.padding(vertical = 4.dp))
                            DropdownMenuItem(
                                text = { Text(str(R.string.text_trash_8af1079)) },
                                leadingIcon = { Icon(Icons.Outlined.DeleteOutline, null) },
                                onClick = { tools = false; client.vault.toggleTrash() },
                            )
                        }
                    }
                }
            }) { searching ->
            if (!searching && !query.trash) ScreenFab {
                val atTop by remember { derivedStateOf { scroll.firstVisibleItemIndex == 0 } }
                ExtendedFloatingActionButton(onClick = { client.vaultDrafts.open(null) }, expanded = atTop,
                    icon = { Icon(Icons.Outlined.Add, str(R.string.text_new_item_9916f9f)) }, text = { Text(str(R.string.text_new_item_9916f9f)) })
            }
            LazyColumn(Modifier.fillMaxSize(), state = scroll, contentPadding = pagePadding(top = 0.dp, bottom = FabClearance)) {
                if (query.trash) item { Text(str(R.string.text_trash_8af1079), style = MaterialTheme.typography.titleMedium) }
                browseScopesItem(searching) {
                    BrowseScopes(trailing = {
                        IconButton(onClick = { filters = true }, modifier = Modifier.semantics { query.tag?.let { stateDescription = it } }) {
                            BadgedBox(badge = { if (query.tag != null) Badge() }) {
                                Icon(Icons.Outlined.FilterList, str(R.string.browse_filters))
                            }
                        }
                    }) {
                        BrowseChoice(str(R.string.text_all_eae0ae1), !query.favorites && query.kind == null && query.tag == null) {
                            changeQuery(VaultQuery(text = query.text, sort = query.sort, trash = query.trash))
                        }
                        BrowseChoice(str(R.string.text_favorites_960eb5d), query.favorites, toggle = true) { changeQuery(query.copy(favorites = !query.favorites)) }
                        BrowseChoice(str(R.string.browse_passwords), query.kind == "login") { changeQuery(query.copy(kind = "login")) }
                        BrowseChoice(vaultKindName("note"), query.kind == "note") { changeQuery(query.copy(kind = "note")) }
                        val other = query.kind?.takeUnless { it == "login" || it == "note" }
                        MenuChoice(other?.let(::vaultKindName) ?: str(R.string.text_type_6172196), other != null) { close ->
                            DropdownMenuItem(
                                text = { Text(str(R.string.text_all_types_338629e)) },
                                leadingIcon = { Icon(painterResource(R.drawable.glyph_vault), null) },
                                trailingIcon = if (query.kind == null) ({ Icon(Icons.Outlined.Check, null) }) else null,
                                onClick = { close(); changeQuery(query.copy(kind = null)) },
                            )
                            HorizontalDivider(Modifier.padding(vertical = 4.dp))
                            vaultKinds().forEach { kind ->
                                val count = page?.kinds?.get(kind)
                                DropdownMenuItem(
                                    text = { Text(vaultKindName(kind)) },
                                    leadingIcon = { Icon(painterResource(vaultGlyph(kind)), null) },
                                    trailingIcon = if (query.kind == kind || count != null) { {
                                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                            count?.let { Text(localizedNumber(it), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                                            if (query.kind == kind) Icon(Icons.Outlined.Check, null)
                                        }
                                    } } else null,
                                    onClick = { close(); changeQuery(query.copy(kind = kind)) },
                                )
                            }
                        }
                    }
                }
                item { ReadFeedback(read) { client.vault.browse(query) } }
                val rows = page?.items.orEmpty()
                items(rows, key = { it.id }) { item ->
                    BrowseRow(item.title, Modifier.animateItem().clickable { select(item.id) }, leading = { SecretIcon(item.kind) },
                        supporting = { Text(item.subtitle.ifEmpty { vaultKindName(item.kind) }, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                        trailing = {
                            if (item.reserved) Icon(Icons.Outlined.VerifiedUser, str(R.string.text_managed_by_dmsg_35b3681), Modifier.size(18.dp))
                            if (item.conflict) Icon(Icons.Outlined.WarningAmber, str(R.string.text_version_conflict_f71b5e8), Modifier.size(18.dp), tint = MaterialTheme.colorScheme.error)
                            if (!item.synced && item.saveState != "local_only") Icon(Icons.Outlined.Sync, str(R.string.text_waiting_to_sync_865c0a0), Modifier.size(16.dp),
                                tint = MaterialTheme.colorScheme.onSurfaceVariant)
                            if (item.favorite) Icon(Icons.Outlined.Star, str(R.string.text_favorite_1fd85c7), Modifier.size(18.dp))
                        })
                }
                if (read is ReadState.Empty) item {
                    EmptyState(str(when { query.filtered -> R.string.text_no_matching_items_b35c7c3; query.trash -> R.string.text_trash_is_empty_c562cd3; else -> R.string.autofill_no_items }),
                        icon = when { query.trash -> Icons.Outlined.DeleteOutline; query.filtered -> Icons.Outlined.Search; else -> null },
                        glyph = if (!query.trash && !query.filtered) R.drawable.glyph_vault else null) {
                        if (!query.trash && !query.filtered) TextButton(onClick = { client.navigation.open("vault-backup") }) { Text(str(R.string.vault_backup_import)) }
                        if (query.text.isNotEmpty()) {
                            TextButton(onClick = { changeQuery(query.copy(text = "")) }) { Text(str(R.string.text_clear_search_3f4dec6)) }
                            if (!query.trash) TextButton(onClick = { client.vaultDrafts.open(null) }) { Text(str(R.string.text_new_item_9916f9f)) }
                        } else if (query.filtered) TextButton(onClick = { changeQuery(VaultQuery(sort = query.sort, trash = query.trash)) }) { Text(str(R.string.text_clear_filters_5ea61be)) }
                    }
                }
                if (page?.hasMore == true) item { TextButton(onClick = { client.vault.browse(query, more = true) }, enabled = read !is ReadState.Loading, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_load_more_4e1d9f8)) } }
            }
        }
    }
    if (filters) VaultFilterSheet(query, page, ::changeQuery, { filters = false })
}

/** Sort as a radio list, tags as chips, and Clear and Done at the bottom. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable private fun VaultFilterSheet(query: VaultQuery, page: VaultListing?, update: (VaultQuery) -> Unit, close: () -> Unit) {
    ModalBottomSheet(onDismissRequest = close) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = pageMargin()).padding(bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(str(R.string.browse_filters), style = MaterialTheme.typography.titleLarge)
            page?.let {
                Text(quantity(R.plurals.items, it.total) + " · " + str(R.string.text_favorites_960eb5d) + " · " + localizedNumber(it.favorites),
                    style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Section(title = str(R.string.sort_by)) {
                listOf("title" to R.string.text_title_4c18fa6, "updated" to R.string.text_updated_9306ca8).forEach { (key, label) ->
                    RadioChoiceRow(str(label), query.sort == key) { update(query.copy(sort = key)) }
                }
            }
            val tags = page?.tags.orEmpty()
            if (tags.isNotEmpty()) {
                SectionTitle(str(R.string.text_tags_33e9c4d))
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    BrowseChoice(str(R.string.text_all_tags_b8ceb46), query.tag == null) { update(query.copy(tag = null)) }
                    tags.forEach { tag -> BrowseChoice(tag, query.tag == tag) { update(query.copy(tag = tag)) } }
                }
            }
            Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                TextButton(onClick = { update(VaultQuery(text = query.text, sort = query.sort, trash = query.trash)) }) { Text(str(R.string.text_clear_filters_5ea61be)) }
                Button(onClick = close) { Text(str(R.string.text_done_9ffd599)) }
            }
        }
    }
}
