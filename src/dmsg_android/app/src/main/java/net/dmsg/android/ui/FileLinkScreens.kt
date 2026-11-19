package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.quantity
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str
import android.content.Intent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun FileLinkComposerScreen(client: ClientController, id: String, version: String?, backup: Boolean) {
    val state by client.state.collectAsStateWithLifecycle()
    val target by client.fileLinks.target.state.collectAsStateWithLifecycle()
    val draft by client.fileLinks.draft.collectAsStateWithLifecycle()
    val value = target.valueOrNull()
    val working = state.working("links:prepare")
    LaunchedEffect(id, version, backup, state.epoch) { client.fileLinks.open(id, version, backup) }
    DisposableEffect(id, version, backup) { onDispose { client.fileLinks.leaveComposer() } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(target, { client.fileLinks.open(id, version, backup) })
        value?.let { Text(it.name, style = MaterialTheme.typography.headlineSmall); Text(fileSize(it.size) + " · " + it.mime) }
        Notice(str(R.string.text_anyone_with_this_link_can_download_the_file_until_it_expires_or_y_7f123e4))
        if (value?.remote == true) Notice(str(R.string.text_this_cloud_copy_is_downloaded_temporarily_to_create_the_share_it_41b5d50))
        SectionTitle(str(R.string.text_link_limits_653a1af))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(1, 7, 30).forEach { days -> FilterChip(draft.duration == days.toULong() * 86_400_000u, { client.fileLinks.edit(draft.copy(duration = days.toULong() * 86_400_000u)) }, { Text(quantity(R.plurals.days, days)) }, enabled = !working) }
        }
        Text(str(R.string.text_downloads_allowed_c461d5b), style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(0uL, 1uL, 5uL, 10uL, 50uL, 100uL).forEach { count -> FilterChip(draft.maxDownloads == count, { client.fileLinks.edit(draft.copy(maxDownloads = count)) }, { Text(if (count == 0uL) str(R.string.text_unlimited_de7b472) else localizedNumber(count)) }, enabled = !working) }
        }
        OutlinedTextField(draft.password, { client.fileLinks.edit(draft.copy(password = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), singleLine = true, enabled = !working,
            label = { Text(str(R.string.text_optional_password_d741ed3)) }, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password))
        SettingToggle(str(R.string.text_show_my_public_profile_21b825f), draft.showProfile,
            { client.fileLinks.edit(draft.copy(showProfile = it)) }, modifier = Modifier.testTag("file-link-profile"), enabled = !working)
        Text(str(R.string.text_anyone_with_this_link_can_see_your_public_profile_even_before_ent_c412297), style = MaterialTheme.typography.bodySmall)
        if (value != null && value.size > 100uL * 1024u * 1024u) Notice(str(R.string.text_share_links_support_files_up_to_100_mib_244bc00), true)
        Button(onClick = client.fileLinks::create, enabled = value != null && value.size <= 100uL * 1024u * 1024u && target is ReadState.Ready && !working, modifier = Modifier.fillMaxWidth()) {
            if (working) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp) else Icon(Icons.Outlined.Link, null)
            Spacer(Modifier.width(8.dp)); Text(str(R.string.text_create_link_2ae3b5b))
        }
    }
}

@Composable fun FileLinksScreen(client: ClientController, selected: String, snapshots: Boolean = false, vaultItem: String? = null) {
    val state by client.state.collectAsStateWithLifecycle()
    val links by client.fileLinks.links.state.collectAsStateWithLifecycle()
    val intents by client.fileLinks.intents.state.collectAsStateWithLifecycle()
    LaunchedEffect(state.epoch) { client.fileLinks.browse(false, refreshAfter = true) }
    val rows = links.valueOrNull().orEmpty().filter {
        if (snapshots) it.source == "vault_snapshot" && (vaultItem == null || it.vaultItem == vaultItem) else it.source != "vault_snapshot"
    }
    val current = links.valueOrNull()?.find { it.id == selected }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(if (snapshots) str(R.string.text_share_links_fe69aa5) else if (selected.isEmpty()) str(R.string.text_shared_files_b3cfe3a) else str(R.string.text_share_link_b335ffa), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                IconButton(onClick = { client.fileLinks.browse() }) { Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_links_5941c71)) }
            }
            ReadFeedback(links, { client.fileLinks.browse() })
        }
        if (selected.isEmpty()) {
            if (!snapshots) item { ReadFeedback(intents, client.fileLinks::loadIntents) }
            items(intents.valueOrNull().orEmpty().filter { !snapshots && it.stage != "complete" }, key = { "intent:" + it.id }) { job ->
                ListItem(modifier = Modifier.clickable { client.navigation.push(Route("file-link-job", id = job.id)) }, headlineContent = { Text(job.name) },
                    leadingContent = { Icon(Icons.Outlined.PendingActions, null) }, supportingContent = { Text(str(R.string.text_unfinished_creation_continue_the_original_link_059e094)) })
            }
            items(rows, key = { "link:" + it.id }) { link ->
                ListItem(modifier = Modifier.clickable {
                    if (snapshots) client.navigation.push(Route("file-links", link.id)) else client.navigation.select(link.id)
                }, headlineContent = { Text(link.name ?: str(R.string.text_encrypted_share_34c22e9)) },
                    leadingContent = { Icon(if (link.password) Icons.Outlined.Lock else Icons.Outlined.Link, null) },
                    supportingContent = { Text(fileLinkState(link.state) + " · " + str(R.string.text_downloads_35a65b7) + localizedNumber(link.downloads)) })
            }
            if (snapshots && links.valueOrNull() != null && rows.isEmpty()) item { EmptyPanel(str(R.string.no_matches)) }
            if (!snapshots && links.valueOrNull() != null && rows.isEmpty() && intents.valueOrNull().orEmpty().none { it.stage != "complete" }) item { Notice(str(R.string.text_choose_share_by_link_on_a_file_or_cloud_backup_secret_snapshots_a_e246762)) }
        } else if (current != null) item { FileLinkDetails(client, current) }
        else if (links !is ReadState.Loading) item { Notice(str(R.string.text_this_link_is_not_available_in_the_current_account_s_saved_list_fab3a8d)) }
    }
}

@Composable fun FileLinkJobScreen(client: ClientController, id: String) {
    val state by client.state.collectAsStateWithLifecycle()
    val intents by client.fileLinks.intents.state.collectAsStateWithLifecycle()
    val progress by client.fileLinks.progress.collectAsStateWithLifecycle()
    val job = intents.valueOrNull()?.find { it.id == id }
    val working = state.working("links:run:$id")
    var discard by remember(id) { mutableStateOf(false) }
    LaunchedEffect(id, state.epoch) { client.fileLinks.loadIntents() }
    DisposableEffect(id) { onDispose { if (client.state.value.working("links:run:$id")) client.fileLinks.pause(id) } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(intents, client.fileLinks::loadIntents)
        if (job != null) {
            Text(job.name, style = MaterialTheme.typography.headlineSmall)
            Text(fileSize(job.size) + " · " + job.mime)
            if (job.link != null) FileLinkDetails(client, job.link)
            else {
                Text(str(R.string.text_expires_after_a5d9473) + quantity(R.plurals.days, job.duration / 86_400_000u))
                Text(str(R.string.text_downloads_allowed_1eb18d0) + if (job.maxDownloads == 0uL) str(R.string.text_unlimited_de7b472) else localizedNumber(job.maxDownloads))
                Text(if (job.password) str(R.string.text_password_protected_21484fe) else str(R.string.text_the_link_contains_the_file_key_ed180c0))
                Text(if (job.showProfile) str(R.string.text_your_public_profile_is_visible_a7e8423) else str(R.string.text_your_public_profile_is_not_included_c468316))
                if (working) {
                    Text(str(R.string.text_encrypting_and_publishing_the_selected_copy_84ee298))
                    val value = progress[id]
                    if (value != null && value.total > 0u) LinearProgressIndicator(progress = { (value.completed.toDouble() / value.total.toDouble()).toFloat().coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                    else LinearProgressIndicator(Modifier.fillMaxWidth())
                    TextButton(onClick = { client.fileLinks.pause(id) }) { Text(str(R.string.text_pause_and_keep_this_creation_31febec)) }
                } else {
                    job.error?.let { Notice(errorText(it), true) }
                    Notice(str(R.string.text_continue_this_creation_to_check_its_original_result_or_finish_the_8bbb036))
                    Button(onClick = { client.fileLinks.resume(id) }, enabled = job.stage in setOf("prepared", "uploading", "checking")) { Text(str(R.string.text_continue_original_creation_6dc4b60)) }
                    if (job.canDiscard) TextButton(onClick = { discard = true }) { Text(str(R.string.text_discard_unpublished_draft_8dbb0b0)) }
                }
            }
        }
    }
    if (discard) AlertDialog(onDismissRequest = { discard = false }, title = { Text(str(R.string.text_discard_this_link_draft_2c4e5e6)) }, text = { Text(str(R.string.text_no_publication_has_started_the_source_file_remains_846f4e3)) },
        confirmButton = { TextButton(onClick = { client.fileLinks.discard(id) { discard = false; client.navigation.back() } }, enabled = !state.working("links:discard:$id")) { Text(str(R.string.text_discard_475cd3e)) } },
        dismissButton = { TextButton(onClick = { discard = false }) { Text(str(R.string.text_keep_draft_89639c8)) } })
}

@Composable private fun FileLinkDetails(client: ClientController, link: FileLink) {
    val state by client.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    var revoke by remember(link.id) { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(fileLinkState(link.state), style = MaterialTheme.typography.titleMedium)
        Text(str(R.string.text_downloads_0cf6727) + link.downloads + if (link.maxDownloads == 0uL) "" else " / ${link.maxDownloads}")
        Text(str(R.string.text_expires_4972e43) + messageTime(link.expiresAt))
        if (link.password) Notice(str(R.string.text_send_the_password_through_another_channel_1e76ba2))
        if (link.showProfile) Text(str(R.string.text_your_public_profile_is_visible_before_entering_the_password_dd33aaf), style = MaterialTheme.typography.bodySmall)
        if (link.active && link.url != null) {
            Button(onClick = {
                if (link.active) { client.beginExternalInteraction(); context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, link.url) }, null)) }
            }, modifier = Modifier.fillMaxWidth()) { Icon(Icons.Outlined.Share, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_share_link_b335ffa)) }
            OutlinedButton(onClick = {
                val now = System.currentTimeMillis().coerceAtLeast(0).toULong()
                if (link.active && now < link.expiresAt) client.clipboard.copy(link.url, (link.expiresAt - now).coerceAtMost(30_000u).toLong())
            }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_copy_link_8e702f5)) }
        } else if (link.active) Notice(str(R.string.text_the_full_link_is_unavailable_on_this_device_synchronize_the_accou_c700370))
        link.observedAt?.let { Text(str(R.string.text_last_checked_368f738) + messageTime(it), style = MaterialTheme.typography.labelSmall) }
        if (link.state != "revoked") TextButton(onClick = { revoke = true }, enabled = !state.working("links:revoke:${link.id}")) { Text(str(R.string.text_revoke_link_37b1e72), color = MaterialTheme.colorScheme.error) }
    }
    if (revoke) AlertDialog(onDismissRequest = { revoke = false }, title = { Text(str(R.string.text_revoke_this_link_6117c10)) }, text = { Text(str(R.string.text_this_stops_new_download_authorizations_already_downloaded_copies_53695df)) },
        confirmButton = { TextButton(onClick = { client.fileLinks.revoke(link.id); revoke = false }) { Text(str(R.string.text_revoke_411a6c9)) } },
        dismissButton = { TextButton(onClick = { revoke = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

internal fun fileLinkState(state: String) = when (state) {
    "active" -> str(R.string.text_active_45e5d33)
    "revoked" -> str(R.string.text_revoked_c4fbad3)
    "expired" -> str(R.string.text_expired_70446d7)
    "exhausted" -> str(R.string.text_download_limit_reached_56d3e8b)
    else -> str(R.string.text_unknown_status_310134f) + state
}
