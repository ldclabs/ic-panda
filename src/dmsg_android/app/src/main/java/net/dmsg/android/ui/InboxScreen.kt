package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
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
import androidx.compose.material.icons.automirrored.outlined.ForwardToInbox
import androidx.compose.material.icons.automirrored.outlined.Reply
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

private enum class LetterFolder(val label: Int, val icon: ImageVector) {
    INBOX(R.string.text_inbox_8460020, Icons.Outlined.Inbox),
    SENT(R.string.text_sent_a16d792, Icons.AutoMirrored.Outlined.Send),
    PENDING(R.string.text_pending_3f118eb, Icons.Outlined.Schedule),
    DRAFTS(R.string.text_drafts_14a9642, Icons.Outlined.Drafts),
    ARCHIVED(R.string.text_archived_fafdfe8, Icons.Outlined.Archive),
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun InboxScreen(client: ClientController, state: UiState, route: Route, showScopes: Boolean) {
    val blocked by client.safety.blocked.state.collectAsStateWithLifecycle()
    val incoming by client.letters.incoming.state.collectAsStateWithLifecycle()
    val history by client.letters.history.state.collectAsStateWithLifecycle()
    val saved by client.letters.saved.state.collectAsStateWithLifecycle()
    val drafts by client.letters.drafts.collectAsStateWithLifecycle()
    val letterSearch by client.letters.searchResults.state.collectAsStateWithLifecycle()
    val globalRead by client.search.global.state.collectAsStateWithLifecycle()
    var folder by remember { mutableStateOf(LetterFolder.entries.find { it.name == client.letters.location.folder } ?: LetterFolder.INBOX) }
    var search by remember { mutableStateOf(client.letters.location.query) }
    var searchAll by remember { mutableStateOf(false) }
    var limit by remember(folder, search) { mutableIntStateOf(100) }
    val boxKind = when (folder) {
        LetterFolder.INBOX -> "inbox"
        LetterFolder.ARCHIVED -> "archive"
        LetterFolder.SENT -> "sent"
        LetterFolder.PENDING -> "pending"
        LetterFolder.DRAFTS -> "draft"
    }
    LaunchedEffect(search, boxKind, searchAll, state.epoch) {
        if (searchAll && search.isNotBlank()) client.search.searchAll(search)
        else if (!searchAll) client.letters.search(search, boxKind)
    }
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
    val active = LocalPageActive.current
    DisposableEffect(owner) { onDispose { client.letters.rememberLocation(location, owner) } }
    LaunchedEffect(state.epoch) { client.safety.loadBlocks(); client.letters.refresh() }
    LaunchedEffect(active, selected, letter?.id) {
        if (active) letter?.takeIf { !it.read }?.let { client.letters.mark(it, read = true) }
    }
    Column(Modifier.fillMaxSize()) {
        if (route.id == null) {
            WorkspaceBrowser(str(R.string.text_letters_cfbff5c), search, { search = it }, str(R.string.text_search_letters_1249ccd),
                actions = {
                    IconButton(onClick = { client.letters.refresh() }) { Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_cba212b)) }
                    MessageMoreMenu(client)
                }) { searching ->
                LaunchedEffect(searching) { if (!searching) searchAll = false }
                if (!searching) ScreenFab {
                    val atTop by remember(listState) { derivedStateOf { listState.firstVisibleItemIndex == 0 } }
                    ExtendedFloatingActionButton(onClick = { editing = client.letters.create() }, expanded = atTop,
                        icon = { Icon(Icons.Outlined.Edit, str(R.string.text_write_a_letter_1a8f9c9)) }, text = { Text(str(R.string.text_write_a_letter_1a8f9c9)) })
                }
                LazyColumn(Modifier.fillMaxSize(), state = listState, contentPadding = pagePadding(top = 0.dp, bottom = FabClearance)) {
                    browseScopesItem(searching) {
                        BrowseScopes(trailing = {
                            if (!searching || !searchAll) MenuChoice(str(folder.label), folder != LetterFolder.INBOX, Modifier.testTag("letter-folders")) { close ->
                                LetterFolder.entries.forEach { entry ->
                                    DropdownMenuItem(
                                        text = { Text(str(entry.label)) },
                                        leadingIcon = { Icon(entry.icon, null) },
                                        trailingIcon = if (entry == folder) ({ Icon(Icons.Outlined.Check, null) }) else null,
                                        onClick = { close(); folder = entry },
                                    )
                                }
                            }
                        }) {
                            if (searching) {
                                BrowseChoice(str(R.string.search_scope_current), !searchAll) { searchAll = false }
                                BrowseChoice(str(R.string.search_scope_all), searchAll) { searchAll = true }
                            }
                            if (!searching || !searchAll) {
                                if (showScopes) {
                                    BrowseChoice(str(R.string.text_conversations_23a085e), false, onClick = client.navigation::back)
                                    BrowseChoice(str(R.string.text_letters_cfbff5c), true) {}
                                } else Text(str(R.string.text_on_this_device_4541a6f), style = MaterialTheme.typography.labelMedium)
                            }
                        }
                    }
                    if (searching && searchAll) {
                        globalSearchResults(client, search, globalRead) { client.search.searchAll(search) }
                    } else if (search.isNotBlank()) {
                        val trimmed = search.trim()
                        item { ReadFeedback(letterSearch, { client.letters.search(search, boxKind) }) }
                        val hits = letterSearch.valueOrNull()?.takeIf { it.boxKind == boxKind && it.query == trimmed }?.hits.orEmpty()
                        items(hits.take(limit), key = { "hit:${it.boxKind}:${it.id}" }) { hit ->
                            val who = hit.displayName?.takeIf(String::isNotBlank) ?: hit.account.ifBlank { str(R.string.text_no_recipient_4b720be) }
                            val statusText = when (hit.boxKind) {
                                "sent", "pending" -> letterState(hit.state) + " · " + messageTime(hit.createdAt)
                                "draft" -> str(R.string.text_draft_ce89527)
                                else -> messageTime(hit.createdAt)
                            }
                            LetterRow(who, "", statusText, !hit.read && hit.boxKind == "inbox", hit.nameSpans, hit.snippetSpans) {
                                when (hit.boxKind) {
                                    "sent", "pending" -> client.navigation.push(Route("inbox", id = hit.id, field = "sent"))
                                    "draft" -> editing = hit.id
                                    else -> client.navigation.push(Route("inbox", id = hit.id))
                                }
                            }
                        }
                        if (hits.isEmpty() && (letterSearch is ReadState.Ready || letterSearch is ReadState.Empty)) {
                            item { EmptyState(str(R.string.no_matches), icon = Icons.Outlined.Search) }
                        }
                        if (hits.size > limit) item { TextButton(onClick = { limit += 100 }) { Text(str(R.string.text_load_more_4e1d9f8)) } }
                    } else when (folder) {
                        LetterFolder.INBOX, LetterFolder.ARCHIVED -> {
                            item { ReadFeedback(blocked, { client.safety.loadBlocks() }); ReadFeedback(incoming, { client.letters.refresh() }) }
                            val rows = incoming.valueOrNull().orEmpty().filter { blocked.valueOrNull()?.contains(it.sender) == false && it.archived == (folder == LetterFolder.ARCHIVED) && (search.isBlank() || it.text.contains(search, true) || it.sender.contains(search, true)) }
                            items(rows.take(limit), key = { it.id }) { row -> LetterRow(row.sender, row.text, messageTime(row.createdAt), !row.read) { client.navigation.push(Route("inbox", id = row.id)) } }
                            if (rows.isEmpty() && blocked.valueOrNull() != null && (incoming is ReadState.Ready || incoming is ReadState.Empty || incoming is ReadState.Stale)) item {
                                EmptyState(str(if (search.isBlank()) R.string.text_no_letters_here_edd0b41 else R.string.no_matches),
                                    glyph = if (search.isBlank()) R.drawable.glyph_inbox else null,
                                    action = if (folder == LetterFolder.INBOX && search.isBlank()) ({
                                        OutlinedButton(onClick = { editing = client.letters.create() }) { Text(str(R.string.text_write_a_letter_1a8f9c9)) }
                                    }) else null)
                            }
                            if (rows.size > limit) item { TextButton(onClick = { limit += 100 }) { Text(str(R.string.text_load_more_4e1d9f8)) } }
                        }
                        LetterFolder.SENT, LetterFolder.PENDING -> {
                            item { ReadFeedback(history, { client.letters.refresh(false) }); Notice(str(R.string.text_sending_history_is_saved_on_this_device_older_records_may_not_con_b7b9992)) }
                            val rows = history.valueOrNull().orEmpty().filter { it.pending == (folder == LetterFolder.PENDING) && (search.isBlank() || it.recipient.contains(search, true) || it.text?.contains(search, true) == true) }
                            items(rows.take(limit), key = { it.id }) { row -> LetterRow(row.recipient, row.text ?: str(R.string.text_message_body_unavailable_6b03555), letterState(row.state) + " · " + messageTime(row.createdAt), false) { client.navigation.push(Route("inbox", id = row.id, field = "sent")) } }
                            if (rows.isEmpty() && (history is ReadState.Ready || history is ReadState.Empty)) item { EmptyState(str(if (search.isBlank()) R.string.text_no_letters_here_edd0b41 else R.string.no_matches), glyph = if (search.isBlank()) R.drawable.glyph_inbox else null) }
                            if (rows.size > limit) item { TextButton(onClick = { limit += 100 }) { Text(str(R.string.text_load_more_4e1d9f8)) } }
                        }
                        LetterFolder.DRAFTS -> {
                            item { ReadFeedback(saved, { client.letters.refresh(false) }) }
                            val rows = drafts.values.filter { it.text.isNotBlank() || it.recipient.isNotBlank() || !it.invitation.isNullOrBlank() }
                                .filter { search.isBlank() || it.text.contains(search, true) || it.recipient.contains(search, true) }.sortedByDescending { it.updatedAt }
                            items(rows.take(limit), key = { it.id }) { row -> LetterRow(row.recipient.ifBlank { str(R.string.text_no_recipient_4b720be) }, row.text, str(R.string.text_draft_ce89527), false) { editing = row.id } }
                            if (rows.isEmpty() && (saved is ReadState.Ready || saved is ReadState.Empty)) item { EmptyState(str(if (search.isBlank()) R.string.text_no_drafts_46c70ab else R.string.no_matches)) }
                            if (rows.size > limit) item { TextButton(onClick = { limit += 100 }) { Text(str(R.string.text_load_more_4e1d9f8)) } }
                        }
                    }
                }
            }
        } else if (letter != null || outgoing != null) Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val account = letter?.sender ?: outgoing!!.recipient
            DetailCard {
                Text(if (letter != null) str(R.string.text_from_c08793d) else str(R.string.text_to_50fd5b7),
                    style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Box(Modifier.weight(1f)) { ParticipantIdentity(client, account, showId = true) }
                    CopyIconButton(str(R.string.text_copy_account_id_faa481e), key = account) { client.clipboard.copy(account) }
                }
                Text(messageTime(letter?.createdAt ?: outgoing?.createdAt),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            DetailCard {
                outgoing?.let { sentLetter ->
                    Text(letterState(sentLetter.state), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                    HorizontalDivider(Modifier.padding(vertical = 4.dp))
                }
                SelectionContainer {
                    Text(
                        letter?.text ?: outgoing?.text ?: str(R.string.text_message_body_unavailable_6b03555),
                        modifier = if (letter != null) Modifier.reminderVisible(client, NoticeTarget("inbox", letter.id), revision = letter) else Modifier,
                        style = MaterialTheme.typography.bodyLarge,
                    )
                }
            }
            if (letter != null) {
                val stackPrimary = LocalDensity.current.fontScale >= 1.4f || letter.archived
                val replyAction: @Composable (Modifier) -> Unit = { buttonModifier ->
                    Button(onClick = { val id = client.letters.create(); client.letters.update(LetterDraft(id = id, recipient = letter.sender)); editing = id },
                        modifier = buttonModifier) {
                        Icon(Icons.AutoMirrored.Outlined.Reply, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_reply_b4c9334))
                    }
                }
                val archiveAction: @Composable (Modifier) -> Unit = { buttonModifier ->
                    OutlinedButton(onClick = { client.letters.mark(letter, archived = !letter.archived) }, modifier = buttonModifier) {
                        Icon(if (letter.archived) Icons.Outlined.Unarchive else Icons.Outlined.Archive, null)
                        Spacer(Modifier.width(8.dp))
                        Text(if (letter.archived) str(R.string.text_move_to_inbox_3b770ba) else str(R.string.text_archive_0f0188e))
                    }
                }
                if (stackPrimary) {
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        replyAction(Modifier.fillMaxWidth())
                        archiveAction(Modifier.fillMaxWidth())
                    }
                } else {
                    Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        replyAction(Modifier.weight(1f).fillMaxHeight())
                        archiveAction(Modifier.weight(1f).fillMaxHeight())
                    }
                }
                Section(footer = str(R.string.text_archiving_does_not_allow_another_letter_receiving_a_letter_grants_245790e)) {
                    ListRow(
                        headline = if (letter.read) str(R.string.text_mark_unread_b10bd76) else str(R.string.text_mark_read_20bb0ec),
                        icon = if (letter.read) Icons.Outlined.MarkEmailUnread else Icons.Outlined.MarkEmailRead,
                    ) { client.letters.mark(letter, read = !letter.read) }
                    ListRow(str(R.string.text_allow_another_letter_fee59fb), icon = Icons.AutoMirrored.Outlined.ForwardToInbox) { reopen = true }
                    ListRow(str(R.string.text_invite_to_a_conversation_e66d6f9), icon = Icons.Outlined.PersonAdd) {
                        client.navigation.push(Route("invite-letter-sender", id = letter.sender))
                    }
                }
                Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                    ListRow(str(R.string.text_report_letter_0730024), icon = Icons.Outlined.Flag, destructive = true) {
                        client.safety.openReport(ReportTarget("inbox", letter.sender, letter.id))
                    }
                    ListRow(str(R.string.text_block_sender_021b756), icon = Icons.Outlined.Block, destructive = true) {
                        client.safety.openBlock(letter.sender)
                    }
                }
            } else if (outgoing != null) {
                when {
                    outgoing.error != null -> Notice(errorText(outgoing.error), true)
                    outgoing.state == "quoted" -> Notice(str(R.string.text_payment_is_required_this_client_has_not_paid_or_delivered_the_let_ed11bd6))
                }
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    if (outgoing.pending) Button(onClick = { client.letters.retry(outgoing.id) },
                        enabled = !state.working("letter:send:" + outgoing.id), modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_retry_original_letter_5a838c3))
                    }
                    OutlinedButton(onClick = { client.letters.check(outgoing.id) },
                        enabled = !state.working("letter:check:" + outgoing.id), modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_check_delivery_status_4b7ae8e))
                    }
                }
                if (outgoing.editable) Section {
                    ListRow(str(R.string.text_return_to_editing_190592d), icon = Icons.Outlined.Edit) {
                        client.letters.edit(outgoing.id) {
                            if (client.navigation.state.value.current == route) { editing = outgoing.id; client.navigation.back() }
                        }
                    }
                }
            }
        } else {
            if (sent == null) ReadFeedback(blocked, client.safety::loadBlocks)
            val source = if (sent == null) incoming else history
            ReadFeedback(source) { client.letters.refresh() }
            if ((sent != null || blocked.valueOrNull() != null) && (source is ReadState.Ready || source is ReadState.Empty || source is ReadState.Stale)) {
                EmptyState(str(R.string.text_message_body_unavailable_6b03555))
            }
        }
    }
    editing?.let { id ->
        val form = drafts[id]
        if (form != null) ModalBottomSheet(onDismissRequest = { editing = null }, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
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
                FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                    TextButton(onClick = { discard = true }) { Text(str(R.string.text_discard_draft_6008f2b), color = MaterialTheme.colorScheme.error) }
                    TextButton(onClick = { editing = null }) { Text(str(R.string.text_keep_draft_89639c8)) }
                }
            }
        }
    }
    if (discard) AlertDialog(onDismissRequest = { discard = false }, title = { Text(str(R.string.text_discard_this_draft_ddde861)) }, confirmButton = { TextButton(onClick = { editing?.let(client.letters::discard); discard = false; editing = null }) { Text(str(R.string.text_discard_475cd3e)) } }, dismissButton = { TextButton(onClick = { discard = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (reopen && letter != null) AlertDialog(onDismissRequest = { reopen = false }, title = { Text(str(R.string.text_allow_another_letter_5538009)) }, text = { Text(str(R.string.text_this_sender_can_submit_another_letter_under_your_current_receivin_0910e86)) }, confirmButton = { TextButton(onClick = { client.letters.mark(letter, reopen = true); reopen = false }) { Text(str(R.string.text_allow_ee9ea51)) } }, dismissButton = { TextButton(onClick = { reopen = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable private fun LetterRow(
    account: String,
    text: String,
    status: String,
    unread: Boolean,
    nameSpans: List<HighlightSpan> = emptyList(),
    snippetSpans: List<HighlightSpan> = emptyList(),
    open: () -> Unit,
) {
    BrowseRow(buildHighlightedText(account, nameSpans), Modifier.clickable(onClick = open), leading = { Avatar(account, account) }, emphasized = unread) {
        Column {
            if (snippetSpans.isNotEmpty()) {
                Text(buildHighlightedText(text, snippetSpans), maxLines = 2, overflow = TextOverflow.Ellipsis, color = if (unread) MaterialTheme.colorScheme.onSurface else Color.Unspecified)
            } else {
                Text(text, maxLines = 1, overflow = TextOverflow.Ellipsis, color = if (unread) MaterialTheme.colorScheme.onSurface else Color.Unspecified)
            }
            Text(status, style = MaterialTheme.typography.labelSmall)
        }
    }
}
@Composable internal fun letterState(state: String): String = when (state) {
    "visible" -> str(R.string.text_sent_baf7943); "queued" -> str(R.string.text_waiting_to_submit_b989b07); "unconfirmed" -> str(R.string.text_waiting_for_confirmation_9c87aad)
    "failed" -> str(R.string.text_send_failed_8c3e812); "aborted", "refunded", "expired" -> str(R.string.text_not_delivered_0a57029)
    "quoted" -> str(R.string.text_payment_required_b449cfe); "admitted", "settling" -> str(R.string.text_processing_97d1ec2)
    else -> str(R.string.text_check_delivery_status_527563c)
}
