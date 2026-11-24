package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.AppLanguage
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import android.icu.text.DisplayContext
import android.icu.text.RelativeDateTimeFormatter
import android.icu.util.ULocale
import android.os.SystemClock
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextDirection
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import net.dmsg.android.core.*
import net.dmsg.core.*
import java.net.URI
import java.time.Instant
import java.time.ZoneId
import java.time.temporal.ChronoUnit
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription

fun displayTime(ms: ULong): String = if (ms > Long.MAX_VALUE.toULong()) str(R.string.text_unknown_d748bba) else
    runCatching { java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM, java.text.DateFormat.MEDIUM,
        AppLanguage.locale()).format(java.util.Date(ms.toLong())) }.getOrDefault(str(R.string.text_unknown_d748bba))

/** "Now", "3 minutes ago", "Today at 07:21", "Yesterday at 07:21", or a date and time without seconds. */
fun relativeTime(ms: ULong, now: Long = System.currentTimeMillis()): String {
    if (ms == 0uL || ms > Long.MAX_VALUE.toULong()) return str(R.string.text_unknown_d748bba)
    val locale = AppLanguage.locale()
    val time = ms.toLong()
    val elapsed = now - time
    val relative = RelativeDateTimeFormatter.getInstance(ULocale.forLocale(locale), null, RelativeDateTimeFormatter.Style.LONG,
        DisplayContext.CAPITALIZATION_FOR_BEGINNING_OF_SENTENCE)
    if (elapsed in 0 until 60_000) return relative.format(RelativeDateTimeFormatter.Direction.PLAIN, RelativeDateTimeFormatter.AbsoluteUnit.NOW)
    if (elapsed in 60_000 until 3_600_000) return relative.format((elapsed / 60_000).toDouble(), RelativeDateTimeFormatter.Direction.LAST, RelativeDateTimeFormatter.RelativeUnit.MINUTES)
    val zone = ZoneId.systemDefault()
    val days = ChronoUnit.DAYS.between(Instant.ofEpochMilli(time).atZone(zone).toLocalDate(), Instant.ofEpochMilli(now).atZone(zone).toLocalDate())
    val clock = java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT, locale).format(java.util.Date(time))
    return when (days) {
        0L -> relative.combineDateAndTime(relative.format(RelativeDateTimeFormatter.Direction.THIS, RelativeDateTimeFormatter.AbsoluteUnit.DAY), clock)
        1L -> relative.combineDateAndTime(relative.format(RelativeDateTimeFormatter.Direction.LAST, RelativeDateTimeFormatter.AbsoluteUnit.DAY), clock)
        else -> java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM, java.text.DateFormat.SHORT, locale).format(java.util.Date(time))
    }
}

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

/**
 * The item's title, favourite and edit sit in the page bar and the rarer actions in its ⋮ menu.
 * Fields keep their sections; each row copies on tap and reveals inline.
 */
@Composable private fun SecretDetailBody(client: ClientController, id: String, read: ReadState<VaultItem>, historical: Boolean, reload: () -> Unit, edit: (VaultItem) -> Unit) {
    val item = read.valueOrNull()?.takeIf { it.id == id }
    val state by client.state.collectAsStateWithLifecycle()
    val changing = state.working("vault:edit:" + id)
    var deleting by remember(id) { mutableStateOf(false) }
    var more by remember(id) { mutableStateOf(false) }
    val haptics = LocalHapticFeedback.current
    val scroll = rememberScrollState()
    LaunchedEffect(item?.id, item?.deleted) { if (item != null) scroll.scrollTo(0) }
    ScreenBar(if (historical) null else item?.title) {
        if (item != null && !historical && !item.reserved && !item.deleted) {
            IconToggleButton(checked = item.favorite, onCheckedChange = { next ->
                haptics.performHapticFeedback(if (next) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff)
                client.vault.favorite(item)
            }, enabled = !changing) {
                // The filled star springs in; clearing it simply fades.
                AnimatedContent(item.favorite, transitionSpec = {
                    (if (targetState) scaleIn(Motion.spatialFast(), initialScale = 0.6f) else fadeIn(Motion.effectsFast())) togetherWith fadeOut(Motion.effectsFast())
                }, label = "favorite") { favorite ->
                    Icon(if (favorite) Icons.Outlined.Star else Icons.Outlined.StarBorder, str(R.string.text_favorite_898f4c6))
                }
            }
            IconButton(onClick = { edit(item) }) { Icon(Icons.Outlined.Edit, str(R.string.text_edit_item_3638f4f)) }
        }
        if (item != null && !historical) Box {
            IconButton(onClick = { more = true }) { Icon(Icons.Outlined.MoreVert, str(R.string.more)) }
            DropdownMenu(more, { more = false }) {
                DropdownMenuItem(text = { Text(str(R.string.text_version_history_8a6bf11)) }, leadingIcon = { Icon(Icons.Outlined.History, null) },
                    onClick = { more = false; client.navigation.push(Route("vault-history", id)) })
                if (item.saveState != "local_only" && !item.reserved && !item.deleted) {
                    DropdownMenuItem(text = { Text(str(R.string.text_share_secret_snapshot_d4484d8)) }, leadingIcon = { Icon(Icons.Outlined.Share, null) },
                        enabled = !item.conflict, onClick = { more = false; client.navigation.push(Route("secret-compose", id)) })
                    DropdownMenuItem(text = { Text(str(R.string.text_share_links_fe69aa5)) }, leadingIcon = { Icon(Icons.Outlined.Link, null) },
                        onClick = { more = false; client.navigation.push(Route("vault-links", id)) })
                }
                if (!item.reserved && !item.deleted) DropdownMenuItem(
                    text = { Text(str(R.string.text_move_to_trash_c4056f9), color = MaterialTheme.colorScheme.error) },
                    leadingIcon = { Icon(Icons.Outlined.DeleteOutline, null, tint = MaterialTheme.colorScheme.error) },
                    onClick = { more = false; deleting = true })
            }
        }
    }
    Column(Modifier.fillMaxSize().verticalScroll(scroll).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(read, reload)
        if (item != null) {
            if (historical) Text(item.title, style = MaterialTheme.typography.headlineSmall)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                SecretIcon(item.kind)
                Column(Modifier.weight(1f)) {
                    Text(vaultKindName(item.kind), style = MaterialTheme.typography.titleMedium)
                    Text(str(when {
                        item.deleted -> R.string.text_in_trash_3b5e1bf
                        item.conflict -> R.string.text_sync_conflicts_c35724e
                        item.saveState == "local_only" -> R.string.saved_on_device
                        item.synced -> R.string.synced
                        else -> R.string.text_waiting_to_sync_865c0a0
                    }), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            if (item.tags.isNotEmpty()) Text(item.tags.joinToString(" · "), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
            // One banner at most: the state that blocks the most comes first.
            when {
                item.conflict -> Notice(str(R.string.text_this_item_has_another_version_review_the_conflict_before_replacin_763272d), true) {
                    TextButton(onClick = { client.navigation.open("conflicts") }, colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onErrorContainer)) { Text(str(R.string.text_review_sync_conflicts_7186d41)) }
                }
                item.deleted -> Notice(str(R.string.text_this_item_is_in_the_trash_restore_it_to_edit_history_is_retained_025a0c4),
                    action = if (historical || item.reserved) null else { {
                        TextButton(onClick = { client.perform({ it.restoreItem(id) }, key = "vault:edit:" + id) {
                            client.vault.changed(); client.vault.select(id); client.notice("VAULT_RESTORED")
                        } }, enabled = !changing) { Text(str(R.string.text_restore_item_1f6ceaa)) }
                    } })
                historical -> Notice(str(R.string.text_you_are_viewing_an_immutable_historical_version_0b9544a))
                item.reserved -> Notice(str(R.string.text_this_key_is_managed_by_dmsg_its_private_fields_cannot_be_revealed_4a768a2))
            }
            val sections = remember(item.fields) {
                val groups = mutableListOf<MutableList<VaultField>>()
                item.fields.filter { it.value.isNotEmpty() }.forEach { field ->
                    val group = groups.lastOrNull()
                    if (group != null && group.first().section == field.section) group.add(field)
                    else groups.add(mutableListOf(field))
                }
                groups
            }
            sections.forEach { fields ->
                Section(title = fields.first().section.ifBlank { null }) {
                    fields.forEach { field -> key(item.revision, field.id) { SecretValueRow(client, item, field, historical) } }
                }
            }
            if (item.notes.isNotEmpty()) Section(title = str(R.string.text_notes_f7df47b)) {
                ListItem(headlineContent = { SelectionContainer { Text(item.notes) } }, modifier = Modifier.clipTo(rowShape()))
            }
            if (item.files.isNotEmpty()) Section(title = str(R.string.text_attachments_be5733e)) {
                if (historical) item.files.forEach { file ->
                    ListItem(headlineContent = { SelectionContainer { Text(file, style = MaterialTheme.typography.bodySmall.code()) } },
                        modifier = Modifier.clipTo(rowShape()))
                }
                else ListRow(str(R.string.text_view_attachments_50edfcd), icon = Icons.Outlined.AttachFile, value = localizedNumber(item.files.size),
                    onClick = { client.navigation.push(Route("secret-attachments", id)) })
            }
            Section(title = str(R.string.item_information)) {
                TimeRow(str(R.string.text_updated_9306ca8), item.updatedAt)
                TimeRow(str(R.string.text_created_36ba304), item.createdAt)
                if (historical) ListItem(headlineContent = { Text(str(R.string.text_version_7e42933)) },
                    supportingContent = { SelectionContainer { Text(item.revision, style = MaterialTheme.typography.bodySmall.code()) } },
                    modifier = Modifier.clipTo(rowShape()))
            }
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

/** A relative time; tapping shows the full date and time. */
@Composable private fun TimeRow(label: String, ms: ULong) {
    var full by remember(ms) { mutableStateOf(false) }
    ListRow(label, value = if (full) displayTime(ms) else relativeTime(ms), onClick = { full = !full })
}

/**
 * A field as a list row: tapping copies, the trailing icons reveal, open and copy, and a long press
 * offers large type and the field reference. The copy icon turns into a tick for two seconds.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable private fun SecretValueRow(client: ClientController, item: VaultItem, field: VaultField, historical: Boolean) {
    var revealed by remember { mutableStateOf<String?>(null) }
    var otp by remember { mutableStateOf<String?>(null) }
    var deadline by remember { mutableLongStateOf(0) }
    var period by remember { mutableLongStateOf(0) }
    var tick by remember { mutableLongStateOf(SystemClock.elapsedRealtime()) }
    var menu by remember { mutableStateOf(false) }
    var large by remember { mutableStateOf<String?>(null) }
    val uri = LocalUriHandler.current
    val haptics = LocalHapticFeedback.current
    val isOtp = field.kind == "otp"
    val readOnly = item.reserved || item.deleted
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
            val now = SystemClock.elapsedRealtime()
            val left = received.remainingMs?.let { duration -> secretRemaining(duration, requested, now) }
            if (isOtp && (left == null || left == 0uL)) { otp = null; return@perform }
            val value = received.copy(remainingMs = left)
            if (isOtp) {
                otp = value.value; tick = now
                val remaining = (left ?: 0u).coerceAtMost(300_000u).toLong()
                deadline = tick + remaining
                period = received.periodMs?.toLong() ?: 0
            }
            action(value)
        }
    }
    fun copyValue() = readValue {
        client.clipboard.copy(it.value, (it.remainingMs ?: 30_000u).coerceAtMost(30_000u).toLong())
        copiedAt = SystemClock.elapsedRealtime()
        haptics.performHapticFeedback(HapticFeedbackType.Confirm)
    }
    fun toggleReveal() {
        if (isOtp && otp != null) { otp = null; deadline = 0 } else if (revealed != null) revealed = null else readValue { if (!isOtp) revealed = it.value }
    }
    LaunchedEffect(item.revision, field.id, historical) {
        otp = null; revealed = null; deadline = 0; otpFailed = false
        if (isOtp && !historical && !readOnly) while (!otpFailed) {
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
    val code = otp?.takeIf { tick < deadline }
    // The ring sweeps evenly between the once-a-second ticks and fills at once for a new code.
    val left = if (code != null && period > 0) ((deadline - tick).toFloat() / period).coerceIn(0f, 1f) else 0f
    val ring = remember { Animatable(left) }
    LaunchedEffect(left) { if (left > ring.value) ring.snapTo(left) else ring.animateTo(left, Motion.tick()) }
    val shown = if (isOtp) code != null else revealed != null
    val value = if (isOtp) code ?: if (historical) "••••••••" else "—" else if (field.concealed) revealed ?: "••••••••" else field.value
    val website = if (field.kind == "url" && !field.concealed) {
        runCatching { URI(field.value) }.getOrNull()?.takeIf { it.scheme?.lowercase() in listOf("https", "http") && !it.host.isNullOrEmpty() }
    } else null
    val style = when {
        isOtp -> MaterialTheme.typography.headlineSmall.tabular()
        field.concealed -> MaterialTheme.typography.bodyLarge.code()
        else -> MaterialTheme.typography.bodyLarge
    }
    Box {
        ListItem(
            headlineContent = {
                Column {
                    Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Spacer(Modifier.height(4.dp))
                    // A value being revealed, and each new one-time code, develops from a blur; hiding is instant.
                    Text(value, modifier = (if (isOtp) Modifier.clearAndSetSemantics { contentDescription = str(R.string.otp_accessibility_hint) } else Modifier)
                        .develop(if (isOtp) code else revealed),
                        style = style.copy(textDirection = direction),
                        color = if (website != null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface)
                    if (code != null) Text(str(R.string.text_expires_in_679949a) + localizedNumber((deadline - tick + 999) / 1000) + str(R.string.text_s_4b54bb0),
                        style = MaterialTheme.typography.labelSmall.tabular(), color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            },
            trailingContent = if (readOnly) null else { {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (code != null && period > 0) CircularProgressIndicator(progress = { ring.value },
                        Modifier.padding(end = 8.dp).size(20.dp), strokeWidth = 2.5.dp)
                    if (field.concealed && (!isOtp || historical)) IconButton(onClick = ::toggleReveal) {
                        Icon(if (shown) Icons.Outlined.VisibilityOff else Icons.Outlined.Visibility,
                            str(if (shown) R.string.text_hide_d2e660d else R.string.text_reveal_5917b8a) + " · " + label)
                    }
                    if (website != null) IconButton(onClick = { uri.openUri(website.toASCIIString()) }) {
                        Icon(Icons.AutoMirrored.Outlined.OpenInNew, str(R.string.text_open_website_2c9debd) + " · " + label)
                    }
                    IconButton(onClick = ::copyValue) { CopyIcon(copiedAt != 0L, str(R.string.text_copy_d6e9651) + " · " + label) }
                }
            } },
            modifier = Modifier.clipTo(rowShape()).testTag("vault-field:" + field.id)
                .semantics { if (copiedAt != 0L) stateDescription = str(R.string.vault_copied) }
                .combinedClickable(enabled = !readOnly, role = Role.Button, onClickLabel = str(R.string.text_copy_d6e9651) + " · " + label, onClick = ::copyValue,
                    onLongClickLabel = str(R.string.text_field_actions_a2183fb), onLongClick = { menu = true })
                .heightIn(min = 72.dp),
        )
        DropdownMenu(menu, { menu = false }) {
            DropdownMenuItem(
                text = { Text(str(R.string.text_large_type_35f1ac3)) },
                leadingIcon = { Icon(Icons.Outlined.FormatSize, null) },
                onClick = { menu = false; readValue { large = it.value } },
            )
            if (!historical) DropdownMenuItem(
                text = { Text(str(R.string.text_field_reference_and_rotation_449f016)) },
                leadingIcon = { Icon(Icons.Outlined.Loop, null) },
                onClick = {
                    menu = false; client.navigation.push(Route("secret-reference", item.id, field = field.id))
                },
            )
        }
    }
    if (large != null) AlertDialog(onDismissRequest = { large = null }, title = { Text(label) }, text = { SelectionContainer { Text(if (isOtp) value else large!!, Modifier.heightIn(max = 500.dp).verticalScroll(rememberScrollState()), style = MaterialTheme.typography.headlineLarge.let { if (isOtp) it.tabular() else it }.copy(textDirection = direction)) } },
        confirmButton = { TextButton(onClick = { large = null }) { Text(str(R.string.text_done_9ffd599)) } },
        dismissButton = { TextButton(onClick = ::copyValue) { Text(str(if (copiedAt == 0L) R.string.text_copy_d6e9651 else R.string.vault_copied)) } })
}
