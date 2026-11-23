package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import android.content.Intent
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
import androidx.compose.ui.text.input.PasswordVisualTransformation
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
            Text(item.title, style = MaterialTheme.typography.headlineSmall)
            Notice(str(R.string.text_choose_the_exact_content_of_this_independent_snapshot_editing_the_a26f334))
            Box {
                OutlinedButton(onClick = { destination = true }, enabled = enabled) { Text(value.channel?.let { id -> channels.valueOrNull()?.find { it.id == id }?.name ?: id } ?: str(R.string.text_share_link_b335ffa)); Icon(Icons.Outlined.ArrowDropDown, null) }
                DropdownMenu(destination, { destination = false }) {
                    DropdownMenuItem(text = { Text(str(R.string.text_share_link_b335ffa)) }, onClick = { destination = false; client.secretSharing.edit { it.copy(channel = null) } })
                    channels.valueOrNull().orEmpty().filter { it.canPublish }.forEach { channel ->
                        DropdownMenuItem(text = { Text(channel.name) }, onClick = { destination = false; client.secretSharing.edit { it.copy(channel = channel.id) } })
                    }
                }
            }
            SectionTitle(str(R.string.text_select_fields_to_share_290fe99))
            item.fields.filter { it.value.isNotEmpty() }.forEach { field -> SelectionRow(vaultFieldTitle(field.id, field.kind, field.label), field.id in value.fields, enabled, field.section.takeIf(String::isNotEmpty)) { checked ->
                client.secretSharing.edit { it.copy(fields = if (checked) it.fields + field.id else it.fields - field.id) }
            } }
            if (item.notes.isNotEmpty()) SelectionRow(str(R.string.text_include_notes_bab8a75), value.notes, enabled) { checked -> client.secretSharing.edit { it.copy(notes = checked) } }
            if (item.fields.any { it.id in value.fields && it.kind == "otp" }) OtpSnapshotNotice()
            if (value.source.attachments.files.isNotEmpty()) {
                SectionTitle(str(R.string.text_select_attachments_to_share_2dbaa0d))
                value.source.attachments.files.forEach { entry ->
                    SelectionRow(entry.details?.file?.name ?: str(R.string.text_attachment_unavailable_on_this_device_ede9476), entry.id in value.attachments, enabled && entry.selectable,
                        entry.details?.file?.let { fileSize(it.size) + " · " + it.version } ?: entry.id) { checked -> client.secretSharing.edit { it.copy(attachments = if (checked) it.attachments + entry.id else it.attachments - entry.id) } }
                }
            }
            if (value.channel == null) {
                val durations = listOf(3_600_000uL to str(R.string.text_1_hour_0ac0bd6), 86_400_000uL to str(R.string.text_1_day_2c85bc8), 604_800_000uL to str(R.string.text_7_days_bd55de3))
                TextButton(onClick = { limitsOpen = !limitsOpen }) {
                    Text(str(R.string.text_link_limits_653a1af) + " · " + durations.first { it.first == value.lifetime }.second + " · " + str(R.string.text_downloads_35a65b7) + " " + value.sessions)
                    Icon(if (limitsOpen) Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore, null)
                }
                if (limitsOpen) {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        durations.forEach { (duration, title) ->
                            FilterChip(value.lifetime == duration, { client.secretSharing.edit { it.copy(lifetime = duration) } }, { Text(title) }, enabled = enabled)
                        }
                    }
                    OutlinedTextField(value.sessions, { input -> client.secretSharing.edit { it.copy(sessions = input.take(3)) } }, Modifier.fillMaxWidth(), enabled = enabled,
                        label = { Text(str(R.string.text_download_sessions_1_100_1169f0a)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), singleLine = true,
                        isError = value.sessions.toIntOrNull() !in 1..100)
                    OutlinedTextField(value.password, { password -> client.secretSharing.edit { it.copy(password = password) } }, Modifier.fillMaxWidth(), enabled = enabled,
                        label = { Text(str(R.string.text_optional_password_cc1611b)) }, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Password))
                    Text(str(R.string.text_a_download_session_is_a_retrieval_allowance_not_a_count_of_file_r_2ef4376), style = MaterialTheme.typography.bodySmall)
                }
            } else ChannelSnapshotNotice()
            Notice(str(R.string.text_a_mobile_package_is_limited_to_8_mib_including_attachments_prepar_74b5ba7))
            if (value.uncertain) Notice(str(R.string.text_preparation_did_not_return_a_confirmed_result_check_the_saved_sna_01a2b53), true)
            Button(onClick = client.secretSharing::prepare, enabled = !working && (value.prepared != null || value.uncertain || value.selected && (value.channel != null || value.sessions.toIntOrNull() in 1..100)), modifier = Modifier.fillMaxWidth()) {
                Text(when { value.prepared != null -> str(R.string.text_open_prepared_snapshot_4dea4bc); value.uncertain -> str(R.string.text_check_saved_snapshots_b7fef78); else -> str(R.string.text_review_request_cb6fb73) })
            }
            if (working) { LinearProgressIndicator(Modifier.fillMaxWidth()); TextButton(onClick = { client.cancelOperation("secret:prepare:" + value.id) }) { Text(str(R.string.text_cancel_preparation_d0c1253)) } }
            TextButton(onClick = { discard = true }, enabled = !working) { Text(str(R.string.text_discard_form_9141552)) }
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
    LazyColumn(contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item { TextButton(onClick = { client.navigation.push(Route("vault-links")) }) { Text(str(R.string.text_share_links_fe69aa5)) } }
        item { Notice(str(R.string.text_snapshot_jobs_created_on_this_device_each_snapshot_retains_its_ow_5ffff05)); ReadFeedback(read, client.secretSharing::list) }
        items(read.valueOrNull().orEmpty(), key = { it.id }) { job ->
            ListItem(modifier = Modifier.clickable { client.navigation.push(Route("secret-share", job.id)) }, headlineContent = { Text(job.title) },
                supportingContent = { Text(secretStateName(job.state) + " · " + fileSize(job.bytes)) }, leadingContent = { Icon(if (job.channel == null) Icons.Outlined.Link else Icons.Outlined.Forum, null) })
        }
        if (read is ReadState.Empty) item { EmptyPanel(str(R.string.text_no_snapshots_on_this_device_1710ceb)) }
        item { TextButton(onClick = client.secretSharing::list) { Text(str(R.string.text_refresh_snapshots_9680f9d)) } }
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
    val working = state.working("secret:reconcile:" + id) || state.working("secret:revoke:" + id) || state.working("approval:request:secret-share:" + id)
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(read) { client.secretSharing.select(id) }
        job?.let { value ->
            Text(value.title, style = MaterialTheme.typography.headlineSmall)
            Text(secretStateName(value.state), color = MaterialTheme.colorScheme.primary)
            Text(str(R.string.text_package_size_04b1f98) + fileSize(value.bytes))
            if (value.chunks > 0u) { LinearProgressIndicator(progress = { (value.uploaded.toDouble() / value.chunks.toDouble()).toFloat().coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth()); Text(localizedNumber(value.uploaded) + " / " + localizedNumber(value.chunks), style = MaterialTheme.typography.bodySmall) }
            if (value.channel != null) { SectionTitle(value.channel.name); SelectionContainer { Text(value.channel.id, style = MaterialTheme.typography.bodySmall) }; ChannelSnapshotNotice() }
            else {
                value.expiresAt?.let { Text(str(R.string.text_expires_1efbbe3) + displayTime(it)) }
                value.maxSessions?.let { Text(str(R.string.text_download_sessions_3c6ad5d) + localizedNumber(it)) }
                if (value.password == true) Text(str(R.string.text_password_protected_7587e45))
            }
            value.error?.let { Notice(errorText(it), true) }
            value.request?.let { request -> OutlinedButton(onClick = { client.approvals.open(request) }) { Text(str(R.string.text_open_original_approval_2607721)) } }
            if (value.state in setOf("prepared", "needs_approval", "awaiting_user", "interrupted", "failed", "cancelled") && !value.cancelRequested) Button(onClick = { client.secretSharing.request(id) }, enabled = !working && read is ReadState.Ready) { Text(str(R.string.text_review_publication_or_resume_c40fa98)) }
            if (value.state in setOf("unknown", "publishing", "uploading", "encrypting", "revoke_pending")) {
                Notice(str(R.string.text_the_outcome_is_uncertain_check_this_original_snapshot_before_retr_83e738c))
                Button(onClick = { client.secretSharing.reconcile(id) }, enabled = !working) { Text(str(R.string.text_reconcile_original_snapshot_88bff05)) }
            }
            if (value.channel == null && value.state == "published" && !value.cancelRequested) OutlinedButton(onClick = { client.secretSharing.select(id, reveal = true) }, enabled = !working) { Text(str(R.string.text_show_share_link_fb39fbb)) }
            if (link != null) {
                SectionTitle(str(R.string.text_share_link_b335ffa))
                SelectionContainer { Text(link, style = MaterialTheme.typography.bodySmall) }
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    TextButton(onClick = { client.clipboard.copy(link) }) { Text(str(R.string.text_copy_link_8e702f5)) }
                    TextButton(onClick = { client.beginExternalInteraction(); context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, link), null)) }) { Text(str(R.string.text_share_link_b335ffa)) }
                    TextButton(onClick = { client.secretSharing.hideLink(id) }) { Text(str(R.string.text_hide_d2e660d)) }
                }
                Text(str(R.string.text_anyone_with_the_complete_link_and_required_password_can_retrieve_5d7f03c), style = MaterialTheme.typography.bodySmall)
            }
            if (value.channel == null && value.state !in setOf("revoked", "cancelled")) TextButton(onClick = { revoke = true }, enabled = !working) { Text(str(R.string.text_revoke_or_cancel_this_link_372f372), color = MaterialTheme.colorScheme.error) }
            TextButton(onClick = { client.secretSharing.select(id) }, enabled = !working) { Text(str(R.string.text_refresh_original_snapshot_e83960f)) }
            if (working) LinearProgressIndicator(Modifier.fillMaxWidth())
        }
    }
    if (revoke) AlertDialog(onDismissRequest = { revoke = false }, title = { Text(str(R.string.text_revoke_this_snapshot_link_f42190b)) },
        text = { Text(str(R.string.text_this_requests_an_end_to_future_retrieval_saved_copies_remain_usab_4d07816)) },
        confirmButton = { TextButton(onClick = { revoke = false; client.secretSharing.revoke(id) }) { Text(str(R.string.text_revoke_or_cancel_0e052a1)) } },
        dismissButton = { TextButton(onClick = { revoke = false }) { Text(str(R.string.text_go_back_7741045)) } })
}

@Composable fun SecretPackageScreen(client: ClientController, channel: String?, sequence: ULong?) {
    val read by client.secretSharing.preview.state.collectAsStateWithLifecycle()
    val form by client.secretSharing.incoming.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    val source = PackageInput(channel, sequence)
    val current = form.takeIf { it.source == source }
    val preview = read.valueOrNull()?.takeIf { current != null }
    var choosingFolder by remember { mutableStateOf(false) }
    val working = state.working("secret:preview") || state.working("secret:import")
    LaunchedEffect(source) { client.secretSharing.receiver(source); client.secretSharing.refreshIncoming() }
    LaunchedEffect(qr.result) { client.qr.consume("secret-package")?.let { link -> client.secretSharing.editIncoming { it.copy(link = link) } } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        if (current == null) return@Column
        val editable = !working && current.request == null && current.completed == null && current.pending == null
        if (source.channel == null) {
            OutlinedTextField(current.link, { link -> client.secretSharing.editIncoming { it.copy(link = link.take(8192)) } }, Modifier.fillMaxWidth(), enabled = editable,
                label = { Text(str(R.string.text_paste_secret_package_link_64af151)) }, minLines = 2, maxLines = 4, keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri))
            TextButton(onClick = { client.qr.open("secret-package") }, enabled = editable) { Text(str(R.string.text_scan_a_package_link_6937834)) }
            OutlinedTextField(current.password, { password -> client.secretSharing.editIncoming { it.copy(password = password) } }, Modifier.fillMaxWidth(), enabled = editable,
                label = { Text(str(R.string.text_password_if_required_313b93c)) }, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Password))
            Notice(str(R.string.text_preview_downloads_and_verifies_the_package_on_this_device_and_use_ab1656c))
        } else Notice(str(R.string.text_preview_verifies_the_sender_channel_epoch_and_original_message_be_3e32cf2))
        if (current.pending == null && current.request == null && current.completed == null) Button(onClick = client.secretSharing::inspect,
            enabled = !working && (channel != null && sequence != null || current.link.isNotBlank())) { Text(if (preview == null) str(R.string.text_preview_package_7f98c9f) else str(R.string.text_refresh_verified_preview_5d8e380)) }
        if (working) { LinearProgressIndicator(Modifier.fillMaxWidth()); if (state.working("secret:preview")) TextButton(onClick = { client.cancelOperation("secret:preview") }) { Text(str(R.string.text_cancel_preview_a89c579)) } }
        if (read !is ReadState.Idle) ReadFeedback(read, client.secretSharing::inspect)
        preview?.let { value ->
            Text(value.title, style = MaterialTheme.typography.headlineSmall)
            Text(displayTime(value.createdAt), style = MaterialTheme.typography.bodySmall)
            value.channel?.let { origin ->
                SectionTitle(str(R.string.text_sender_account_e9f6961)); SelectionContainer { Text(origin.sender, style = MaterialTheme.typography.bodySmall) }
                Text(str(R.string.text_channel_0126d4b) + origin.id, style = MaterialTheme.typography.bodySmall)
            }
            if (value.imported.isNotEmpty()) Notice(str(R.string.text_this_package_was_already_imported_here_existing_copies_remain_ind_54592ed))
            SectionTitle(str(R.string.text_select_fields_to_import_97581a6))
            value.fields.forEach { field -> SelectionRow(packageFieldTitle(field), field.id in current.fields, editable, field.section) { checked -> client.secretSharing.editIncoming { it.copy(fields = if (checked) it.fields + field.id else it.fields - field.id) } } }
            if (value.notes) SelectionRow(str(R.string.text_notes_f7df47b), current.notes, editable) { checked -> client.secretSharing.editIncoming { it.copy(notes = checked) } }
            if (value.fields.any { it.id in current.fields && it.kind == "otp" }) OtpSnapshotNotice()
            if (value.attachments.isNotEmpty()) {
                SectionTitle(str(R.string.text_select_attachments_to_import_de0f7db))
                value.attachments.forEachIndexed { index, file -> SelectionRow(file.name, index.toUInt() in current.attachments, editable, fileSize(file.size)) { checked ->
                    client.secretSharing.editIncoming { it.copy(attachments = if (checked) it.attachments + index.toUInt() else it.attachments - index.toUInt()) }
                } }
                OutlinedButton(onClick = { choosingFolder = true }, enabled = editable) { Text(str(R.string.text_destination_843a557) + current.folderName.ifBlank { str(R.string.text_files_c79ddc2) }) }
            }
            Notice(str(R.string.text_import_creates_independent_items_attachments_stay_on_this_device_fe5265b))
            Text(str(R.string.text_verified_package_fingerprint_c59476c), style = MaterialTheme.typography.labelMedium)
            SelectionContainer { Text(value.digest, style = MaterialTheme.typography.bodySmall.code()) }
        }
        if (current.pending != null && current.request == null) Notice(str(R.string.text_the_original_selection_is_fixed_continue_that_request_to_learn_it_480f539))
        if (current.completed == null) Button(onClick = client.secretSharing::importRequest,
            enabled = !working && (current.request != null || current.pending != null || preview != null && current.selected)) {
            Text(when { current.request != null -> str(R.string.text_open_original_approval_2607721); current.pending != null -> str(R.string.text_retry_original_import_request_8c64bea); else -> str(R.string.text_review_selected_import_1457454) })
        }
        if (current.request != null && current.completed == null) TextButton(onClick = client.secretSharing::refreshIncoming) { Text(str(R.string.text_check_original_import_result_23a1ea7)) }
        current.completed?.let { ids -> SectionTitle(str(R.string.text_imported_copies_3c1c692)); ids.forEach { id -> TextButton(onClick = { client.navigation.push(Route("vault", id)) }) { Text(id) } } }
        Text(str(R.string.text_mobile_packages_are_limited_to_8_mib_preview_verifies_content_bef_7a4533d), style = MaterialTheme.typography.bodySmall)
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
    LazyColumn(contentPadding = pagePadding()) {
        item {
            WorkspaceSearchField(query.text, { picker.browse(VaultQuery(text = it)) }, str(R.string.text_search_vault_f6e42b2))
            ReadFeedback(read) { picker.browse() }
            if (read is ReadState.Empty) EmptyPanel(str(if (query.text.isBlank()) R.string.autofill_no_items else R.string.text_no_matching_items_b35c7c3))
        }
        items(read.valueOrNull()?.items.orEmpty(), key = { it.id }) { item ->
            ListItem(modifier = Modifier.clickable(enabled = !item.reserved && !item.deleted && !item.conflict) { client.navigation.push(Route("secret-compose", item.id, channel = channel)) },
                headlineContent = { Text(item.title) }, supportingContent = { Text(vaultKindName(item.kind)) }, leadingContent = { SecretIcon(item.kind) })
        }
        if (read.valueOrNull()?.hasMore == true) item { TextButton(onClick = { picker.browse(more = true) }) { Text(str(R.string.text_load_more_4e1d9f8)) } }
    }
}

@Composable private fun OtpSnapshotNotice() = Notice(str(R.string.text_selected_otp_fields_include_the_reusable_seed_not_just_a_current_4a6ddda))
@Composable private fun ChannelSnapshotNotice() = Notice(str(R.string.text_members_with_access_to_this_channel_epoch_including_later_history_a29d046))

@Composable fun SecretApprovalContent(review: SecretReview) {
    SectionTitle(review.title)
    Text(if (review.importing) str(R.string.text_create_an_independent_copy_of_this_exact_selection_a20b10b) else str(R.string.text_deliver_an_independent_snapshot_of_this_exact_selection_9c08d03))
    SectionTitle(str(R.string.text_selected_fields_cf5d199))
    if (review.fields.isEmpty()) Text(str(R.string.text_no_fields_selected_6b0570a))
    review.fields.forEach { Text(if (review.importing) packageFieldTitle(it) else vaultFieldTitle(it.id, it.kind, it.label)) }
    Text(if (review.notes) str(R.string.text_notes_included_0a3f128) else str(R.string.text_notes_excluded_cfd2b9b))
    if (review.fields.any { it.kind == "otp" }) OtpSnapshotNotice()
    SectionTitle(str(R.string.text_selected_attachments_4c5e318))
    if (review.files.isEmpty()) Text(str(R.string.text_no_attachments_selected_8b05287))
    review.files.forEach { file -> Card { Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(file.name); Text(fileSize(file.size) + " · " + file.mime, style = MaterialTheme.typography.bodySmall)
        SelectionContainer { Text(file.sha256, style = MaterialTheme.typography.bodySmall) }
    } } }
    if (review.importing) {
        Text(str(R.string.text_destination_folder_2f893c8) + review.destination)
        Notice(str(R.string.text_imported_attachments_stay_on_this_device_and_inherit_this_folder_d1a4fc8))
    } else if (review.channel != null) {
        SectionTitle(str(R.string.text_destination_channel_d035e1e)); Text(review.channel.name)
        SelectionContainer { Text(review.channel.id, style = MaterialTheme.typography.bodySmall) }
        Text(str(R.string.text_epoch_4202c18) + review.channel.epoch + " / " + review.channel.aclVersion, style = MaterialTheme.typography.bodySmall)
        ChannelSnapshotNotice()
    } else {
        SectionTitle(str(R.string.text_share_link_b335ffa))
        review.expiry?.let { Text(str(R.string.text_expires_1efbbe3) + displayTime(it)) }
        review.sessions?.let { Text(str(R.string.text_download_sessions_3c6ad5d) + localizedNumber(it)) }
        Text(if (review.password == true) str(R.string.text_password_protected_7587e45) else str(R.string.text_no_additional_password_d56364a))
        Notice(str(R.string.text_a_full_link_and_required_password_grant_retrieval_within_these_li_323a1eb))
    }
    SectionTitle(if (review.importing) str(R.string.text_verified_package_e203c4b) else str(R.string.text_fixed_source_revision_65e4963))
    SelectionContainer { Column { Text(review.source, style = MaterialTheme.typography.bodySmall); Text(review.revision, style = MaterialTheme.typography.bodySmall) } }
}
