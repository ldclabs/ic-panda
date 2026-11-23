package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun MessageSettingsScreen(client: ClientController) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding())) {
        Section {
            ListRow(str(R.string.text_receiving_policy_7270844), icon = Icons.Outlined.MarkEmailRead) { client.navigation.push(Route("inbox-settings")) }
            ListRow(str(R.string.text_blocked_accounts_c7f6322), icon = Icons.Outlined.Block) { client.navigation.push(Route("blocked-accounts")) }
            ListRow(str(R.string.text_your_reports_8c328aa), icon = Icons.Outlined.Flag) { client.navigation.push(Route("reports")) }
            ListRow(str(R.string.text_hidden_conversations_46b7307), icon = Icons.Outlined.VisibilityOff) { client.navigation.push(Route("channel-hidden")) }
        }
    }
}

@Composable fun InboxSettingsScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    val policy by client.safety.policy.state.collectAsStateWithLifecycle()
    val draft by client.safety.receiving.collectAsStateWithLifecycle()
    val pending by client.safety.pending.collectAsStateWithLifecycle()
    var confirming by remember { mutableStateOf(false) }
    var adding by remember { mutableStateOf("") }
    val form = draft
    val working = state.working("safety:policy:save")
    LaunchedEffect(state.epoch) { client.safety.loadPolicy() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            ReadFeedback(policy, { client.safety.loadPolicy(true) })
            Notice(str(R.string.text_choose_who_may_send_you_free_letters_existing_blocks_invitations_b203311))
            if (form != null) {
                listOf("closed", "contacts", "free").forEach { mode -> FilterChip(form.mode == mode, { client.safety.editReceiving(form.copy(mode = mode)) }, { Text(receivingMode(mode)) }, enabled = !working && pending == null) }
                if (form.mode == "paid") Notice(str(R.string.text_the_current_policy_requires_payment_selecting_a_free_or_closed_po_5699ffb))
                when (form.mode) {
                    "closed" -> Text(str(R.string.receive_closed))
                    "contacts" -> Text(str(R.string.receive_selected))
                    "free" -> Text(str(R.string.receive_free))
                }
                if (form.mode == "contacts") SectionTitle(str(R.string.text_selected_accounts_e482d88))
            }
        }
        itemsIndexed(form?.takeIf { it.mode == "contacts" }?.allow.orEmpty()) { index, id ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { ParticipantIdentity(client, id, true) }
                TextButton(onClick = { form?.let { client.safety.editReceiving(it.copy(allow = it.allow.filterIndexed { i, _ -> i != index })) } }, enabled = !working && pending == null) { Text(str(R.string.text_remove_78190c6)) }
            }
        }
        item {
            if (form != null) {
                if (form.mode == "contacts") {
                    RecipientField(client, adding, { adding = it.trim() }, enabled = !working && pending == null)
                    TextButton(onClick = { client.safety.editReceiving(form.copy(allow = form.allow + adding)); adding = "" }, enabled = !working && pending == null && validAccount(adding) && adding !in form.allow && form.allow.size < 1000) { Text(str(R.string.text_add_account_5fa00b0)) }
                }
                policy.valueOrNull()?.let { Text(str(R.string.text_inbox_capacity_e9297a7) + (it.capacity?.toString() ?: str(R.string.text_not_configured_7f1e4ed)), style = MaterialTheme.typography.bodySmall) }
                Button(onClick = { confirming = true }, enabled = !working && form.valid && policy.valueOrNull()?.let { pending != null || it.mode != form.mode || it.allow.toSet() != form.allow.toSet() } == true) { Text(if (pending != null) str(R.string.text_retry_original_change_670ac66) else str(R.string.text_save_receiving_policy_8dcc597)) }
            }
            if (working) LinearProgressIndicator(Modifier.fillMaxWidth())
            ListRow(str(R.string.text_blocked_accounts_c7f6322)) { client.navigation.push(Route("blocked-accounts")) }
        }
    }
    if (confirming) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_apply_these_receiving_rules_f6a2126)) }, text = { Column { Text(receivingMode((pending ?: form)?.mode.orEmpty())); Text(str(R.string.text_blocked_accounts_remain_blocked_choosing_a_free_or_closed_policy_c088050)) } }, confirmButton = { TextButton(onClick = { confirming = false; client.safety.savePolicy() }) { Text(str(R.string.text_save_settings_a01ddf9)) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable fun BlockedAccountsScreen(client: ClientController, initial: String? = null) {
    val blocked by client.safety.blocked.state.collectAsStateWithLifecycle()
    val cloudError by client.safety.blockSyncError.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var input by remember(initial) { mutableStateOf(initial.orEmpty()) }
    var change by remember { mutableStateOf<Pair<String, Boolean>?>(null) }
    LaunchedEffect(state.epoch) { client.safety.loadBlocks() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            ReadFeedback(blocked, { client.safety.loadBlocks() })
            Notice(str(R.string.text_blocked_senders_are_hidden_on_this_device_after_online_synchroniz_34bdd62))
            RecipientField(client, input, { input = it.trim() }, label = str(R.string.text_account_to_block_827a9f0))
            TextButton(onClick = { change = input to true }, enabled = validAccount(input) && input != state.session?.account && input !in blocked.valueOrNull().orEmpty()) { Text(str(R.string.text_block_account_11bc76b)) }
            cloudError?.let { Notice(str(R.string.text_local_rules_are_saved_online_synchronization_needs_attention_fc85a7d) + " " + errorText(it), true) }
            TextButton(onClick = client.safety::syncBlocks, enabled = !state.working("safety:block-sync")) { Text(str(R.string.text_synchronize_online_blocks_491966e)) }
            if (blocked is ReadState.Empty) Text(str(R.string.text_no_blocked_accounts_65f8b9c))
        }
        items(blocked.valueOrNull().orEmpty().sorted(), key = { it }) { account ->
            ParticipantIdentity(client, account, showId = true)
            TextButton(onClick = { change = account to false }, enabled = !state.working("safety:block:$account")) { Text(str(R.string.text_unblock_0d7ca3f)) }
            TextButton(onClick = { client.safety.openReport(ReportTarget("account", account, account)) }) { Text(str(R.string.text_report_account_e84056e)) }
            HorizontalDivider()
        }
    }
    change?.let { (account, block) -> AlertDialog(onDismissRequest = { change = null }, title = { Text(if (block) str(R.string.text_block_this_account_c023ae0) else str(R.string.text_unblock_this_account_b9a3a3b)) }, text = { ParticipantIdentity(client, account, true) }, confirmButton = { TextButton(onClick = { change = null; client.safety.block(account, block); input = "" }) { Text(str(R.string.text_confirm_563fd5a)) } }, dismissButton = { TextButton(onClick = { change = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}

@Composable fun ReportComposeScreen(client: ClientController, target: ReportTarget) {
    val draft by client.safety.reportDraft.collectAsStateWithLifecycle()
    val review by client.safety.report.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val focus = LocalFocusManager.current
    LaunchedEffect(target, state.epoch) { client.safety.openReport(target) }
    val form = draft?.takeIf { it.target == target }
    val prepared = review.valueOrNull()?.takeIf { form != null && it.account == target.account && it.resource == target.resource }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (prepared != null) ReportReviewContent(client, prepared)
        else if (form != null) {
            ParticipantIdentity(client, target.account, true)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("spam", "harassment", "illegal", "other").forEach { reason ->
                    FilterChip(form.reason == reason, { client.safety.editReport(form.copy(reason = reason)) }, { Text(reportReason(reason)) })
                }
            }
            OutlinedTextField(form.note, { client.safety.editReport(form.copy(note = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), minLines = 3, label = { Text(str(R.string.text_describe_the_issue_d8ac397)) }, supportingText = { Text(localizedNumber(form.note.toByteArray(Charsets.UTF_8).size) + " / " + localizedNumber(2000) + " " + str(R.string.text_utf_8_bytes_121f5ad)) }, isError = form.note.isNotEmpty() && !form.valid)
            if (target.kind != "account") SettingToggle(str(R.string.text_include_this_message_s_text_359f600), form.disclose, { client.safety.editReport(form.copy(disclose = it)) })
            Notice(str(R.string.text_only_the_details_you_review_and_text_you_explicitly_select_are_se_a93daca))
            if (review !is ReadState.Idle) ReadFeedback(review, client.safety::prepareReport)
        }
        }
        if (prepared == null && form != null) {
            HorizontalDivider()
            Button(onClick = { focus.clearFocus(); client.safety.prepareReport() }, enabled = form.valid && !state.working("safety:report:prepare"), modifier = Modifier.fillMaxWidth().padding(20.dp, 12.dp)) { Text(str(R.string.text_review_report_0dd84bd)) }
        }
    }
}

@Composable private fun ReportReviewContent(client: ClientController, value: ReportReview) {
    val state by client.state.collectAsStateWithLifecycle()
    val errors by client.safety.submissionErrors.collectAsStateWithLifecycle()
    Text(str(R.string.text_report_review_644e7fd), style = MaterialTheme.typography.titleLarge)
    ParticipantIdentity(client, value.account, true)
    Text(reportReason(value.reason))
    if (value.note.isNotEmpty()) SelectionContainer { Text(value.note) }
    if (value.disclosedText != null) {
        Text(str(R.string.text_message_text_to_disclose_376773f), style = MaterialTheme.typography.titleMedium)
        SelectionContainer { Text(value.disclosedText) }
    } else Notice(str(R.string.text_no_message_text_is_included_cac6498))
    Text(str(R.string.text_request_id_3182520), style = MaterialTheme.typography.labelMedium)
    SelectionContainer { Text(value.report.id, style = MaterialTheme.typography.bodySmall) }
    Text(reportState(value.report.state))
    errors[value.report.id]?.let { Notice(errorText(it), true) }
    if (value.report.state == "pending") {
        Notice(str(R.string.text_submitting_shares_exactly_these_details_with_moderation_retrying_683dd40))
        Button(onClick = { client.safety.submitReport(value.report.id) }, enabled = !state.working("safety:report:submit:${value.report.id}")) { Text(str(R.string.text_submit_this_report_93efb76)) }
    }
}

@Composable fun ReportsScreen(client: ClientController, id: String?) {
    val reports by client.safety.reports.state.collectAsStateWithLifecycle()
    val detail by client.safety.report.state.collectAsStateWithLifecycle()
    LaunchedEffect(id) { if (id == null) client.safety.loadReports() else client.safety.reviewReport(id) }
    if (id != null) Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ReadFeedback(detail, { client.safety.reviewReport(id) }); detail.valueOrNull()?.takeIf { it.report.id == id }?.let { ReportReviewContent(client, it) }
    } else LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item { ReadFeedback(reports, { client.safety.loadReports() }); if (reports is ReadState.Empty) EmptyState(str(R.string.text_no_reports_22f7dd6)) }
        items(reports.valueOrNull().orEmpty(), key = { it.id }) { report ->
            Text(reportState(report.state), style = MaterialTheme.typography.titleMedium)
            SelectionContainer { Text(report.id, style = MaterialTheme.typography.bodySmall) }
            report.resolution?.let { Text(it) }
            report.createdAt?.let { Text(displayTime(it), style = MaterialTheme.typography.bodySmall) }
            if (report.state == "pending") TextButton(onClick = { client.navigation.push(Route("reports", id = report.id)) }) { Text(str(R.string.text_review_saved_report_be7834f)) }
            HorizontalDivider()
        }
    }
}

private fun receivingMode(mode: String) = when (mode) { "closed" -> str(R.string.text_closed_4999cac); "contacts" -> str(R.string.text_selected_accounts_e482d88); "free" -> str(R.string.text_anyone_free_of_charge_967e5e2); "paid" -> str(R.string.text_current_paid_policy_1255778); else -> str(R.string.text_unrecognized_receiving_policy_b78b92a) }
private fun reportReason(reason: String) = when (reason) { "spam" -> str(R.string.text_spam_afdcb81); "harassment" -> str(R.string.text_harassment_f9197e0); "illegal" -> str(R.string.text_illegal_content_506dfed); else -> str(R.string.text_other_71e9878) }
private fun reportState(state: String) = when (state) { "pending" -> str(R.string.text_waiting_to_submit_b989b07); "received", "submitted" -> str(R.string.text_received_for_review_118e82e); "reviewed" -> str(R.string.text_reviewed_147ebf0); else -> str(R.string.text_status_needs_verification_8009b3d) }
