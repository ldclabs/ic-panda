package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.quantity
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
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
private fun kindIcon(kind: String): ImageVector = when (kind) {
    "login" -> Icons.Outlined.Key; "api" -> Icons.Outlined.Code; "ssh" -> Icons.Outlined.Terminal
    "wallet" -> Icons.Outlined.AccountBalanceWallet; "card" -> Icons.Outlined.CreditCard; "identity" -> Icons.Outlined.Badge
    "server" -> Icons.Outlined.Dns; "database" -> Icons.Outlined.Storage; "license" -> Icons.Outlined.Verified
    "key" -> Icons.Outlined.VpnKey; else -> Icons.Outlined.Description
}
@Composable internal fun SecretIcon(kind: String) {
    TonalIcon(kindIcon(kind), when (kind) {
        "note", "license", "identity" -> MaterialTheme.colorScheme.secondaryContainer
        "wallet", "card" -> MaterialTheme.colorScheme.tertiaryContainer
        else -> MaterialTheme.colorScheme.primaryContainer
    })
}

@Composable fun VaultScreen(client: ClientController, select: (String) -> Unit) {
    val localMode = client.state.collectAsStateWithLifecycle().value.session?.accountKind == "local"
    val read by client.vault.listing.state.collectAsStateWithLifecycle()
    val page = read.valueOrNull()
    val query by client.vault.filters.collectAsStateWithLifecycle()
    val scroll = workspaceListState(client.navigation, "vault:list")
    var types by remember { mutableStateOf(false) }
    var tags by remember { mutableStateOf(false) }
    var tools by remember { mutableStateOf(false) }
    val savedAccounts by client.accountWorkspace.accounts.state.collectAsStateWithLifecycle()
    LaunchedEffect(tools, localMode) { if (tools && !localMode) client.accountWorkspace.accounts(resolveNames = false) }
    LaunchedEffect(Unit) { client.vault.browse() }
    LazyColumn(Modifier.fillMaxSize(), state = scroll, contentPadding = PaddingValues(start = 16.dp, top = 8.dp, end = 16.dp, bottom = 100.dp + LocalWorkspaceBarInset.current),
        verticalArrangement = Arrangement.spacedBy(2.dp)) {
        item(key = "controls") {
            WorkspaceSearchField(query.text, { client.vault.browse(query.copy(text = it.take(1000))) }, str(R.string.text_search_vault_f6e42b2))
            Spacer(Modifier.height(4.dp))
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(!query.favorites && query.kind == null && query.tag == null && !query.trash, { client.vault.browse(VaultQuery(text = if (query.trash) "" else query.text, sort = query.sort)) }, { Text(str(R.string.text_all_eae0ae1) + (page?.all?.let { " · " + localizedNumber(it) } ?: "")) })
                FilterChip(query.favorites, { client.vault.browse(query.copy(favorites = !query.favorites, trash = false)) }, { Text(str(R.string.text_favorites_960eb5d) + (page?.favorites?.let { " · " + localizedNumber(it) } ?: "")) })
                Box {
                    FilterChip(query.kind != null, { types = true }, { Text(query.kind?.let(::vaultKindName) ?: str(R.string.text_type_6172196)) })
                    DropdownMenu(types, { types = false }) {
                        DropdownMenuItem(text = { Text(str(R.string.text_all_types_338629e)) }, onClick = { client.vault.browse(query.copy(kind = null)); types = false })
                        vaultKinds().forEach { kind -> DropdownMenuItem(text = { Text(vaultKindName(kind) + " · " + localizedNumber(page?.kinds?.get(kind) ?: 0u)) }, onClick = { client.vault.browse(query.copy(kind = kind)); types = false }) }
                    }
                }
                Box {
                    FilterChip(query.tag != null, { tags = true }, { Text(query.tag ?: str(R.string.text_tags_33e9c4d)) })
                    DropdownMenu(tags, { tags = false }) {
                        DropdownMenuItem(text = { Text(str(R.string.text_all_tags_b8ceb46)) }, onClick = { client.vault.browse(query.copy(tag = null)); tags = false })
                        page?.tags?.forEach { tag -> DropdownMenuItem(text = { Text(tag) }, onClick = { client.vault.browse(query.copy(tag = tag)); tags = false }) }
                    }
                }
                FilterChip(query.trash, client.vault::toggleTrash, { Text(str(R.string.text_trash_8af1079)) })
            }
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(page?.total?.let { quantity(R.plurals.items, it) } ?: str(R.string.text_reading_vault_e577709), Modifier.weight(1f), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                SortMenu(query.sort, listOf("title" to R.string.text_title_4c18fa6, "updated" to R.string.text_updated_9306ca8)) {
                    client.vault.browse(query.copy(sort = it))
                }
                Box {
                    IconButton(onClick = { tools = true }) { Icon(Icons.Outlined.MoreHoriz, str(R.string.text_vault_tools_14fe2cc)) }
                    DropdownMenu(tools, { tools = false }) {
                        DropdownMenuItem(text = { Text(str(R.string.vault_backup_title)) }, onClick = { tools = false; client.navigation.open("vault-backup") })
                        DropdownMenuItem(text = { Text(str(R.string.text_local_security_review_b7f4013)) }, onClick = { tools = false; client.navigation.open("secret-health") })
                        if (!localMode) DropdownMenuItem(text = { Text(str(R.string.text_version_conflicts_0df1b3c)) }, onClick = { tools = false; client.navigation.open("conflicts") })
                        if (!localMode) DropdownMenuItem(text = { Text(str(R.string.text_activity_on_this_device_288740a)) }, onClick = { tools = false; client.navigation.open("secret-activity") })
                        if (!localMode) DropdownMenuItem(text = { Text(str(R.string.text_shared_snapshots_08679a9)) }, onClick = { tools = false; client.navigation.open("secret-shares") })
                        if (!localMode) DropdownMenuItem(text = { Text(str(R.string.text_import_secret_package_51433fa)) }, onClick = { tools = false; client.navigation.open("secret-import") })
                        DropdownMenuItem(text = { Text(str(R.string.text_import_passwords_16088a6)) }, onClick = { tools = false; client.navigation.open("vault-import") })
                        if (!localMode && savedAccounts.valueOrNull()?.any { it.id == LOCAL_ACCOUNT } == true) {
                            DropdownMenuItem(text = { Text(str(R.string.import_local_account)) }, onClick = { tools = false; client.navigation.open("local-import") })
                        }
                    }
                }
            }
            ReadFeedback(read) { client.vault.browse(query) }
        }
        val rows = page?.items.orEmpty()
        itemsIndexed(rows, key = { _, item -> item.id }) { index, item ->
            ListItem(modifier = Modifier.listRowShape(index, rows.size).clickable { select(item.id) }, headlineContent = { Text(item.title, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                supportingContent = { Text(item.subtitle.ifEmpty { vaultKindName(item.kind) }, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                leadingContent = { SecretIcon(item.kind) }, trailingContent = {
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        if (item.reserved) Icon(Icons.Outlined.VerifiedUser, str(R.string.text_managed_by_dmsg_35b3681), Modifier.size(18.dp))
                        if (item.conflict) Icon(Icons.Outlined.WarningAmber, str(R.string.text_version_conflict_f71b5e8), Modifier.size(18.dp), tint = MaterialTheme.colorScheme.error)
                        if (!item.synced && item.saveState != "local_only") Icon(Icons.Outlined.CloudUpload, str(R.string.text_waiting_to_sync_865c0a0), Modifier.size(18.dp))
                        if (item.favorite) Icon(Icons.Outlined.Star, str(R.string.text_favorite_1fd85c7), Modifier.size(18.dp))
                    }
                })
        }
        if (read is ReadState.Empty) item {
            EmptyPanel(str(when { query.filtered -> R.string.text_no_matching_items_b35c7c3; query.trash -> R.string.text_trash_is_empty_c562cd3; else -> R.string.autofill_no_items }),
                when { query.trash -> Icons.Outlined.DeleteOutline; query.filtered -> Icons.Outlined.Search; else -> Icons.Outlined.Lock }) {
                if (!query.trash && !query.filtered) TextButton(onClick = { client.navigation.open("vault-backup") }) { Text(str(R.string.vault_backup_import)) }
                if (query.text.isNotEmpty()) TextButton(onClick = { client.vault.browse(query.copy(text = "")) }) { Text(str(R.string.text_clear_search_3f4dec6)) }
                else if (query.filtered) TextButton(onClick = { client.vault.browse(VaultQuery(sort = query.sort, trash = query.trash)) }) { Text(str(R.string.text_clear_filters_5ea61be)) }
            }
        }
        if (page?.hasMore == true) item { TextButton(onClick = { client.vault.browse(query, more = true) }, enabled = read !is ReadState.Loading, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_load_more_4e1d9f8)) } }
    }
}
