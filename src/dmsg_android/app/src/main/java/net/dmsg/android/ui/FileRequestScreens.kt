package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.quantity
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str
import android.content.Intent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun FileRequestsScreen(client: ClientController, selected: String) {
    val state by client.state.collectAsStateWithLifecycle()
    val links by client.fileRequests.links.state.collectAsStateWithLifecycle()
    val intents by client.fileRequests.intents.state.collectAsStateWithLifecycle()
    val capacity by client.cloudFiles.capacity.state.collectAsStateWithLifecycle()
    LaunchedEffect(state.epoch) { client.fileRequests.browse(false, refreshAfter = true); client.cloudFiles.capacity() }
    if (selected.isNotEmpty()) {
        val link = links.valueOrNull()?.find { it.id == selected }
        if (link != null) FileRequestDetailScreen(client, selected, link)
        else Column(Modifier.padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            ReadFeedback(links, { client.fileRequests.browse() })
            if (links is ReadState.Ready || links is ReadState.Empty) Notice(str(R.string.text_this_link_is_not_in_the_account_s_current_list_187258b))
        }
        return
    }
    val allowed = capacity.valueOrNull()?.requestLimit
    ScreenBar(null) {
        IconButton(onClick = { client.fileRequests.browse(); client.cloudFiles.capacity() }) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_cba212b))
        }
    }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (allowed == 0uL) Notice(str(R.string.text_creating_links_requires_plus_or_above_you_can_still_read_save_and_e4a97b9))
                DetailCard {
                    Text(str(R.string.text_send_a_link_so_someone_can_upload_without_an_account_only_you_can_0d42e5f), style = MaterialTheme.typography.bodyMedium)
                    when (allowed) {
                        0uL -> Unit
                        null -> Text(str(R.string.text_link_allowance_has_not_been_read_yet_63631ad), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        else -> Text(str(R.string.text_active_link_limit_2dcbf47) + allowed, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Button(onClick = { client.navigation.push(Route("file-request-compose")) }, enabled = allowed != 0uL, modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Outlined.Add, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_new_collection_link_c94a6dc))
                    }
                }
                ReadFeedback(capacity, client.cloudFiles::capacity)
                ReadFeedback(intents, client.fileRequests::loadIntents)
            }
        }
        val batches = intents.valueOrNull().orEmpty().filter { it.stage != "complete" }.groupBy { it.batch }.toList()
        items(batches, key = { "batch:" + it.first }) { (batch, jobs) ->
            Section {
                ListRow(
                    headline = jobs.first().title,
                    supporting = str(R.string.text_unfinished_creations_43bb280) + localizedNumber(jobs.size),
                    icon = Icons.Outlined.Pending,
                ) { client.navigation.push(Route("file-request-batch", id = batch)) }
            }
        }
        item { ReadFeedback(links, { client.fileRequests.browse() }) }
        items(links.valueOrNull().orEmpty(), key = { it.id }) { link ->
            Section {
                val sub = buildList {
                    link.label?.takeIf(String::isNotBlank)?.let(::add)
                    add(requestState(link.status) + " · " + quantity(R.plurals.files, link.files))
                }.joinToString("\n")
                ListRow(
                    headline = link.title ?: str(R.string.text_encrypted_request_acd01c8),
                    supporting = sub,
                    icon = Icons.Outlined.MoveToInbox,
                    trailing = if (link.unread) ({ Badge { Text(str(R.string.text_new_426a93d)) } }) else null,
                ) { client.navigation.select(link.id) }
            }
        }
        if (links is ReadState.Empty) item { EmptyState(str(R.string.text_no_collection_links_yet_d10e09a), icon = Icons.Outlined.MoveToInbox) }
    }
}

@Composable fun FileRequestComposerScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    val draft by client.fileRequests.draft.collectAsStateWithLifecycle()
    val capacity by client.cloudFiles.capacity.state.collectAsStateWithLifecycle()
    val working = state.working("requests:prepare")
    val limit = capacity.valueOrNull()
    LaunchedEffect(state.epoch) { client.cloudFiles.capacity() }
    DisposableEffect(Unit) { onDispose { client.fileRequests.leaveComposer() } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        DetailCard {
            OutlinedTextField(draft.title, { client.fileRequests.edit(draft.copy(title = it)) }, Modifier.fillMaxWidth().revealFocusedInput(),
                label = { Text(str(R.string.text_what_do_you_need_51c5d87)) }, enabled = !working, maxLines = 3)
            OutlinedTextField(draft.instructions, { client.fileRequests.edit(draft.copy(instructions = it)) }, Modifier.fillMaxWidth().revealFocusedInput(),
                label = { Text(str(R.string.text_instructions_optional_a2bd5d5)) }, enabled = !working, minLines = 2, maxLines = 6)
            Text(str(R.string.text_anyone_with_the_link_can_read_this_title_and_these_instructions_a_c5a2790),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            OutlinedTextField(draft.names, { client.fileRequests.edit(draft.copy(names = it)) }, Modifier.fillMaxWidth().revealFocusedInput(),
                label = { Text(str(R.string.text_who_is_it_for_optional_6027f5e)) }, enabled = !working, minLines = 2, maxLines = 8,
                supportingText = { Text(str(R.string.text_one_name_per_line_up_to_50_each_gets_an_independent_link_only_you_74cfa46)) })
        }
        DetailCard {
            Text(str(R.string.text_open_for_c0595c0), style = MaterialTheme.typography.labelLarge)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                listOf(7, 14, 30, 90).forEach { days -> FilterChip(draft.days == days, { client.fileRequests.edit(draft.copy(days = days)) }, { Text(quantity(R.plurals.days, days)) }, enabled = !working) }
            }
            Text(str(R.string.text_total_per_link_8166023), style = MaterialTheme.typography.labelLarge)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                FilterChip(draft.maxTotal == null, { client.fileRequests.edit(draft.copy(maxTotal = null)) }, { Text(str(R.string.text_plan_default_023d514)) }, enabled = !working)
                listOf(100uL, 500uL, 2048uL, 5120uL, 10240uL).map { it * 1024u * 1024u }.filter { limit?.storageLimit?.let { max -> it <= max } != false }.forEach { bytes ->
                    FilterChip(draft.maxTotal == bytes, { client.fileRequests.edit(draft.copy(maxTotal = bytes)) }, { Text(fileSize(bytes)) }, enabled = !working)
                }
            }
            limit?.fileLimit?.let {
                Text(str(R.string.text_current_single_file_limit_5febfdb) + fileSize(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Text(str(R.string.text_received_files_use_your_cloud_storage_the_service_checks_your_cur_480d5db),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        ReadFeedback(capacity, client.cloudFiles::capacity)
        if (!draft.valid && (draft.title.isNotBlank() || draft.names.isNotBlank())) {
            Text(str(R.string.text_use_a_title_up_to_512_bytes_instructions_up_to_4096_bytes_and_at_e7daa07),
                color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }
        Button(onClick = { client.fileRequests.create() }, enabled = draft.valid && !working && limit?.requestLimit != 0uL, modifier = Modifier.fillMaxWidth()) {
            Text(if (working) str(R.string.text_preparing_c7019a3) else if (draft.labels.size > 1) quantity(R.plurals.create_links, draft.labels.size) else str(R.string.text_create_link_2ae3b5b))
        }
    }
}

@Composable fun FileRequestBatchScreen(client: ClientController, batch: String) {
    val state by client.state.collectAsStateWithLifecycle()
    val intents by client.fileRequests.intents.state.collectAsStateWithLifecycle()
    val progress by client.fileRequests.publication.collectAsStateWithLifecycle()
    val jobs = intents.valueOrNull().orEmpty().filter { it.batch == batch }
    var discard by remember(batch) { mutableStateOf<String?>(null) }
    LaunchedEffect(batch, state.epoch) { client.fileRequests.loadIntents() }
    DisposableEffect(batch) { onDispose { if (client.fileRequests.publication.value?.batch == batch) client.fileRequests.pausePublication() } }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                jobs.firstOrNull()?.let { Text(it.title, style = MaterialTheme.typography.titleLarge) }
                if (jobs.isEmpty() && (intents is ReadState.Ready || intents is ReadState.Empty)) {
                    EmptyState(str(R.string.text_this_item_is_no_longer_available_refresh_the_list_23e6e8f), icon = Icons.Outlined.LinkOff) {
                        TextButton(onClick = client.fileRequests::loadIntents) { Text(str(R.string.text_refresh_cba212b)) }
                    }
                }
                ReadFeedback(intents, client.fileRequests::loadIntents)
                val running = progress?.takeIf { it.batch == batch }
                if (running != null) {
                    DetailCard {
                        LinearProgressIndicator(progress = { running.done.toFloat() / running.total.coerceAtLeast(1) }, modifier = Modifier.fillMaxWidth())
                        Text(str(R.string.links_created_progress, localizedNumber(running.done.toULong()), localizedNumber(running.total.toULong())),
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        OutlinedButton(onClick = { client.fileRequests.pausePublication() }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_pause_c65f066)) }
                    }
                } else if (jobs.any { it.stage in setOf("prepared", "checking") }) {
                    Notice(str(R.string.text_continue_to_check_or_publish_the_original_links_links_already_mad_3ebe5a7))
                    Button(onClick = { client.fileRequests.resumeBatch(batch) }, enabled = !state.working("requests:publish"), modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_continue_original_creations_4e7384c))
                    }
                }
            }
        }
        items(jobs, key = { it.id }) { job ->
            DetailCard {
                Text(job.label.ifBlank { str(R.string.text_collection_link_5c12570) }, style = MaterialTheme.typography.titleMedium)
                if (job.link != null) {
                    RequestLinkActions(client, job.link)
                    TextButton(onClick = { client.navigation.push(Route("file-requests", id = job.id)) }) { Text(str(R.string.text_view_received_files_c1b45ae)) }
                } else {
                    job.error?.let { Text(errorText(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error) }
                    Text(if (job.stage == "prepared") str(R.string.text_ready_to_publish_4f565e8) else str(R.string.text_waiting_to_confirm_the_original_result_fbe6478),
                        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    if (job.canDiscard) TextButton(onClick = { discard = job.id }, enabled = !state.working("requests:publish")) {
                        Text(str(R.string.text_discard_unpublished_draft_8dbb0b0), color = MaterialTheme.colorScheme.error)
                    }
                }
            }
        }
    }
    discard?.let { id -> AlertDialog(onDismissRequest = { discard = null }, title = { Text(str(R.string.text_discard_this_unpublished_draft_ee0769a)) },
        confirmButton = { TextButton(onClick = { client.fileRequests.discard(id); discard = null }) { Text(str(R.string.text_discard_475cd3e)) } }, dismissButton = { TextButton(onClick = { discard = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}

@Composable private fun RequestLinkActions(client: ClientController, link: FileRequest) {
    val context = LocalContext.current
    if (link.shareable && link.url != null) FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        TextButton(onClick = { if (link.shareable) { client.beginExternalInteraction(); context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, link.url) }, null)) } }) {
            Icon(Icons.Outlined.Share, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_share_link_b335ffa))
        }
        TextButton(onClick = {
            val now = System.currentTimeMillis().coerceAtLeast(0).toULong()
            if (link.shareable && now < link.expiresAt) client.clipboard.copy(link.url, (link.expiresAt - now).coerceAtMost(30_000u).toLong())
        }) { Icon(Icons.Outlined.ContentCopy, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_copy_link_8e702f5)) }
    } else if (link.shareable) Notice(str(R.string.text_this_device_does_not_yet_have_this_link_s_content_key_synchronize_2d2794e))
}

@Composable private fun FileRequestDetailScreen(client: ClientController, id: String, link: FileRequest?) {
    val state by client.state.collectAsStateWithLifecycle()
    val received by client.fileRequests.received.state.collectAsStateWithLifecycle()
    val saving by client.fileRequests.saving.collectAsStateWithLifecycle()
    var closing by remember(id) { mutableStateOf(false) }
    var deleting by remember(id) { mutableStateOf<ReceivedFile?>(null) }
    var choosing by remember(id) { mutableStateOf<ReceivedFile?>(null) }
    val files = received.valueOrNull()?.takeIf { it.request == id }?.files.orEmpty()
    LaunchedEffect(id, state.epoch, link?.url) { if (link != null) client.fileRequests.select(id) }
    DisposableEffect(id) { onDispose { client.fileRequests.leaveDetail() } }
    ScreenBar(str(R.string.text_collection_link_5c12570)) {
        IconButton(onClick = { client.fileRequests.select(id) }) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_received_files_ace2ac4))
        }
    }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                link?.let {
                    DetailCard {
                        it.title?.let { title -> Text(title, style = MaterialTheme.typography.titleLarge) }
                        it.label?.takeIf(String::isNotBlank)?.let { label -> Text(label, style = MaterialTheme.typography.titleMedium) }
                        Text(requestState(it.status), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                        LabelValue(str(R.string.text_expires_4972e43).trimEnd(' ', '·', ':'), messageTime(it.expiresAt))
                        LabelValue(str(R.string.text_total_per_link_8166023), fileSize(it.bytes) + " / " + fileSize(it.maxTotal))
                        it.instructions?.takeIf(String::isNotBlank)?.let { notes -> Text(notes, style = MaterialTheme.typography.bodyMedium) }
                        RequestLinkActions(client, it)
                    }
                    if (it.status != "revoked") Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                        ListRow(str(R.string.text_close_link_2384596), icon = Icons.Outlined.LinkOff, destructive = true, enabled = !state.working("requests:revoke:$id")) { closing = true }
                    }
                }
                SectionTitle(str(R.string.text_received_files_1a98b82))
                Text(str(R.string.text_sender_names_and_notes_are_unverified_file_contents_download_only_01b7ba6),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                ReadFeedback(received, { client.fileRequests.select(id) })
                if (received is ReadState.Empty) EmptyState(str(R.string.text_nothing_has_arrived_yet_bfc5dd1), icon = Icons.Outlined.MoveToInbox)
            }
        }
        items(files, key = { it.id }) { file ->
            DetailCard {
                Text(file.name ?: str(R.string.text_encrypted_file_2ff58b2), style = MaterialTheme.typography.titleMedium)
                Text(fileSize(file.size ?: file.bytes) + " · " + messageTime(file.storedAt), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                file.senderName?.takeIf(String::isNotBlank)?.let { Text(str(R.string.text_sender_unverified_2d47891) + it, style = MaterialTheme.typography.bodyMedium) }
                file.senderNote?.takeIf(String::isNotBlank)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                file.replaces?.let { original -> Text(str(R.string.text_replaces_71d4870) + (files.find { it.id == original }?.name ?: original), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                when (file.state) {
                    "unreadable" -> Text(str(R.string.text_this_file_cannot_be_opened_its_cloud_copy_can_be_deleted_16ab8e6), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                    "withdrawn" -> Text(str(R.string.text_withdrawn_by_the_sender_cad3248), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    "saved" -> Text(str(R.string.text_saved_to_your_library_c5ae61f), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                    "received" -> Text(str(R.string.text_in_cloud_not_saved_locally_55a9f3c), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    else -> Text(str(R.string.text_unknown_file_status_2b98225), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                }
                val progress = saving[file.id]
                if (progress != null) {
                    if (progress.total > 0u) LinearProgressIndicator(progress = { (progress.completed.toDouble() / progress.total.toDouble()).toFloat().coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                    else LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(str(R.string.text_downloading_and_verifying_keep_dmsg_open_the_file_is_saved_only_a_3156684), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton(onClick = { client.fileRequests.cancelSave(file.id) }) { Text(str(R.string.text_stop_saving_839873c)) }
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (file.canSave) TextButton(onClick = { choosing = file }, enabled = saving.isEmpty() && !state.working("requests:delete:${file.id}")) { Text(str(R.string.text_save_to_library_0ea6c15)) }
                    file.savedFile?.let { saved -> TextButton(onClick = { client.navigation.push(Route("files", id = saved)) }) { Text(str(R.string.text_show_in_library_ea56489)) } }
                    if (file.canDelete) TextButton(onClick = { deleting = file }, enabled = progress == null && !state.working("requests:delete:${file.id}")) { Text(str(R.string.text_delete_cloud_copy_2cefb18), color = MaterialTheme.colorScheme.error) }
                }
            }
        }
    }
    if (closing) AlertDialog(onDismissRequest = { closing = false }, title = { Text(str(R.string.text_close_this_link_304dbda)) }, text = { Text(str(R.string.text_it_will_stop_accepting_uploads_files_already_received_stay_until_eafd93b)) }, confirmButton = { TextButton(onClick = { client.fileRequests.revoke(id); closing = false }) { Text(str(R.string.text_close_link_2384596)) } }, dismissButton = { TextButton(onClick = { closing = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    deleting?.let { file -> AlertDialog(onDismissRequest = { deleting = null }, title = { Text(str(R.string.text_delete_this_cloud_copy_79eadc7)) }, text = { Text(str(R.string.text_the_received_cloud_copy_will_be_deleted_a_copy_already_saved_in_y_0a229b8)) }, confirmButton = { TextButton(onClick = { client.fileRequests.delete(file); deleting = null }) { Text(str(R.string.text_delete_5b87532)) } }, dismissButton = { TextButton(onClick = { deleting = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
    choosing?.let { file ->
        FileFolderDialog(client, str(R.string.text_save_into_folder_dd2484b), { choosing = null }, action = str(R.string.text_save_here_ac62376), review = {
            Text(str(R.string.text_choose_a_folder_or_use_received_files_title_name_4a3845e))
            TextButton(onClick = { client.fileRequests.save(file, null, str(R.string.text_received_files_1a98b82)); choosing = null }) { Text(str(R.string.text_use_organized_received_folder_a524895)) }
        }) { path -> client.fileRequests.save(file, path.folderId, str(R.string.text_received_files_1a98b82)); choosing = null }
    }
}

internal fun requestState(state: String) = when (state) {
    "open" -> str(R.string.text_accepting_files_4bcf6d4)
    "full" -> str(R.string.text_total_limit_reached_e258a80)
    "paused" -> str(R.string.text_paused_cf54b60)
    "expired" -> str(R.string.text_expired_70446d7)
    "revoked" -> str(R.string.text_link_closed_89076c4)
    else -> str(R.string.text_unknown_status_310134f) + state
}
