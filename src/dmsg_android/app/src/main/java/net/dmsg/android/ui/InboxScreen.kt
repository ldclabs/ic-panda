package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

private enum class LetterFolder(val label: Int) { INBOX(R.string.text_inbox_8460020), SENT(R.string.text_sent_a16d792), PENDING(R.string.text_pending_3f118eb), DRAFTS(R.string.text_drafts_14a9642), ARCHIVED(R.string.text_archived_fafdfe8) }

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun InboxScreen(client: ClientController, state: UiState, route: Route, showScopes: Boolean) {
    val blocked by client.safety.blocked.state.collectAsStateWithLifecycle()
    val incoming by client.letters.incoming.state.collectAsStateWithLifecycle()
    val history by client.letters.history.state.collectAsStateWithLifecycle()
    val saved by client.letters.saved.state.collectAsStateWithLifecycle()
    val drafts by client.letters.drafts.collectAsStateWithLifecycle()
    var folder by remember { mutableStateOf(LetterFolder.entries.find { it.name == client.letters.location.folder } ?: LetterFolder.INBOX) }
    var search by remember { mutableStateOf(client.letters.location.query) }
    var limit by remember(folder, search) { mutableIntStateOf(100) }
    val listState = key(folder, search) { rememberLazyListState() }
    val selected = route.id.takeUnless { route.field == "sent" }
    val sent = route.id.takeIf { route.field == "sent" }
    var editing by remember(state.epoch) { mutableStateOf(client.letters.location.editing) }
    var reopen by remember { mutableStateOf(false) }
    var discard by remember { mutableStateOf(false) }
    val letter = incoming.valueOrNull()?.find { it.id == selected && blocked.valueOrNull()?.contains(it.sender) == false }
    val outgoing = history.valueOrNull()?.find { it.id == sent }
    val owner = remember(state.epoch) { client.currentStamp() }
    val location by rememberUpdatedState(LetterLocation(folder.name, search, editing))
    DisposableEffect(owner) { onDispose { client.letters.rememberLocation(location, owner) } }
    LaunchedEffect(state.epoch) { client.safety.loadBlocks(); client.letters.refresh() }
    LaunchedEffect(selected, letter?.id) { letter?.takeIf { !it.read }?.let { client.letters.mark(it, read = true) } }
    Column(Modifier.fillMaxSize()) {
        if (route.id == null) {
            Column(Modifier.padding(start = 16.dp, top = 8.dp, end = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                WorkspaceSearchField(search, { search = it }, str(R.string.text_search_letters_1249ccd))
                if (showScopes) {
                    WorkspaceScope(str(R.string.text_conversations_23a085e), str(R.string.text_letters_cfbff5c), true, switch = client.navigation::back)
                }
            }
            Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(str(R.string.text_on_this_device_4541a6f), Modifier.weight(1f), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                IconButton(onClick = { client.navigation.push(Route("message-settings")) }) { Icon(Icons.Outlined.Settings, str(R.string.text_message_settings_1a28455)) }
                IconButton(onClick = { client.letters.refresh() }) { Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_cba212b)) }
                IconButton(onClick = { editing = client.letters.create() }) { Icon(Icons.Outlined.Edit, str(R.string.text_write_a_letter_1a8f9c9)) }
            }
            PrimaryScrollableTabRow(selectedTabIndex = folder.ordinal, edgePadding = 16.dp) { LetterFolder.entries.forEach { item ->
                Tab(folder == item, onClick = { folder = item }, text = { Text(str(item.label)) })
            } }
            LazyColumn(Modifier.weight(1f), state = listState, contentPadding = PaddingValues(16.dp, 16.dp, 16.dp, 16.dp + LocalWorkspaceBarInset.current)) {
                when (folder) {
                    LetterFolder.INBOX, LetterFolder.ARCHIVED -> {
                        item { ReadFeedback(blocked, { client.safety.loadBlocks() }); ReadFeedback(incoming, { client.letters.refresh() }) }
                        val rows = incoming.valueOrNull().orEmpty().filter { blocked.valueOrNull()?.contains(it.sender) == false && it.archived == (folder == LetterFolder.ARCHIVED) && (search.isBlank() || it.text.contains(search, true) || it.sender.contains(search, true)) }
                        items(rows.take(limit), key = { it.id }) { row -> LetterRow(row.sender, row.text, messageTime(row.createdAt), !row.read) { client.navigation.push(Route("inbox", id = row.id)) } }
                        if (rows.isEmpty() && blocked.valueOrNull() != null && (incoming is ReadState.Ready || incoming is ReadState.Empty || incoming is ReadState.Stale)) item {
                            EmptyPanel(str(if (search.isBlank()) R.string.text_no_letters_here_edd0b41 else R.string.no_matches))
                            if (folder == LetterFolder.INBOX && search.isBlank()) OutlinedButton(onClick = { editing = client.letters.create() }) { Text(str(R.string.text_write_a_letter_1a8f9c9)) }
                        }
                        if (rows.size > limit) item { TextButton(onClick = { limit += 100 }) { Text(str(R.string.text_load_more_4e1d9f8)) } }
                    }
                    LetterFolder.SENT, LetterFolder.PENDING -> {
                        item { ReadFeedback(history, { client.letters.refresh(false) }); Notice(str(R.string.text_sending_history_is_saved_on_this_device_older_records_may_not_con_b7b9992)) }
                        val rows = history.valueOrNull().orEmpty().filter { it.pending == (folder == LetterFolder.PENDING) && (search.isBlank() || it.recipient.contains(search, true) || it.text?.contains(search, true) == true) }
                        items(rows.take(limit), key = { it.id }) { row -> LetterRow(row.recipient, row.text ?: str(R.string.text_message_body_unavailable_6b03555), letterState(row.state) + " · " + messageTime(row.createdAt), false) { client.navigation.push(Route("inbox", id = row.id, field = "sent")) } }
                        if (rows.isEmpty() && (history is ReadState.Ready || history is ReadState.Empty)) item { EmptyPanel(str(if (search.isBlank()) R.string.text_no_letters_here_edd0b41 else R.string.no_matches)) }
                        if (rows.size > limit) item { TextButton(onClick = { limit += 100 }) { Text(str(R.string.text_load_more_4e1d9f8)) } }
                    }
                    LetterFolder.DRAFTS -> {
                        item { ReadFeedback(saved, { client.letters.refresh(false) }) }
                        val rows = drafts.values.filter { it.text.isNotBlank() || it.recipient.isNotBlank() || !it.invitation.isNullOrBlank() }
                            .filter { search.isBlank() || it.text.contains(search, true) || it.recipient.contains(search, true) }.sortedByDescending { it.updatedAt }
                        items(rows.take(limit), key = { it.id }) { row -> LetterRow(row.recipient.ifBlank { str(R.string.text_no_recipient_4b720be) }, row.text, str(R.string.text_draft_ce89527), false) { editing = row.id } }
                        if (rows.isEmpty() && (saved is ReadState.Ready || saved is ReadState.Empty)) item { EmptyPanel(str(if (search.isBlank()) R.string.text_no_drafts_46c70ab else R.string.no_matches)) }
                        if (rows.size > limit) item { TextButton(onClick = { limit += 100 }) { Text(str(R.string.text_load_more_4e1d9f8)) } }
                    }
                }
            }
        } else if (letter != null || outgoing != null) Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val account = letter?.sender ?: outgoing!!.recipient
            Text(if (letter != null) str(R.string.text_from_c08793d) else str(R.string.text_to_50fd5b7), style = MaterialTheme.typography.labelLarge)
            ParticipantIdentity(client, account, showId = true)
            TextButton(onClick = { client.clipboard.copy(account) }) { Text(str(R.string.text_copy_account_id_faa481e)) }
            Text(messageTime(letter?.createdAt ?: outgoing?.createdAt), style = MaterialTheme.typography.labelMedium)
            SelectionContainer { Text(letter?.text ?: outgoing?.text ?: str(R.string.text_message_body_unavailable_6b03555), if (letter != null) Modifier.reminderVisible(client, NoticeTarget("inbox", letter.id), revision = letter) else Modifier) }
            if (letter != null) {
                Button(onClick = { val id = client.letters.create(); client.letters.update(LetterDraft(id = id, recipient = letter.sender)); editing = id }) { Text(str(R.string.text_reply_b4c9334)) }
                OutlinedButton(onClick = { client.letters.mark(letter, archived = !letter.archived) }) { Text(if (letter.archived) str(R.string.text_move_to_inbox_3b770ba) else str(R.string.text_archive_0f0188e)) }
                TextButton(onClick = { client.letters.mark(letter, read = !letter.read) }) { Text(if (letter.read) str(R.string.text_mark_unread_b10bd76) else str(R.string.text_mark_read_20bb0ec)) }
                TextButton(onClick = { reopen = true }) { Text(str(R.string.text_allow_another_letter_fee59fb)) }
                TextButton(onClick = { client.navigation.push(Route("invite-letter-sender", id = letter.sender)) }) { Text(str(R.string.text_invite_to_a_conversation_e66d6f9)) }
                TextButton(onClick = { client.safety.openReport(ReportTarget("inbox", letter.sender, letter.id)) }) { Text(str(R.string.text_report_letter_0730024)) }
                TextButton(onClick = { client.safety.openBlock(letter.sender) }) { Text(str(R.string.text_block_sender_021b756)) }
                Notice(str(R.string.text_archiving_does_not_allow_another_letter_receiving_a_letter_grants_245790e))
            } else if (outgoing != null) {
                Text(letterState(outgoing.state), style = MaterialTheme.typography.titleMedium)
                outgoing.error?.let { Notice(errorText(it), true) }
                if (outgoing.state == "quoted") Notice(str(R.string.text_payment_is_required_this_client_has_not_paid_or_delivered_the_let_ed11bd6))
                if (outgoing.pending) Button(onClick = { client.letters.retry(outgoing.id) }, enabled = !state.working("letter:send:" + outgoing.id)) { Text(str(R.string.text_retry_original_letter_5a838c3)) }
                OutlinedButton(onClick = { client.letters.check(outgoing.id) }, enabled = !state.working("letter:check:" + outgoing.id)) { Text(str(R.string.text_check_delivery_status_4b7ae8e)) }
                if (outgoing.editable) TextButton(onClick = { client.letters.edit(outgoing.id) {
                    if (client.navigation.state.value.current == route) { editing = outgoing.id; client.navigation.back() }
                } }) { Text(str(R.string.text_return_to_editing_190592d)) }
            }
        } else {
            if (sent == null) ReadFeedback(blocked, client.safety::loadBlocks)
            val source = if (sent == null) incoming else history
            ReadFeedback(source) { client.letters.refresh() }
            if ((sent != null || blocked.valueOrNull() != null) && (source is ReadState.Ready || source is ReadState.Empty || source is ReadState.Stale)) {
                EmptyPanel(str(R.string.text_message_body_unavailable_6b03555))
            }
        }
    }
    editing?.let { id ->
        val form = drafts[id]
        if (form != null) ModalBottomSheet(onDismissRequest = { editing = null }, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).imePadding().padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Text(str(R.string.text_write_a_letter_1a8f9c9), style = MaterialTheme.typography.headlineSmall)
                RecipientField(client, form.recipient, { client.letters.update(form.copy(recipient = it.trim())) }, label = str(R.string.text_recipient_account_id_99c767d))
                OutlinedTextField(form.text, { client.letters.update(form.copy(text = it)) }, Modifier.fillMaxWidth(), minLines = 6, label = { Text(str(R.string.text_message_373be16)) }, isError = form.bytes > 6000, supportingText = { Text("${form.bytes} / 6000 " + str(R.string.text_utf_8_bytes_121f5ad)) })
                OutlinedTextField(form.invitation.orEmpty(), { client.letters.update(form.copy(invitation = it.trim().takeIf(String::isNotEmpty))) }, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_contact_invitation_optional_37ca036)) })
                Notice(str(R.string.text_only_free_delivery_is_attempted_a_payment_quote_never_triggers_pa_145ba4a))
                Button(onClick = { client.letters.send(id) {
                    if (editing == id && client.navigation.state.value.current == route) {
                        editing = null
                        client.navigation.push(Route("inbox", id = id, field = "sent"))
                    }
                } }, enabled = form.valid && !state.working("letter:queue:" + id), modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_send_06acd73)) }
                TextButton(onClick = { editing = null }) { Text(str(R.string.text_keep_draft_89639c8)) }
                TextButton(onClick = { discard = true }) { Text(str(R.string.text_discard_draft_6008f2b), color = MaterialTheme.colorScheme.error) }
            }
        }
    }
    if (discard) AlertDialog(onDismissRequest = { discard = false }, title = { Text(str(R.string.text_discard_this_draft_ddde861)) }, confirmButton = { TextButton(onClick = { editing?.let(client.letters::discard); discard = false; editing = null }) { Text(str(R.string.text_discard_475cd3e)) } }, dismissButton = { TextButton(onClick = { discard = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (reopen && letter != null) AlertDialog(onDismissRequest = { reopen = false }, title = { Text(str(R.string.text_allow_another_letter_5538009)) }, text = { Text(str(R.string.text_this_sender_can_submit_another_letter_under_your_current_receivin_0910e86)) }, confirmButton = { TextButton(onClick = { client.letters.mark(letter, reopen = true); reopen = false }) { Text(str(R.string.text_allow_ee9ea51)) } }, dismissButton = { TextButton(onClick = { reopen = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable private fun LetterRow(account: String, text: String, status: String, unread: Boolean, open: () -> Unit) {
    ListItem(modifier = Modifier.clickable(onClick = open), headlineContent = { Text(account, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        supportingContent = { Column { Text(text, maxLines = 2, overflow = TextOverflow.Ellipsis); Text(status, style = MaterialTheme.typography.labelSmall) } },
        leadingContent = { Icon(if (unread) Icons.Outlined.MarkEmailUnread else Icons.Outlined.MailOutline, null) })
    HorizontalDivider()
}
@Composable internal fun letterState(state: String): String = when (state) {
    "visible" -> str(R.string.text_sent_baf7943); "queued" -> str(R.string.text_waiting_to_submit_b989b07); "unconfirmed" -> str(R.string.text_waiting_for_confirmation_9c87aad)
    "failed" -> str(R.string.text_send_failed_8c3e812); "aborted", "refunded", "expired" -> str(R.string.text_not_delivered_0a57029)
    "quoted" -> str(R.string.text_payment_required_b449cfe); "admitted", "settling" -> str(R.string.text_processing_97d1ec2)
    else -> str(R.string.text_check_delivery_status_527563c)
}
