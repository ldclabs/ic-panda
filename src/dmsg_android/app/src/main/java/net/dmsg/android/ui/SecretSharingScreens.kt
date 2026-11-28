package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import android.content.Intent
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

private fun packageFieldTitle(field: PackageField): String = field.label.ifBlank {
    field.role?.let(::vaultFieldName) ?: fieldKindName(field.kind)
}

@Composable fun SecretSnapshotComposer(client: ClientController, id: String, initialChannel: String?) {
    val source by client.secretSharing.source.state.collectAsStateWithLifecycle()
    val current by client.secretSharing.draft.collectAsStateWithLifecycle()
    val channels by client.messages.channels.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val draft = current?.takeIf { it.source.item.id == id }
    var destination by remember { mutableStateOf(false) }
    var discard by remember { mutableStateOf(false) }
    var limitsOpen by remember(id) { mutableStateOf(false) }
    LaunchedEffect(id) { client.secretSharing.composer(id, initialChannel) }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        if (draft == null) ReadFeedback(source) { client.secretSharing.composer(id, initialChannel) }
        draft?.let { value ->
            val item = value.source.item
            val working = state.working("secret:prepare:" + value.id)
            val enabled = !working && value.prepared == null && !value.uncertain
            if (value.uncertain) Notice(str(R.string.text_preparation_did_not_return_a_confirmed_result_check_the_saved_sna_01a2b53), true)
            DetailCard {
                Text(item.title, style = MaterialTheme.typography.titleLarge)
                Text(str(R.string.text_choose_the_exact_content_of_this_independent_snapshot_editing_the_a26f334),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Box(Modifier.fillMaxWidth()) {
                    OutlinedButton(onClick = { destination = true }, enabled = enabled, modifier = Modifier.fillMaxWidth()) {
                        Icon(if (value.channel == null) Icons.Outlined.Link else Icons.Outlined.Forum, null)
                        Spacer(Modifier.width(8.dp))
                        Text(value.channel?.let { channelId -> channels.valueOrNull()?.find { it.id == channelId }?.name ?: channelId } ?: str(R.string.text_share_link_b335ffa))
                        Spacer(Modifier.width(4.dp))
                        Icon(Icons.Outlined.ArrowDropDown, null)
                    }
                    DropdownMenu(destination, { destination = false }) {
                        DropdownMenuItem(
                            text = { Text(str(R.string.text_share_link_b335ffa)) },
                            leadingIcon = { Icon(Icons.Outlined.Link, null) },
                            onClick = { destination = false; client.secretSharing.edit { it.copy(channel = null) } },
                        )
                        channels.valueOrNull().orEmpty().filter { it.canPublish }.forEach { channel ->
                            DropdownMenuItem(
                                text = { Text(channel.name) },
                                leadingIcon = { Icon(Icons.Outlined.Forum, null) },
                                onClick = { destination = false; client.secretSharing.edit { it.copy(channel = channel.id) } },
                            )
                        }
                    }
                }
                if (value.channel != null) Text(str(R.string.text_members_with_access_to_this_channel_epoch_including_later_history_a29d046),
                    style = MaterialTheme.typography.bodyMedium)
            }
            val selectableFields = item.fields.filter { it.value.isNotEmpty() }
            if (selectableFields.isNotEmpty() || item.notes.isNotEmpty()) {
                val sharesOtp = selectableFields.any { it.kind == "otp" && it.id in value.fields }
                Section(
                    title = str(R.string.text_select_fields_to_share_290fe99),
                    footer = if (sharesOtp) str(R.string.text_selected_otp_fields_include_the_reusable_seed_not_just_a_current_4a6ddda) else null,
                ) {
                    selectableFields.forEach { field ->
                        SelectionRow(vaultFieldTitle(field.id, field.kind, field.label), field.id in value.fields, enabled, field.section.takeIf(String::isNotEmpty)) { checked ->
                            client.secretSharing.edit { it.copy(fields = if (checked) it.fields + field.id else it.fields - field.id) }
                        }
                    }
                    if (item.notes.isNotEmpty()) SelectionRow(str(R.string.text_include_notes_bab8a75), value.notes, enabled) { checked ->
                        client.secretSharing.edit { it.copy(notes = checked) }
                    }
                }
            }
            if (value.source.attachments.files.isNotEmpty()) {
                Section(title = str(R.string.text_select_attachments_to_share_2dbaa0d)) {
                    value.source.attachments.files.forEach { entry ->
                        SelectionRow(entry.details?.file?.name ?: str(R.string.text_attachment_unavailable_on_this_device_ede9476), entry.id in value.attachments, enabled && entry.selectable,
                            entry.details?.file?.let { fileSize(it.size) + " · " + it.version } ?: entry.id) { checked ->
                            client.secretSharing.edit { it.copy(attachments = if (checked) it.attachments + entry.id else it.attachments - entry.id) }
                        }
                    }
                }
            }
            if (value.channel == null) {
                val durations = listOf(3_600_000uL to str(R.string.text_1_hour_0ac0bd6), 86_400_000uL to str(R.string.text_1_day_2c85bc8), 604_800_000uL to str(R.string.text_7_days_bd55de3))
                Section {
                    ListRow(
                        headline = str(R.string.text_link_limits_653a1af),
                        supporting = durations.first { it.first == value.lifetime }.second + " · " + str(R.string.text_downloads_35a65b7) + " " + value.sessions,
                        icon = Icons.Outlined.Tune,
                        trailing = { Icon(if (limitsOpen) Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore, null) },
                    ) { limitsOpen = !limitsOpen }
                }
                if (limitsOpen) {
                    DetailCard {
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            durations.forEach { (duration, title) ->
                                FilterChip(value.lifetime == duration, { client.secretSharing.edit { it.copy(lifetime = duration) } }, { Text(title) }, enabled = enabled)
                            }
                        }
                        OutlinedTextField(value.sessions, { input -> client.secretSharing.edit { it.copy(sessions = input.take(3)) } }, Modifier.fillMaxWidth(), enabled = enabled,
                            label = { Text(str(R.string.text_download_sessions_1_100_1169f0a)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), singleLine = true,
                            isError = value.sessions.toIntOrNull() !in 1..100)
                        SecretTextField(value.password, { password -> client.secretSharing.edit { it.copy(password = password) } }, str(R.string.text_optional_password_cc1611b), enabled = enabled)
                        Text(str(R.string.text_a_download_session_is_a_retrieval_allowance_not_a_count_of_file_r_2ef4376),
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
            Text(str(R.string.text_deliver_an_independent_snapshot_of_this_exact_selection_9c08d03),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Button(onClick = client.secretSharing::prepare, enabled = !working && (value.prepared != null || value.uncertain || value.selected && (value.channel != null || value.sessions.toIntOrNull() in 1..100)), modifier = Modifier.fillMaxWidth()) {
                Text(when {
                    value.prepared != null -> str(R.string.text_open_prepared_snapshot_4dea4bc)
                    value.uncertain -> str(R.string.text_check_saved_snapshots_b7fef78)
                    value.channel != null -> str(R.string.text_send_06acd73)
                    else -> str(R.string.text_create_link_2ae3b5b)
                })
            }
            if (working) {
                LinearProgressIndicator(Modifier.fillMaxWidth())
                TextButton(onClick = { client.cancelOperation("secret:prepare:" + value.id) }) { Text(str(R.string.text_cancel_preparation_d0c1253)) }
            }
            TextButton(onClick = { discard = true }, enabled = !working) {
                Text(str(R.string.text_discard_form_9141552), color = MaterialTheme.colorScheme.error)
            }
        }
    }
    if (discard) AlertDialog(onDismissRequest = { discard = false }, title = { Text(str(R.string.text_discard_this_form_1a5950c)) },
        text = { Text(str(R.string.text_any_snapshot_already_prepared_stays_in_shared_snapshots_until_you_b89ed23)) },
        confirmButton = { TextButton(onClick = { discard = false; client.secretSharing.discardDraft(); client.navigation.back() }) { Text(str(R.string.text_discard_form_9141552)) } },
        dismissButton = { TextButton(onClick = { discard = false }) { Text(str(R.string.text_keep_form_8789c0c)) } })
}

@Composable fun SecretSharesScreen(client: ClientController) {
    val read by client.secretSharing.jobs.state.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { client.secretSharing.list() }
    ScreenBar(null) {
        IconButton(onClick = client.secretSharing::list) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_snapshots_9680f9d))
        }
    }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Section(footer = str(R.string.text_snapshot_jobs_created_on_this_device_each_snapshot_retains_its_ow_5ffff05)) {
                ListRow(str(R.string.text_share_links_fe69aa5), icon = Icons.Outlined.Link) { client.navigation.push(Route("vault-links")) }
            }
        }
        item { ReadFeedback(read, client.secretSharing::list) }
        items(read.valueOrNull().orEmpty(), key = { it.id }) { job ->
            Section {
                ListRow(
                    headline = job.title,
                    supporting = secretStateName(job.state) + " · " + fileSize(job.bytes),
                    icon = if (job.channel == null) Icons.Outlined.Link else Icons.Outlined.Forum,
                ) { client.navigation.push(Route("secret-share", job.id)) }
            }
        }
        if (read is ReadState.Empty) item { EmptyState(str(R.string.text_no_snapshots_on_this_device_1710ceb), glyph = R.drawable.glyph_link) }
    }
}

@Composable fun SecretShareScreen(client: ClientController, id: String) {
    val read by client.secretSharing.job.state.collectAsStateWithLifecycle()
    val visibleLink by client.secretSharing.link.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val job = read.valueOrNull()?.takeIf { it.id == id }
    val link = visibleLink?.takeIf { it.first == id }?.second
    val context = LocalContext.current
    var revoke by remember(id) { mutableStateOf(false) }
    LaunchedEffect(id) { client.secretSharing.select(id) }
    DisposableEffect(id) { onDispose { client.secretSharing.hideLink(id) } }
    val working = state.working("secret:reconcile:" + id) || state.working("secret:revoke:" + id) || state.working("secret:publish:" + id)
    ScreenBar(null) {
        IconButton(onClick = { client.secretSharing.select(id) }, enabled = !working) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_original_snapshot_e83960f))
        }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(read) { client.secretSharing.select(id) }
        job?.let { value ->
            when {
                value.error != null -> Notice(errorText(value.error), true)
                value.state in setOf("unknown", "publishing", "uploading", "encrypting", "revoke_pending") -> Notice(str(R.string.text_the_outcome_is_uncertain_check_this_original_snapshot_before_retr_83e738c))
            }
            DetailCard {
                Text(value.title, style = MaterialTheme.typography.titleLarge)
                Text(secretStateName(value.state), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                LabelValue(str(R.string.text_package_size_04b1f98).trimEnd(' ', '·', ':'), fileSize(value.bytes))
                if (value.chunks > 0u) {
                    Progress((value.uploaded.toDouble() / value.chunks.toDouble()).toFloat().coerceIn(0f, 1f))
                    Text(localizedNumber(value.uploaded) + " / " + localizedNumber(value.chunks), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (working) LinearProgressIndicator(Modifier.fillMaxWidth())
            }
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                if (value.state in setOf("prepared", "needs_approval", "awaiting_user", "interrupted", "failed") && !value.cancelRequested) {
                    Button(onClick = { client.secretSharing.publish(id) }, enabled = !working && read is ReadState.Ready, modifier = Modifier.fillMaxWidth()) {
                        Text(str(if (value.state == "failed") R.string.text_retry_563171c else R.string.text_publish_88bb01f))
                    }
                }
                if (value.state in setOf("unknown", "publishing", "uploading", "encrypting", "revoke_pending")) {
                    Button(onClick = { client.secretSharing.reconcile(id) }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_reconcile_original_snapshot_88bff05))
                    }
                }
                if (value.channel == null && value.nearby == null && value.state == "published" && !value.cancelRequested) {
                    OutlinedButton(onClick = { client.secretSharing.select(id, reveal = true) }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Outlined.Link, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_show_share_link_fb39fbb))
                    }
                }
            }
            if (link != null) {
                DetailCard {
                    Text(str(R.string.text_share_link_b335ffa), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                    SelectionContainer { Text(link, style = MaterialTheme.typography.bodySmall.code()) }
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        CopyButton(str(R.string.text_copy_link_8e702f5), key = link) { client.clipboard.copy(link) }
                        TextButton(onClick = { client.beginExternalInteraction(); context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, link), null)) }) {
                            Icon(Icons.Outlined.Share, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_share_link_b335ffa))
                        }
                        TextButton(onClick = { client.secretSharing.hideLink(id) }) { Text(str(R.string.text_hide_d2e660d)) }
                    }
                    Text(str(R.string.text_anyone_with_the_complete_link_and_required_password_can_retrieve_5d7f03c),
                        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            Section(
                footer = if (value.channel != null) str(R.string.text_members_with_access_to_this_channel_epoch_including_later_history_a29d046) else null,
            ) {
                if (value.channel != null) {
                    InfoRow(str(R.string.text_destination_channel_d035e1e), value.channel.name)
                    CodeRow(str(R.string.text_channel_0126d4b).trimEnd(' ', '·', ':'), value.channel.id) { client.clipboard.copy(value.channel.id) }
                } else if (value.nearby != null) {
                    InfoRow(str(R.string.nearby_device), value.nearby.peerLabel)
                    InfoRow(str(R.string.nearby_verification_code), groupedNearbySas(value.nearby.sas))
                } else {
                    value.expiresAt?.let { LabelValue(str(R.string.text_expires_1efbbe3).trimEnd(' ', '·', ':'), displayTime(it)) }
                    value.maxSessions?.let { LabelValue(str(R.string.text_download_sessions_3c6ad5d).trimEnd(' ', '·', ':'), localizedNumber(it)) }
                    if (value.password == true) InfoRow(str(R.string.text_password_347d7fc), str(R.string.text_password_protected_7587e45))
                }
            }
            if (value.channel == null && value.nearby == null && value.state !in setOf("revoked", "cancelled")) {
                Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                    ListRow(str(R.string.text_revoke_or_cancel_this_link_372f372), icon = Icons.Outlined.LinkOff, destructive = true, enabled = !working) { revoke = true }
                }
            }
        }
    }
    if (revoke) AlertDialog(onDismissRequest = { revoke = false }, title = { Text(str(R.string.text_revoke_this_snapshot_link_f42190b)) },
        text = { Text(str(R.string.text_this_requests_an_end_to_future_retrieval_saved_copies_remain_usab_4d07816)) },
        confirmButton = { TextButton(onClick = { revoke = false; client.secretSharing.revoke(id) }) { Text(str(R.string.text_revoke_or_cancel_0e052a1)) } },
        dismissButton = { TextButton(onClick = { revoke = false }) { Text(str(R.string.text_go_back_7741045)) } })
}

@Composable fun SecretPackageScreen(client: ClientController, channel: String?, sequence: ULong?, nearbyDigest: String? = null) {
    val read by client.secretSharing.preview.state.collectAsStateWithLifecycle()
    val form by client.secretSharing.incoming.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    val source = PackageInput(channel, sequence, nearbyDigest)
    val current = form.takeIf { it.source == source }
    val preview = read.valueOrNull()?.takeIf { current != null }
    var choosingFolder by remember { mutableStateOf(false) }
    val working = state.working("secret:preview") || state.working("secret:import")
    LaunchedEffect(source) { client.secretSharing.receiver(source) }
    // A package the device refuses to open (wrong password, altered or expired) answers with the Reject haptic.
    val haptics = LocalHapticFeedback.current
    val refused = read is ReadState.Failed<*>
    val wasRefused = remember { booleanArrayOf(refused) }
    LaunchedEffect(refused) { if (refused && !wasRefused[0]) haptics.performHapticFeedback(HapticFeedbackType.Reject); wasRefused[0] = refused }
    LaunchedEffect(qr.result) { client.qr.consume("secret-package")?.let { link -> client.secretSharing.editIncoming { it.copy(link = link) } } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        if (current == null) return@Column
        val editable = !working && current.completed == null && current.pending == null
        if (current.pending != null) Notice(str(R.string.text_the_original_selection_is_fixed_continue_that_request_to_learn_it_480f539))
        if (source.nearby == null) DetailCard {
            if (source.channel == null) {
                OutlinedTextField(current.link, { link -> client.secretSharing.editIncoming { it.copy(link = link.take(8192)) } }, Modifier.fillMaxWidth(), enabled = editable,
                    label = { Text(str(R.string.text_paste_secret_package_link_64af151)) }, minLines = 2, maxLines = 4, keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri))
                OutlinedButton(onClick = { client.qr.open("secret-package") }, enabled = editable, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.QrCodeScanner, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_scan_a_package_link_6937834))
                }
                SecretTextField(current.password, { password -> client.secretSharing.editIncoming { it.copy(password = password) } }, str(R.string.text_password_if_required_313b93c), enabled = editable)
                Text(str(R.string.text_preview_downloads_and_verifies_the_package_on_this_device_and_use_ab1656c),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                Text(str(R.string.text_preview_verifies_the_sender_channel_epoch_and_original_message_be_3e32cf2),
                    style = MaterialTheme.typography.bodyMedium)
            }
            if (current.pending == null && current.completed == null) {
                Button(onClick = client.secretSharing::inspect, enabled = !working && (channel != null && sequence != null || current.link.isNotBlank()), modifier = Modifier.fillMaxWidth()) {
                    Text(if (preview == null) str(R.string.text_preview_package_7f98c9f) else str(R.string.text_refresh_verified_preview_5d8e380))
                }
            }
            if (working) {
                LinearProgressIndicator(Modifier.fillMaxWidth())
                if (state.working("secret:preview")) TextButton(onClick = { client.cancelOperation("secret:preview") }) { Text(str(R.string.text_cancel_preview_a89c579)) }
            }
        } else if (preview != null) {
            Notice(str(R.string.nearby_vault_verified_hint))
        }
        if (read !is ReadState.Idle) ReadFeedback(read, client.secretSharing::inspect)
        preview?.let { value ->
            DetailCard {
                Text(value.title, style = MaterialTheme.typography.titleLarge)
                Text(displayTime(value.createdAt), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (value.imported.isNotEmpty()) Text(str(R.string.text_this_package_was_already_imported_here_existing_copies_remain_ind_54592ed),
                    style = MaterialTheme.typography.bodyMedium)
            }
            value.channel?.let { origin ->
                Section {
                    CodeRow(str(R.string.text_sender_account_e9f6961), origin.sender) { client.clipboard.copy(origin.sender) }
                    CodeRow(str(R.string.text_channel_0126d4b).trimEnd(' ', '·', ':'), origin.id) { client.clipboard.copy(origin.id) }
                }
            }
            if (value.fields.isNotEmpty() || value.notes) {
                val importsOtp = value.fields.any { it.kind == "otp" && it.id in current.fields }
                Section(
                    title = str(R.string.text_select_fields_to_import_97581a6),
                    footer = if (importsOtp) str(R.string.text_selected_otp_fields_include_the_reusable_seed_not_just_a_current_4a6ddda) else null,
                ) {
                    value.fields.forEach { field ->
                        SelectionRow(packageFieldTitle(field), field.id in current.fields, editable, field.section) { checked ->
                            client.secretSharing.editIncoming { it.copy(fields = if (checked) it.fields + field.id else it.fields - field.id) }
                        }
                    }
                    if (value.notes) SelectionRow(str(R.string.text_notes_f7df47b), current.notes, editable) { checked ->
                        client.secretSharing.editIncoming { it.copy(notes = checked) }
                    }
                }
            }
            if (value.attachments.isNotEmpty()) {
                Section(
                    title = str(R.string.text_select_attachments_to_import_de0f7db),
                    footer = str(R.string.text_imported_attachments_stay_on_this_device_and_inherit_this_folder_d1a4fc8),
                ) {
                    value.attachments.forEachIndexed { index, file ->
                        SelectionRow(file.name, index.toUInt() in current.attachments, editable, fileSize(file.size)) { checked ->
                            client.secretSharing.editIncoming { it.copy(attachments = if (checked) it.attachments + index.toUInt() else it.attachments - index.toUInt()) }
                        }
                    }
                    ListRow(
                        headline = str(R.string.text_destination_843a557).trimEnd(' ', '·', ':'),
                        supporting = current.folderName.ifBlank { str(R.string.text_files_c79ddc2) },
                        icon = Icons.Outlined.Folder,
                        enabled = editable,
                    ) { choosingFolder = true }
                }
            } else {
                Text(str(R.string.text_create_an_independent_copy_of_this_exact_selection_a20b10b),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Section {
                CodeRow(str(R.string.text_verified_package_fingerprint_c59476c), value.digest) { client.clipboard.copy(value.digest) }
            }
        }
        if (current.completed == null) {
            Button(onClick = client.secretSharing::importPackage,
                enabled = !working && (current.pending != null || preview != null && current.selected),
                modifier = Modifier.fillMaxWidth()) {
                Text(if (current.pending != null) str(R.string.text_retry_original_import_c3a9967) else str(R.string.text_import_1a200b3))
            }
        }
        current.completed?.let { ids ->
            Section(title = str(R.string.text_imported_copies_3c1c692)) {
                ids.forEach { itemId ->
                    ListRow(itemId, icon = Icons.Outlined.Key) { client.navigation.push(Route("vault", itemId)) }
                }
            }
        }
        Text(str(R.string.text_mobile_packages_are_limited_to_8_mib_preview_verifies_content_bef_7a4533d),
            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    if (choosingFolder) FileFolderDialog(client, str(R.string.text_files_c79ddc2), { choosingFolder = false }) { path ->
        client.secretSharing.editIncoming { it.copy(folder = path.folderId, folderName = path.lastOrNull()?.name ?: str(R.string.text_files_c79ddc2)) }
        choosingFolder = false
    }
}

@Composable fun SecretSnapshotPicker(client: ClientController, channel: String) {
    val picker = client.secretSharing.picker
    val read by picker.listing.state.collectAsStateWithLifecycle()
    val query by picker.filters.collectAsStateWithLifecycle()
    LaunchedEffect(channel) { picker.browse() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                WorkspaceSearchField(query.text, { picker.browse(VaultQuery(text = it)) }, str(R.string.text_search_vault_f6e42b2))
                ReadFeedback(read) { picker.browse() }
                if (read is ReadState.Empty) EmptyState(str(if (query.text.isBlank()) R.string.autofill_no_items else R.string.text_no_matching_items_b35c7c3))
            }
        }
        items(read.valueOrNull()?.items.orEmpty(), key = { it.id }) { item ->
            BrowseRow(
                title = item.title,
                modifier = Modifier.clickable(enabled = !item.reserved && !item.deleted && !item.conflict) {
                    client.navigation.push(Route("secret-compose", item.id, channel = channel))
                },
                leading = { SecretIcon(item.kind) },
            ) { Text(vaultKindName(item.kind)) }
        }
        if (read.valueOrNull()?.hasMore == true) item {
            OutlinedButton(onClick = { picker.browse(more = true) }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_load_more_4e1d9f8)) }
        }
    }
}

@Composable private fun OtpSnapshotNotice() = Notice(str(R.string.text_selected_otp_fields_include_the_reusable_seed_not_just_a_current_4a6ddda))
@Composable private fun ChannelSnapshotNotice() = Notice(str(R.string.text_members_with_access_to_this_channel_epoch_including_later_history_a29d046))

@Composable fun SecretApprovalContent(review: SecretReview) {
    DetailCard {
        Text(review.title, style = MaterialTheme.typography.titleLarge)
        Text(if (review.importing) str(R.string.text_create_an_independent_copy_of_this_exact_selection_a20b10b) else str(R.string.text_deliver_an_independent_snapshot_of_this_exact_selection_9c08d03),
            style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    Section(title = str(R.string.text_selected_fields_cf5d199)) {
        if (review.fields.isEmpty()) InfoRow(str(R.string.text_selected_fields_cf5d199), str(R.string.text_no_fields_selected_6b0570a))
        review.fields.forEach { InfoRow(fieldKindName(it.kind), if (review.importing) packageFieldTitle(it) else vaultFieldTitle(it.id, it.kind, it.label)) }
        InfoRow(str(R.string.text_notes_f7df47b), if (review.notes) str(R.string.text_notes_included_0a3f128) else str(R.string.text_notes_excluded_cfd2b9b))
    }
    if (review.fields.any { it.kind == "otp" }) OtpSnapshotNotice()
    Section(title = str(R.string.text_selected_attachments_4c5e318)) {
        if (review.files.isEmpty()) InfoRow(str(R.string.text_attachments_be5733e), str(R.string.text_no_attachments_selected_8b05287))
        review.files.forEach { file ->
            ListItem(
                headlineContent = { Text(file.name) },
                supportingContent = {
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(fileSize(file.size) + " · " + file.mime, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        SelectionContainer { Text(file.sha256, style = MaterialTheme.typography.bodySmall.code()) }
                    }
                },
                leadingContent = { RowIcon(Icons.Outlined.AttachFile) },
            )
        }
    }
    if (review.importing) {
        Section(footer = str(R.string.text_imported_attachments_stay_on_this_device_and_inherit_this_folder_d1a4fc8)) {
            InfoRow(str(R.string.text_destination_folder_2f893c8).trimEnd(' ', '·', ':'), review.destination.orEmpty())
        }
    } else if (review.channel != null) {
        Section(title = str(R.string.text_destination_channel_d035e1e)) {
            InfoRow(str(R.string.text_name_dd4dc4c), review.channel.name)
            CodeRow(str(R.string.text_channel_0126d4b).trimEnd(' ', '·', ':'), review.channel.id)
            LabelValue(str(R.string.text_epoch_4202c18).trimEnd(' ', '·', ':'), "${review.channel.epoch} / ${review.channel.aclVersion}")
        }
        ChannelSnapshotNotice()
    } else if (review.nearby != null) {
        Section(
            title = str(R.string.nearby_snapshot),
            footer = str(R.string.nearby_vault_approval_hint),
        ) {
            InfoRow(str(R.string.nearby_device), review.nearby.peerLabel)
            InfoRow(str(R.string.nearby_verification_code), groupedNearbySas(review.nearby.sas))
            CodeRow(str(R.string.text_verified_package_fingerprint_c59476c), review.nearby.transcript)
        }
    } else {
        Section(
            title = str(R.string.text_share_link_b335ffa),
            footer = str(R.string.text_a_full_link_and_required_password_grant_retrieval_within_these_li_323a1eb),
        ) {
            review.expiry?.let { LabelValue(str(R.string.text_expires_1efbbe3).trimEnd(' ', '·', ':'), displayTime(it)) }
            review.sessions?.let { LabelValue(str(R.string.text_download_sessions_3c6ad5d).trimEnd(' ', '·', ':'), localizedNumber(it)) }
            InfoRow(str(R.string.text_password_347d7fc), if (review.password == true) str(R.string.text_password_protected_7587e45) else str(R.string.text_no_additional_password_d56364a))
        }
    }
    Section(title = if (review.importing) str(R.string.text_verified_package_e203c4b) else str(R.string.text_fixed_source_revision_65e4963)) {
        CodeRow(str(R.string.text_source_fcdba0f), review.source)
        CodeRow(str(R.string.text_fixed_source_revision_65e4963), review.revision)
    }
}
