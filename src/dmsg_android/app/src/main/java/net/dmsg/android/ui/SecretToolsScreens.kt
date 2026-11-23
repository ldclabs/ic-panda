package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.approvalTitle
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun SecretAttachmentsScreen(client: ClientController, id: String) {
    val read by client.secretTools.attachments.state.collectAsStateWithLifecycle()
    val listing = read.valueOrNull()?.takeIf { it.item == id }
    LaunchedEffect(id) { client.secretTools.attachments(id) }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Text(str(R.string.text_item_references_may_sync_before_their_files_files_and_backups_are_decc308),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            ReadFeedback(read) { client.secretTools.attachments(id) }
        }
        items(listing?.files.orEmpty(), key = { it.id }) { entry ->
            val file = entry.details?.file
            Section {
                ListItem(
                    modifier = if (file != null) Modifier.clickable { client.navigation.push(Route("files", file.id)) } else Modifier,
                    headlineContent = { Text(file?.name ?: str(R.string.text_attachment_unavailable_on_this_device_10beb51)) },
                    supportingContent = {
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(
                                if (file == null) entry.id else fileSize(file.size),
                                style = if (file == null) MaterialTheme.typography.bodySmall.code() else MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            Text(when {
                                file == null -> str(R.string.text_its_reference_is_retained_7a8942b)
                                file.trashedAt != null -> str(R.string.text_in_the_file_trash_21810a4)
                                !entry.selectable -> str(R.string.text_download_and_verify_a_version_before_adding_it_to_a_snapshot_9f4d1d7)
                                else -> str(R.string.text_current_version_available_locally_7f71da2)
                            }, style = MaterialTheme.typography.bodySmall)
                        }
                    },
                    leadingContent = { RowIcon(Icons.Outlined.AttachFile) },
                )
            }
        }
        if (listing != null && listing.files.isEmpty()) item { EmptyState(str(R.string.text_no_item_attachments_343bf1e), icon = Icons.Outlined.AttachFile) }
    }
}

@Composable fun SecretReferenceScreen(client: ClientController, item: String, field: String) {
    val read by client.secretTools.reference.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val reference = read.valueOrNull()?.takeIf { it.item == item && it.field == field }
    var confirming by remember(item, field) { mutableStateOf(false) }
    LaunchedEffect(item, field) { client.secretTools.reference(item, field) }
    ScreenBar(null) {
        IconButton(onClick = { client.secretTools.reference(item, field) }) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_dependencies_298a32c))
        }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(read) { client.secretTools.reference(item, field) }
        reference?.let { value ->
            Section(
                title = str(R.string.text_field_reference_f38f4f4),
                footer = str(R.string.text_a_reference_identifies_a_field_it_contains_no_secret_value_and_gr_b9acac1),
            ) {
                CodeRow(str(R.string.text_field_reference_f38f4f4), value.uri) { client.clipboard.copy(value.uri) }
            }
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Button(onClick = { confirming = true }, enabled = !state.working("secret:rotation:create:" + item + ":" + field), modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.Loop, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_start_a_rotation_checklist_db2bb4b))
                }
                OutlinedButton(onClick = { client.clipboard.copy(value.uri) }, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.ContentCopy, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_copy_reference_8191597))
                }
            }
            SectionTitle(str(R.string.text_dependencies_on_this_device_2c15aef))
            Text(str(R.string.text_this_inventory_covers_this_device_s_records_other_devices_and_arb_af07c5c),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            SecretDependencyRows(value.index)
        }
    }
    if (confirming && reference != null) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_start_a_rotation_checklist_1941735)) },
        text = { Text(str(R.string.text_this_tracks_your_work_on_this_device_you_create_test_and_revoke_c_4c60133)) },
        confirmButton = { TextButton(onClick = { confirming = false; client.secretTools.createRotation(reference) }) { Text(str(R.string.text_start_checklist_a7a9f7e)) } },
        dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable private fun SecretDependencyRows(index: SecretDependencyIndex) {
    index.dependencies.forEach { dependency ->
        key(dependency.key) {
            DetailCard {
                Text(dependency.label.ifBlank { dependencyKind(dependency.kind) }, style = MaterialTheme.typography.titleMedium)
                Text(secretStateName(dependency.state), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                Text(secretNextAction(dependency.action), style = MaterialTheme.typography.bodyMedium)
                SelectionContainer { Text(dependency.id, style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
        }
    }
    if (index.dependencies.isEmpty()) Text(str(R.string.text_no_dependencies_recorded_on_this_device_614b62c),
        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable fun SecretHealthScreen(client: ClientController) {
    val read by client.secretTools.health.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val report = read.valueOrNull()
    var includeDismissed by remember { mutableStateOf(false) }
    var dismiss by remember { mutableStateOf<SecretFinding?>(null) }
    LaunchedEffect(Unit) { client.secretTools.health() }
    ScreenBar(null) {
        IconButton(onClick = client.secretTools::health) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_local_review_861ff88))
        }
    }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            ReadFeedback(read, client.secretTools::health)
        }
        if (report != null) {
            item {
                Section(footer = str(R.string.text_checked_a562832) + displayTime(report.observedAt) + " · " + str(R.string.text_local_observations_only_this_is_not_a_breach_scan_or_proof_of_a_p_2bcdde7)) {
                    SettingToggle(str(R.string.text_show_dismissed_findings_3b8c890), includeDismissed, { includeDismissed = it }, icon = Icons.Outlined.VisibilityOff)
                }
            }
            val findings = report.findings.filter { includeDismissed || !it.dismissed }
            items(findings, key = { it.id }) { finding ->
                DetailCard {
                    Text(findingTitle(finding.rule), style = MaterialTheme.typography.titleMedium,
                        color = if (finding.severity == "high") MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface)
                    val item = report.subjects[finding.subject]
                    Text(item?.title ?: finding.subject, style = MaterialTheme.typography.bodyMedium)
                    finding.field?.let { field ->
                        Text(item?.fields?.find { it.id == field }?.label?.ifBlank { field } ?: field,
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Text(
                        (if (finding.evidence == "last_known_cloud_state") str(R.string.text_last_known_cloud_state_9f3cb26) else str(R.string.text_observed_on_this_device_6821107)) +
                            " · " + displayTime(finding.observedAt),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(secretNextAction(finding.action), style = MaterialTheme.typography.bodyMedium)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        if (finding.field != null) {
                            TextButton(onClick = { client.navigation.push(Route("vault", finding.subject)) }) { Text(str(R.string.text_open_secret_6ca0516)) }
                            TextButton(onClick = { client.navigation.push(Route("secret-reference", finding.subject, field = finding.field)) }) { Text(str(R.string.text_review_field_and_dependencies_e933729)) }
                        }
                        if (!finding.dismissed) TextButton(onClick = { dismiss = finding }, enabled = !state.working("secret:dismiss:" + finding.id)) {
                            Text(str(R.string.text_dismiss_this_observation_719f68b))
                        } else {
                            Text(str(R.string.text_dismissed_on_this_device_5babd55), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
            if (findings.isEmpty()) item { EmptyState(str(R.string.text_no_findings_from_this_device_s_checks_c1564e4), glyph = R.drawable.glyph_security) }
            if (report.rotations.isNotEmpty()) {
                item {
                    Section(title = str(R.string.text_rotation_checklists_6fca54f)) {
                        report.rotations.sortedByDescending { it.updatedAt }.forEach { rotation ->
                            ListRow(
                                headline = report.subjects[rotation.item]?.title ?: rotation.field,
                                supporting = if (rotation.completedAt == null) str(R.string.text_in_progress_6906b67) else str(R.string.text_checklist_completed_38e6d28),
                                icon = Icons.Outlined.Loop,
                            ) { client.navigation.push(Route("secret-rotation", rotation.id)) }
                        }
                    }
                }
            } else {
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        SectionTitle(str(R.string.text_rotation_checklists_6fca54f))
                        Text(str(R.string.text_start_a_checklist_from_a_field_s_reference_page_88740fc),
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        }
    }
    dismiss?.let { finding -> AlertDialog(onDismissRequest = { dismiss = null }, title = { Text(str(R.string.text_dismiss_this_observation_7571b06)) },
        text = { Text(str(R.string.text_only_this_exact_finding_and_revision_is_hidden_on_this_device_fut_e8c95f1)) },
        confirmButton = { TextButton(onClick = { dismiss = null; client.secretTools.dismiss(finding.id) }) { Text(str(R.string.text_dismiss_observation_3d24435)) } },
        dismissButton = { TextButton(onClick = { dismiss = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}

@Composable fun SecretRotationScreen(client: ClientController, id: String) {
    val read by client.secretTools.rotation.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val detail = read.valueOrNull()?.takeIf { it.rotation.id == id }
    var confirming by remember(id) { mutableStateOf<String?>(null) }
    LaunchedEffect(id) { client.secretTools.rotation(id) }
    ScreenBar(null) {
        IconButton(onClick = { client.secretTools.rotation(id) }) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_dependencies_298a32c))
        }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(read) { client.secretTools.rotation(id) }
        detail?.let { value ->
            val rotation = value.rotation
            Notice(
                if (rotation.completedAt != null) str(R.string.text_checklist_completed_from_your_statements_upstream_status_must_be_8c2081d)
                else str(R.string.text_you_perform_provider_changes_yourself_dmsg_records_your_statement_4b02727),
            )
            Section(title = rotation.field) {
                ListRow(str(R.string.text_open_secret_6ca0516), icon = Icons.Outlined.Key) { client.navigation.push(Route("vault", rotation.item)) }
                CodeRow(str(R.string.text_original_revision_7cd153b), rotation.previous) { client.clipboard.copy(rotation.previous) }
                rotation.next?.let { CodeRow(str(R.string.text_new_revision_538256b), it) { client.clipboard.copy(it) } }
            }
            val steps = listOf(
                Triple("created", rotation.created, true), Triple("tested", rotation.tested, rotation.created),
                Triple("references_checked", rotation.referencesChecked, rotation.tested), Triple("upstream_revoked", rotation.upstreamRevoked, rotation.referencesChecked))
            Section(title = str(R.string.text_confirm_your_completed_steps_d3b6119)) {
                steps.forEach { (step, completed, enabled) ->
                    ListItem(
                        leadingContent = {
                            Icon(
                                if (completed) Icons.Outlined.CheckCircle else Icons.Outlined.RadioButtonUnchecked,
                                if (completed) str(R.string.text_completed_6ff49bc) else str(R.string.text_incomplete_022e64c),
                                tint = if (completed) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        },
                        headlineContent = { Text(rotationStepTitle(step)) },
                        supportingContent = if (!completed && rotation.completedAt == null) ({
                            TextButton(onClick = { confirming = step }, enabled = enabled && !state.working("secret:rotation:record:" + id) && read is ReadState.Ready) {
                                Text(str(R.string.text_confirm_completed_step_761f2f1))
                            }
                        }) else null,
                    )
                }
            }
            SectionTitle(str(R.string.text_current_local_dependencies_aeffc64))
            SecretDependencyRows(value.index)
            Section {
                CodeRow(str(R.string.text_inventory_fingerprint_5976df4), value.index.digest) { client.clipboard.copy(value.index.digest) }
            }
        }
    }
    confirming?.let { step -> if (detail != null) AlertDialog(onDismissRequest = { confirming = null }, title = { Text(str(R.string.text_record_this_step_as_completed_414dc52)) },
        text = { Text(rotationStepTitle(step) + "\n\n" + str(R.string.text_confirm_only_work_you_have_actually_completed_this_does_not_execu_f4bd7d1)) },
        confirmButton = { TextButton(onClick = { confirming = null; client.secretTools.record(detail, step) }) { Text(str(R.string.text_i_completed_this_step_40b1a3e)) } },
        dismissButton = { TextButton(onClick = { confirming = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}

@Composable fun SecretActivityScreen(client: ClientController) {
    val read by client.secretTools.activity.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val page = read.valueOrNull()
    var action by remember { mutableStateOf<SecretActivityEntry?>(null) }
    LaunchedEffect(Unit) { client.secretTools.activity() }
    ScreenBar(null) {
        IconButton(onClick = { client.secretTools.activity() }) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_activity_10e1e16))
        }
    }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Text(str(R.string.text_activity_belongs_to_this_device_desktop_cli_ssh_and_file_mount_re_d1b3407),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            ReadFeedback(read) { client.secretTools.activity() }
        }
        items(page?.items.orEmpty(), key = { it.id }) { entry ->
            DetailCard {
                ListItem(
                    headlineContent = { Text(approvalTitle(entry.kind)) },
                    supportingContent = {
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(secretStateName(entry.state), color = MaterialTheme.colorScheme.primary)
                            Text(entry.source, style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant)
                            entry.error?.let { Text(errorText(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error) }
                        }
                    },
                    leadingContent = { RowIcon(painterResource(R.drawable.glyph_audit)) },
                    modifier = Modifier.clickable { client.approvals.open(entry.id) },
                )
                if (entry.state in listOf("awaiting_user", "lost")) {
                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    TextButton(onClick = { action = entry }, enabled = !state.working("secret:activity:" + entry.id)) {
                        Text(if (entry.state == "lost") str(R.string.text_record_manual_review_7612f77) else str(R.string.text_cancel_pending_request_4097f9e))
                    }
                }
            }
        }
        if (read is ReadState.Empty) item { EmptyState(str(R.string.text_no_activity_on_this_device_b992006), glyph = R.drawable.glyph_audit) }
        if (page?.next != null) item {
            OutlinedButton(onClick = { client.secretTools.activity(more = true) }, enabled = !state.working("secret:activity"), modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_load_earlier_activity_ef7d280))
            }
        }
    }
    action?.let { entry ->
        val reviewed = entry.state == "lost"
        AlertDialog(onDismissRequest = { action = null }, title = { Text(if (reviewed) str(R.string.text_have_you_checked_the_missing_outcome_311ca9c) else str(R.string.text_cancel_this_request_e55f895)) },
            text = { Text(if (reviewed) str(R.string.text_this_records_your_review_it_does_not_mark_the_operation_successfu_ee7049c) else str(R.string.text_only_this_undecided_request_will_be_cancelled_completed_deliverie_0d19134)) },
            confirmButton = { TextButton(onClick = { action = null; client.secretTools.activityAction(entry.id, reviewed) }) { Text(if (reviewed) str(R.string.text_record_review_f505ca6) else str(R.string.text_cancel_request_9dfee80)) } },
            dismissButton = { TextButton(onClick = { action = null }) { Text(str(R.string.text_go_back_7741045)) } })
    }
}

fun secretStateName(state: String): String = when (state) {
    "awaiting_user", "needs_approval" -> str(R.string.text_waiting_for_approval_45d78c0); "prepared" -> str(R.string.text_prepared_on_this_device_e536d43)
    "encrypting" -> str(R.string.text_encrypting_df1a40f); "uploading" -> str(R.string.text_uploading_8d14635); "publishing" -> str(R.string.text_publication_pending_5e47cdc)
    "unknown" -> str(R.string.text_result_unconfirmed_4f84f4c); "revoke_pending" -> str(R.string.text_revocation_pending_7c3d33f)
    "published" -> str(R.string.text_published_e9a862a); "revoked" -> str(R.string.text_revoked_c4fbad3); "executing" -> str(R.string.text_in_progress_6906b67)
    "done" -> str(R.string.text_completed_6ff49bc); "cancelled" -> str(R.string.text_cancelled_bf41d7d); "rejected" -> str(R.string.text_rejected_056421e)
    "expired" -> str(R.string.text_expired_70446d7); "failed" -> str(R.string.text_failed_948e2ae); "elsewhere" -> str(R.string.text_handled_on_another_device_2bce528)
    "lost" -> str(R.string.text_outcome_no_longer_available_f989a25); "current", "active" -> str(R.string.text_current_6865fd6)
    "configured" -> str(R.string.text_configured_on_this_device_6601a96); "granted" -> str(R.string.text_access_granted_2f19dc4)
    "disabled" -> str(R.string.text_disabled_2caae82); "stale" -> str(R.string.text_needs_a_fresh_review_495fb48); "interrupted" -> str(R.string.text_interrupted_2974557)
    else -> str(R.string.text_unknown_state_0c210d9) + state
}
private fun dependencyKind(kind: String): String = when (kind) {
    "policy", "policy_grant" -> str(R.string.text_device_policy_7701a69); "mount" -> str(R.string.text_desktop_file_mapping_cbad970)
    "ssh" -> str(R.string.text_ssh_binding_7b8b66f); "snapshot_link" -> str(R.string.text_snapshot_link_e46aa30); "snapshot_channel" -> str(R.string.text_channel_snapshot_9891325)
    "process_observation" -> str(R.string.text_local_delivery_record_7c01218); else -> str(R.string.text_local_dependency_dde7112)
}
fun secretNextAction(action: String): String = when (action) {
    "review_agent_authorization" -> str(R.string.text_review_the_agent_s_authorization_on_its_originating_device_ff23dac)
    "review_policy_and_grants", "review_device_policy" -> str(R.string.text_review_this_device_s_policy_and_access_grants_e16bf98)
    "restart_mount_after_source_change", "review_mount_sources_and_restart" -> str(R.string.text_review_and_restart_the_desktop_file_mapping_on_its_originating_de_f572a60)
    "approve_current_key" -> str(R.string.text_review_the_current_key_and_renew_its_ssh_binding_on_the_originati_8279efe)
    "check_project_refs_and_delivered_copies" -> str(R.string.text_check_project_references_and_copies_already_delivered_025d0fd)
    "repair_reference" -> str(R.string.text_repair_the_unavailable_field_reference_on_its_originating_device_6e2f868)
    "review_device_history" -> str(R.string.text_review_this_device_s_activity_before_deciding_whether_access_is_s_5d0751c)
    "review_share_expiry_and_recipients" -> str(R.string.text_review_the_snapshot_s_recipients_and_expiry_77c7920)
    "review_and_revoke_old_snapshot" -> str(R.string.text_review_old_snapshots_and_revoke_remaining_links_where_appropriate_be54df2)
    "back_up_selected_file_version" -> str(R.string.text_explicitly_back_up_the_selected_file_version_if_you_want_a_cloud_2323225)
    else -> str(R.string.text_review_this_local_record_before_making_changes_dba5185)
}
private fun findingTitle(rule: String): String = when (rule) {
    "reused" -> str(R.string.text_credential_reused_e86cc3c); "weak" -> str(R.string.text_weak_credential_30b45ef); "expired" -> str(R.string.text_credential_expired_ee251a7)
    "invalid_policy_reference" -> str(R.string.text_policy_points_to_an_unavailable_field_7a10395)
    "policy_review_due" -> str(R.string.text_device_policy_needs_review_61e2a85)
    "not_observed_on_this_device" -> str(R.string.text_no_recent_use_observed_on_this_device_21ea584)
    "invalid_ssh_reference" -> str(R.string.text_ssh_binding_needs_review_d0ab9f5); "invalid_mount_reference" -> str(R.string.text_desktop_file_mapping_needs_review_8184e33)
    "active_share" -> str(R.string.text_a_snapshot_link_is_still_active_219cadd); "snapshot_precedes_current_credential" -> str(R.string.text_a_snapshot_contains_an_older_credential_26754df)
    "attachment_backup_unconfirmed" -> str(R.string.text_attachment_backup_is_unconfirmed_7a9b003); else -> str(R.string.text_review_local_finding_12ab7c6) + " · " + rule
}
private fun rotationStepTitle(step: String): String = when (step) {
    "created" -> str(R.string.text_create_a_new_credential_at_its_provider_and_save_it_in_the_vault_2c5b164)
    "tested" -> str(R.string.text_test_the_new_credential_at_its_provider_c5ba5d4)
    "references_checked" -> str(R.string.text_review_this_device_s_current_dependencies_e68afb7)
    else -> str(R.string.text_revoke_the_old_credential_at_its_provider_dece529)
}
