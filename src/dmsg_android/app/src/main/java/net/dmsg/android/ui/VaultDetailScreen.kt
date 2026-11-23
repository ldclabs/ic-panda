package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import android.os.SystemClock
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextDirection
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import net.dmsg.android.core.*
import net.dmsg.core.*
import java.net.URI
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription

fun displayTime(ms: ULong): String = if (ms > Long.MAX_VALUE.toULong()) str(R.string.text_unknown_d748bba) else
    runCatching { java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM, java.text.DateFormat.MEDIUM,
        net.dmsg.android.platform.AppLanguage.locale()).format(java.util.Date(ms.toLong())) }.getOrDefault(str(R.string.text_unknown_d748bba))

@Composable fun SecretScreen(client: ClientController, id: String, edit: (VaultItem) -> Unit) {
    val read by client.vault.detail.state.collectAsStateWithLifecycle()
    LaunchedEffect(id) { client.vault.select(id) }
    SecretDetailBody(client, id, read, false, { client.vault.select(id) }, edit)
}

@Composable fun HistoricalSecretScreen(client: ClientController, id: String, revision: String) {
    val read by client.vaultHistory.historical.state.collectAsStateWithLifecycle()
    LaunchedEffect(id, revision) { client.vaultHistory.select(id, revision) }
    SecretDetailBody(client, id, read, true, { client.vaultHistory.select(id, revision) }) { }
}

@Composable private fun SecretDetailBody(client: ClientController, id: String, read: ReadState<VaultItem>, historical: Boolean, reload: () -> Unit, edit: (VaultItem) -> Unit) {
    val item = read.valueOrNull()?.takeIf { it.id == id }
    val state by client.state.collectAsStateWithLifecycle()
    val changing = state.working("vault:edit:" + id)
    var deleting by remember(id) { mutableStateOf(false) }
    val scroll = rememberScrollState()
    LaunchedEffect(item?.id, item?.deleted) { if (item != null) scroll.scrollTo(0) }
    Column(Modifier.fillMaxSize().verticalScroll(scroll).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(read, reload)
        if (item != null) {
            DetailCard {
                if (item.saveState == "local_only") Text(str(R.string.saved_on_device), style = MaterialTheme.typography.labelMedium)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    SecretIcon(item.kind)
                    Column(Modifier.weight(1f)) { Text(item.title, style = MaterialTheme.typography.headlineSmall); Text(vaultKindName(item.kind), color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    if (!historical && !item.reserved && !item.deleted) {
                        IconToggleButton(checked = item.favorite, onCheckedChange = { client.vault.favorite(item) }, enabled = !changing) {
                            Icon(if (item.favorite) Icons.Outlined.Star else Icons.Outlined.StarBorder, str(R.string.text_favorite_898f4c6))
                        }
                        IconButton(onClick = { edit(item) }) { Icon(Icons.Outlined.Edit, str(R.string.text_edit_item_3638f4f)) }
                    }
                }
            }
            if (historical) { Notice(str(R.string.text_you_are_viewing_an_immutable_historical_version_0b9544a)); SelectionContainer { Text(item.revision, style = MaterialTheme.typography.bodySmall) } }
            if (item.deleted) {
                Notice(str(R.string.text_this_item_is_in_the_trash_restore_it_to_edit_history_is_retained_025a0c4))
                if (!historical && !item.reserved) Button(onClick = { client.perform({ it.restoreItem(id) }, key = "vault:edit:" + id) {
                    client.vault.changed(); client.vault.select(id); client.notice("VAULT_RESTORED")
                } }, enabled = !changing) { Text(str(R.string.text_restore_item_1f6ceaa)) }
            }
            if (item.conflict) { Notice(str(R.string.text_this_item_has_another_version_review_the_conflict_before_replacin_763272d), true); TextButton(onClick = { client.navigation.open("conflicts") }) { Text(str(R.string.text_review_sync_conflicts_7186d41)) } }
            if (item.tags.isNotEmpty()) Text(item.tags.joinToString(" · "), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
            var section = ""
            item.fields.filter { it.value.isNotEmpty() }.forEach { field ->
                if (field.section != section) { section = field.section; if (section.isNotBlank()) SectionTitle(section) }
                key(item.revision, field.id) { SecretField(client, item, field, historical) }
            }
            if (item.notes.isNotEmpty()) { SectionTitle(str(R.string.text_notes_f7df47b)); SelectionContainer { Text(item.notes) } }
            if (item.files.isNotEmpty()) {
                SectionTitle(str(R.string.text_attachments_be5733e))
                if (historical) item.files.forEach { file -> SelectionContainer { Text(file, style = MaterialTheme.typography.bodySmall) } }
                else OutlinedButton(onClick = { client.navigation.push(Route("secret-attachments", id)) }) { Text(str(R.string.text_view_attachments_50edfcd) + " · " + localizedNumber(item.files.size)) }
            }
            Text(str(R.string.text_updated_7676bd2) + displayTime(item.updatedAt), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(str(R.string.text_created_c1dc4dc) + displayTime(item.createdAt), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (!historical) TextButton(onClick = { client.navigation.push(Route("vault-history", id)) }) { Text(str(R.string.text_version_history_8a6bf11)) }
            if (item.saveState != "local_only" && !historical && !item.reserved && !item.deleted) {
                OutlinedButton(onClick = { client.navigation.push(Route("secret-compose", id)) }, enabled = !item.conflict) { Text(str(R.string.text_share_secret_snapshot_d4484d8)) }
                TextButton(onClick = { client.navigation.push(Route("vault-links", id)) }) { Text(str(R.string.text_share_links_fe69aa5)) }
            }
            if (item.reserved) Notice(str(R.string.text_this_key_is_managed_by_dmsg_its_private_fields_cannot_be_revealed_4a768a2))
            else if (!historical && !item.deleted) TextButton(onClick = { deleting = true }) { Text(str(R.string.text_move_to_trash_c4056f9), color = MaterialTheme.colorScheme.error) }
        }
    }
    if (deleting && item != null) AlertDialog(onDismissRequest = { if (!changing) deleting = false }, title = { Text(str(R.string.text_move_this_item_to_trash_2a04c82)) },
        text = { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(item.title, style = MaterialTheme.typography.titleMedium)
            if (state.session?.accountKind != "local") Text(str(R.string.text_a_deletion_marker_will_sync_to_your_other_devices_version_history_18e2556))
        } },
        confirmButton = { TextButton(onClick = { client.perform({ it.deleteItem(id) }, key = "vault:edit:" + id) { deleting = false; client.vault.changed(); client.vault.select(id); client.notice("VAULT_TRASHED") } }, enabled = !changing) { Text(str(R.string.text_move_to_trash_c4056f9), color = MaterialTheme.colorScheme.error) } },
        dismissButton = { TextButton(onClick = { deleting = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@OptIn(ExperimentalFoundationApi::class)
@Composable private fun SecretField(client: ClientController, item: VaultItem, field: VaultField, historical: Boolean) {
    var revealed by remember { mutableStateOf<String?>(null) }
    var otp by remember { mutableStateOf<String?>(null) }
    var deadline by remember { mutableLongStateOf(0) }
    var tick by remember { mutableLongStateOf(SystemClock.elapsedRealtime()) }
    var menu by remember { mutableStateOf(false) }
    var large by remember { mutableStateOf<String?>(null) }
    val uri = LocalUriHandler.current
    val isOtp = field.kind == "otp"
    val resource = "vault:field:" + item.id + ":" + item.revision + ":" + field.id
    var otpFailed by remember { mutableStateOf(false) }
    var copiedAt by remember { mutableLongStateOf(0) }
    LaunchedEffect(copiedAt) { if (copiedAt != 0L) { delay(2000); copiedAt = 0L } }
    DisposableEffect(resource) {
        onDispose { client.cancelOperation(resource); revealed = null; otp = null; large = null; deadline = 0 }
    }
    val label = vaultFieldTitle(field.id, field.kind, field.label)
    val leftToRight = field.concealed || field.kind in setOf("url", "email", "otp", "date", "month", "phone") ||
        field.id in setOf("private_key", "public_key", "fingerprint", "number", "cvv", "pin") || field.value.contains('@')
    val direction = if (leftToRight) TextDirection.Ltr else TextDirection.Content
    fun readValue(action: (SecretValue) -> Unit) {
        val requested = SystemClock.elapsedRealtime()
        client.perform({ it.revealField(item.id, item.revision, field.id) }, key = resource, replace = true,
            onError = { if (isOtp) otpFailed = true }) { received ->
            val left = received.remainingMs?.let { duration -> secretRemaining(duration, requested, SystemClock.elapsedRealtime()) }
            if (isOtp && (left == null || left == 0uL)) { otp = null; return@perform }
            val value = received.copy(remainingMs = left)
            if (isOtp) { otp = value.value; tick = SystemClock.elapsedRealtime(); deadline = tick + (left ?: 0u).coerceAtMost(300_000u).toLong() }
            action(value)
        }
    }
    fun copyValue() = readValue {
        client.clipboard.copy(it.value, (it.remainingMs ?: 30_000u).coerceAtMost(30_000u).toLong())
        copiedAt = SystemClock.elapsedRealtime()
    }
    LaunchedEffect(item.revision, field.id, historical) {
        otp = null; revealed = null; deadline = 0; otpFailed = false
        if (isOtp && !historical && !item.reserved && !item.deleted) while (!otpFailed) {
            tick = SystemClock.elapsedRealtime()
            if (tick >= deadline) { otp = null; readValue { } }
            delay(1000)
        }
    }
    LaunchedEffect(deadline) {
        if (deadline > 0) {
            while (SystemClock.elapsedRealtime() < deadline) {
                tick = SystemClock.elapsedRealtime()
                delay(minOf(1000, (deadline - tick).coerceAtLeast(1)))
            }
            otp = null; tick = SystemClock.elapsedRealtime()
        }
    }
    val value = if (isOtp) otp?.takeIf { tick < deadline } ?: if (historical) "••••••••" else "—" else if (field.concealed) revealed ?: "••••••••" else field.value
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.card)) {
        Row(Modifier.fillMaxWidth().heightIn(min = 72.dp).semantics { if (copiedAt != 0L) stateDescription = str(R.string.vault_copied) }
            .combinedClickable(enabled = !item.reserved && !item.deleted, role = Role.Button, onClickLabel = str(R.string.text_copy_d6e9651) + " · " + label, onClick = ::copyValue,
                onLongClickLabel = str(R.string.text_field_actions_a2183fb), onLongClick = { menu = true }).padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Spacer(Modifier.height(6.dp)); Text(value, modifier = if (isOtp) Modifier.clearAndSetSemantics { contentDescription = str(R.string.otp_accessibility_hint) } else Modifier, style = (if (isOtp) MaterialTheme.typography.headlineSmall.tabular() else MaterialTheme.typography.bodyLarge).copy(textDirection = direction))
                if (isOtp && deadline > tick) Text(str(R.string.text_expires_in_679949a) + localizedNumber((deadline - tick + 999) / 1000) + str(R.string.text_s_4b54bb0), style = MaterialTheme.typography.labelSmall.tabular())
            }
            if (!item.reserved && !item.deleted) Box {
                IconButton(onClick = { menu = true }) { Icon(if (copiedAt != 0L) Icons.Outlined.Check else Icons.Outlined.MoreVert, str(R.string.text_field_actions_a2183fb) + " · " + label) }
                DropdownMenu(menu, { menu = false }) {
                    DropdownMenuItem(text = { Text(str(R.string.text_copy_d6e9651)) }, onClick = { menu = false; copyValue() })
                    if (field.concealed && (!isOtp || historical)) DropdownMenuItem(text = { Text(if ((if (isOtp) otp else revealed) == null) str(R.string.text_reveal_5917b8a) else str(R.string.text_hide_d2e660d)) }, onClick = { menu = false; if (isOtp && otp != null) { otp = null; deadline = 0 } else if (revealed != null) revealed = null else readValue { if (!isOtp) revealed = it.value } })
                    DropdownMenuItem(text = { Text(str(R.string.text_large_type_35f1ac3)) }, onClick = { menu = false; readValue { large = it.value } })
                    if (!historical) DropdownMenuItem(text = { Text(str(R.string.text_field_reference_and_rotation_449f016)) }, onClick = {
                        menu = false; client.navigation.push(Route("secret-reference", item.id, field = field.id))
                    })
                    val website = runCatching { URI(field.value) }.getOrNull()?.takeIf { field.kind == "url" && !field.concealed && it.scheme?.lowercase() in listOf("https", "http") && !it.host.isNullOrEmpty() }
                    if (website != null) DropdownMenuItem(text = { Text(str(R.string.text_open_website_2c9debd)) }, onClick = { menu = false; uri.openUri(website.toASCIIString()) })
                }
            }
        }
    }
    if (large != null) AlertDialog(onDismissRequest = { large = null }, title = { Text(label) }, text = { SelectionContainer { Text(if (isOtp) value else large!!, Modifier.heightIn(max = 500.dp).verticalScroll(rememberScrollState()), style = MaterialTheme.typography.headlineLarge.let { if (isOtp) it.tabular() else it }.copy(textDirection = direction)) } },
        confirmButton = { TextButton(onClick = { large = null }) { Text(str(R.string.text_done_9ffd599)) } },
        dismissButton = { TextButton(onClick = ::copyValue) { Text(str(if (copiedAt == 0L) R.string.text_copy_d6e9651 else R.string.vault_copied)) } })
}
