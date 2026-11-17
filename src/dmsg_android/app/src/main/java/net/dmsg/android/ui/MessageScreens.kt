package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.quantity
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.automirrored.outlined.Login
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.flow.distinctUntilChanged
import net.dmsg.android.core.*
import net.dmsg.core.MessageItem

@Composable fun MessagesScreen(client: ClientController, state: UiState, selected: String, select: (String) -> Unit, inbox: () -> Unit) {
    val listing by client.messages.channels.state.collectAsStateWithLifecycle()
    val drafts by client.messages.composers.collectAsStateWithLifecycle()
    var search by remember { mutableStateOf("") }
    LaunchedEffect(state.epoch) { client.messages.browse() }
    if (selected.isEmpty()) LazyColumn(Modifier.fillMaxSize(), state = workspaceListState(client.navigation, "messages:list"), contentPadding = PaddingValues(20.dp)) {
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                FilterChip(true, {}, { Text(str(R.string.text_conversations_23a085e)) })
                FilterChip(false, inbox, { Text(str(R.string.text_letters_cfbff5c)) })
                Spacer(Modifier.weight(1f))
                IconButton(onClick = { client.channels.openSetup(false) }) { Icon(Icons.Outlined.Add, str(R.string.text_new_channel_6ea7fcc)) }
                IconButton(onClick = { client.channels.openSetup(true) }) { Icon(Icons.AutoMirrored.Outlined.Login, str(R.string.text_join_5f4ca2d)) }
            }
            WorkspaceSearchField(search, { search = it }, str(R.string.text_search_conversations_554f767))
            TextButton(onClick = { client.navigation.push(Route("message-settings")) }) { Text(str(R.string.text_message_settings_1a28455)) }
            ReadFeedback(listing, client.messages::browse)
        }
        items(listing.valueOrNull().orEmpty().filter { search.isBlank() || it.name.contains(search, true) || it.preview.contains(search, true) }, key = { it.id }) { channel ->
            val draft = drafts[channel.id]?.text ?: channel.draft
            val attachmentCount = drafts[channel.id]?.attachments?.size ?: 0
            ListItem(modifier = Modifier.clickable { select(channel.id) }, headlineContent = { Text(channel.name) }, supportingContent = {
                Column {
                    Text(if (draft.isNotEmpty()) str(R.string.text_draft_4a4143d) + draft else if (attachmentCount > 0) quantity(R.plurals.draft_attachments, attachmentCount) else channel.preview, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (channel.pendingSends > 0u) Text(str(R.string.text_waiting_to_send_93d9311) + channel.pendingSends, style = MaterialTheme.typography.labelSmall)
                }
            }, leadingContent = { Icon(if (channel.news) Icons.Outlined.Campaign else Icons.Outlined.Forum, null, tint = MaterialTheme.colorScheme.primary) }, trailingContent = {
                Column(horizontalAlignment = Alignment.End) {
                    Text(messageTime(channel.lastAt), style = MaterialTheme.typography.labelSmall)
                    if (channel.muted) Icon(Icons.Outlined.NotificationsOff, str(R.string.text_muted_8d0508d), Modifier.size(16.dp))
                    if (channel.unread > 0u) Badge { Text(localizedNumber(channel.unread)) }
                }
            })
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        }
        item { if (listing is ReadState.Empty) Notice(str(R.string.text_start_a_private_channel_or_join_with_an_invitation_no_contacts_ar_ac9dfa2)) }
    } else ConversationScreen(client, state, selected)

}

@Composable private fun ConversationScreen(client: ClientController, state: UiState, channel: String) {
    val channels by client.messages.channels.state.collectAsStateWithLifecycle()
    val blocked by client.safety.blocked.state.collectAsStateWithLifecycle()
    val timeline by client.messages.timeline.state.collectAsStateWithLifecycle()
    val outgoing by client.messages.outbox.state.collectAsStateWithLifecycle()
    val composers by client.messages.composers.collectAsStateWithLifecycle()
    val draftsReady by client.messages.draftsReady.collectAsStateWithLifecycle()
    val files by client.messages.attachmentChoices.state.collectAsStateWithLifecycle()
    val composer = composers[channel] ?: client.messages.composer(channel)
    val info = channels.valueOrNull()?.firstOrNull { it.id == channel }
    var attachment by remember { mutableStateOf(false) }
    var attachmentFolder by remember(channel) { mutableStateOf("root") }
    var attachmentParents by remember(channel) { mutableStateOf(listOf("root")) }
    var search by remember(channel) { mutableStateOf("") }
    var searching by remember(channel) { mutableStateOf(false) }
    var action by remember(channel) { mutableStateOf<MessageItem?>(null) }
    var editText by remember { mutableStateOf<String?>(null) }
    var retract by remember { mutableStateOf(false) }
    val scroll = workspaceListState(client.navigation, "conversation:" + channel)
    val noticeOwner = remember(channel) { java.util.UUID.randomUUID().toString() }
    val noticeView = androidx.compose.ui.platform.LocalView.current
    val noticeFocused = androidx.compose.ui.platform.LocalWindowInfo.current.isWindowFocused
    var noticeActive by remember(channel) { mutableStateOf(true) }
    var visibleThrough by remember(channel) { mutableStateOf<ULong?>(null) }
    DisposableEffect(channel) { onDispose { noticeActive = false; client.notifications.hide(noticeOwner) } }
    val currentTimeline = timeline.valueOrNull()?.takeIf { it.channel == channel && it.query == search }
    val rows = currentTimeline?.items.orEmpty().filter { blocked.valueOrNull()?.contains(it.account) == false }
    LaunchedEffect(channel, state.epoch) { client.safety.loadBlocks(); client.messages.load(channel, search = "") }
    LaunchedEffect(channel, rows, search, noticeFocused, timeline is ReadState.Ready, state.unlocked, state.concealed) {
        if (search.isBlank()) snapshotFlow { scroll.layoutInfo.visibleItemsInfo.mapNotNull { visible -> rows.find { visible.key == "message:" + it.id }?.sequence }.maxOrNull() }
            .distinctUntilChanged().collect { through ->
                if (through != null && noticeFocused && state.unlocked && !state.concealed && timeline is ReadState.Ready) {
                    if (through > (info?.readThrough ?: 0u)) client.messages.markVisible(channel, through)
                    visibleThrough = through
                    if (timeline is ReadState.Ready) client.notifications.observed(NoticeTarget("channel", channel), through, noticeOwner) { noticeActive && noticeView.hasWindowFocus() && visibleThrough != null }
                } else {
                    visibleThrough = null; client.notifications.hide(noticeOwner)
                }
            }
    }
    fun update(value: MessageComposer) = client.messages.edit(channel, value)
    Column(Modifier.fillMaxSize().imePadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(str(R.string.text_encrypted_conversation_9f9e912), Modifier.weight(1f), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            IconButton(onClick = { searching = !searching; if (!searching) { search = ""; client.messages.search("") } }) { Icon(Icons.Outlined.Search, str(R.string.text_search_messages_ee3eb59)) }
            IconButton(onClick = { client.messages.refresh(channel) }) { Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_cba212b)) }
            IconButton(onClick = { client.navigation.push(Route("channel-settings", channel = channel)) }) { Icon(Icons.Outlined.Settings, str(R.string.text_conversation_settings_f07a685)) }
        }
        if (searching) WorkspaceSearchField(search, { search = it; client.messages.search(it) }, str(R.string.text_search_messages_and_attachments_ff5cf7f), Modifier.padding(horizontal = 16.dp))
        ReadFeedback(blocked, { client.safety.loadBlocks() })
        ReadFeedback(timeline, { client.messages.load(channel) })
        LazyColumn(Modifier.weight(1f), state = scroll, contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (currentTimeline?.hasMore == true) item { TextButton(onClick = client.messages::older) { Text(str(R.string.text_load_earlier_messages_1e9b531)) } }
            itemsIndexed(rows, key = { _, message -> "message:" + message.id }) { index, message ->
                val mine = message.account == state.session?.account
                val previous = rows.getOrNull(index - 1)
                val day = messageDay(message.createdAtMs)
                if (previous == null || messageDay(previous.createdAtMs) != day) Box(Modifier.fillMaxWidth().padding(vertical = 8.dp), contentAlignment = Alignment.Center) { Text(day, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                Column(Modifier.fillMaxWidth(), horizontalAlignment = if (mine) Alignment.End else Alignment.Start) {
                    if (message.sequence > (info?.readThrough ?: 0u) && rows.firstOrNull { it.sequence > (info?.readThrough ?: 0u) }?.id == message.id) Text(str(R.string.text_unread_messages_15b52a0), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                    if (previous?.account != message.account || messageDay(previous?.createdAtMs) != day) ParticipantIdentity(client, message.account, avatar = false)
                    Surface(Modifier.widthIn(max = 440.dp).combinedClickable(onClick = {}, onLongClick = { action = message }), shape = RoundedCornerShape(18.dp), color = if (mine) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainer) {
                        Column(Modifier.padding(14.dp)) {
                            message.replySequence?.let { sequence ->
                                TextButton(onClick = { client.navigation.push(Route("message-reference", channel = channel, sequence = sequence)) }) {
                                    Column { Text(str(R.string.text_reply_to_4ac60e6) + sequence, style = MaterialTheme.typography.labelSmall)
                                        val original = rows.find { it.sequence == sequence }
                                        Text(original?.takeIf { !it.unreadable && !it.retracted }?.let { it.text.ifBlank { it.attachmentName.orEmpty() } } ?: str(R.string.text_view_original_message_a5b610b), maxLines = 2, overflow = TextOverflow.Ellipsis) }
                                }
                            }
                            Text(when { message.unreadable -> str(R.string.text_this_message_cannot_be_verified_or_opened_0021683)
                                message.retracted -> str(R.string.text_message_withdrawn_c19a5e2)
                                else -> message.text })
                            Text(messageTime(message.createdAtMs), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            if (message.edited && !message.retracted) Text(str(R.string.text_edited_91efa39), style = MaterialTheme.typography.labelSmall)
                        }
                    }
                    if (message.attachmentName != null && !message.unreadable && !message.retracted) MessageAttachmentRow(client, channel, message)
                }
            }
            item { ReadFeedback(outgoing, { client.messages.load(channel, false) }) }
            items(outgoing.valueOrNull()?.takeIf { it.channel == channel }?.items.orEmpty(), key = { "outgoing:" + it.id }) { queued ->
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.End) {
                    Text(queued.text.ifBlank { queued.attachmentName.orEmpty() }, Modifier.background(MaterialTheme.colorScheme.surfaceContainer, RoundedCornerShape(18.dp)).padding(14.dp))
                    if (queued.text.isNotBlank()) queued.attachmentName?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                    Text(if (queued.error == "EXECUTION_UNKNOWN") str(R.string.text_waiting_for_confirmation_9c87aad) else str(R.string.text_saved_locally_waiting_to_send_7e2484e), style = MaterialTheme.typography.labelSmall)
                    queued.error?.let { Text(errorText(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error) }
                    TextButton(onClick = { client.messages.retry(channel, queued.id) }, enabled = !state.working("message:send:" + queued.id)) { Text(str(R.string.text_retry_original_message_695da75)) }
                }
            }
            if (timeline is ReadState.Empty && outgoing.valueOrNull()?.items.isNullOrEmpty()) item { Notice(str(R.string.text_no_messages_yet_1d23127)) }
        }
        HorizontalDivider()
        composer.reply?.let { reply -> Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(str(R.string.text_reply_2f31ef1) + (reply.preview ?: rows.find { it.sequence == reply.sequence }?.text ?: "#${reply.sequence}"), Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis)
            IconButton(onClick = { update(composer.copy(reply = null)) }) { Icon(Icons.Outlined.Close, str(R.string.text_cancel_reply_b66716c)) }
        } }
        if (composer.attachments.isNotEmpty()) LazyRow(Modifier.fillMaxWidth(), contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(composer.attachments, key = { "${it.file}:${it.version}" }) { file -> InputChip(selected = true, onClick = {}, label = { Text(file.name, Modifier.widthIn(max = 180.dp), maxLines = 2, overflow = TextOverflow.Ellipsis) },
                leadingIcon = { Icon(Icons.Outlined.AttachFile, null) }, trailingIcon = { IconButton(onClick = { update(composer.copy(attachments = composer.attachments.filterNot { it.file == file.file && it.version == file.version })) }) { Icon(Icons.Outlined.Close, str(R.string.text_remove_attachment_d043b7e) + file.name) } }) }
        }
        if (!draftsReady) ReadFeedback(channels, client.messages::browse)
        if (info?.canPublish == false) Notice(str(R.string.text_you_cannot_publish_in_the_current_channel_state_a56d35a))
        Row(Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
            FileIntakeButton(client, channel = channel, enabled = info?.canPublish == true && draftsReady, chooseExisting = { attachmentFolder = "root"; attachmentParents = listOf("root"); client.messages.chooseFolder("root"); attachment = true })
            OutlinedTextField(composer.text, { update(composer.copy(text = it)) }, Modifier.weight(1f).revealFocusedInput(), enabled = draftsReady && info?.canPublish == true, placeholder = { Text(str(R.string.text_say_something_c413667)) }, shape = RoundedCornerShape(24.dp), maxLines = 5)
            FilledIconButton(onClick = { client.messages.send(channel) }, enabled = composer.sendable && !state.working("message:queue:" + channel) && draftsReady && info?.canPublish == true) { Icon(Icons.AutoMirrored.Outlined.Send, str(R.string.text_send_06acd73)) }
        }
    }
    if (attachment) AlertDialog(onDismissRequest = { attachment = false }, title = { Text(str(R.string.text_attach_from_files_d9fc92f)) }, text = {
        Column(Modifier.heightIn(max = 350.dp).verticalScroll(rememberScrollState())) {
            TextButton(onClick = { attachment = false; client.navigation.push(Route("secret-picker", channel = channel)) }) { Icon(Icons.Outlined.Shield, null); Text(str(R.string.text_share_a_secret_snapshot_7502b61)) }
            ReadFeedback(files, { client.messages.chooseFolder(attachmentFolder) })
            if (attachmentParents.size > 1) TextButton(onClick = { attachmentParents = attachmentParents.dropLast(1); attachmentFolder = attachmentParents.last(); client.messages.chooseFolder(attachmentFolder) }) { Text(str(R.string.text_parent_folder_3af9479)) }
            val choices = files.valueOrNull()?.takeIf { it.folder == attachmentFolder }
            choices?.folders.orEmpty().forEach { folder -> TextButton(onClick = { attachmentFolder = folder.id; attachmentParents += folder.id; client.messages.chooseFolder(folder.id) }) { Icon(Icons.Outlined.Folder, null); Text(folder.name) } }
            choices?.files.orEmpty().filter { it.local && it.size <= 100uL * 1024u * 1024u }.forEach { file ->
                val chosen = composer.attachments.any { it.file == file.id && it.version == file.version }
                SelectionRow(file.name, chosen, enabled = chosen || composer.attachments.size < 10) { checked -> if (checked) client.messages.attach(channel, file) else update(composer.copy(attachments = composer.attachments.filterNot { it.file == file.id && it.version == file.version })) }
            }
            if (files is ReadState.Empty) Text(str(R.string.text_import_a_file_in_files_first_d76a295))
        }
    }, confirmButton = { TextButton(onClick = { attachment = false }) { Text(str(R.string.text_close_6cf4a77)) } })
    action?.let { message -> AlertDialog(onDismissRequest = { action = null }, title = { Text(str(R.string.text_message_6302964)) }, text = {
        Column {
            Text(message.text, maxLines = 4, overflow = TextOverflow.Ellipsis)
            TextButton(onClick = { client.clipboard.copy(message.text); action = null }) { Text(str(R.string.text_copy_d6e9651)) }
            TextButton(onClick = { action = null; client.safety.openReport(ReportTarget("channel", message.account, channel, message.sequence)) }, enabled = !message.unreadable) { Text(str(R.string.text_report_message_9354138)) }
            if (message.account != state.session?.account) TextButton(onClick = { action = null; client.safety.openBlock(message.account) }) { Text(str(R.string.text_block_account_11bc76b)) }
            if (!message.unreadable && !message.retracted && info?.canPublish == true) {
                TextButton(onClick = { update(composer.copy(reply = ComposerReply(message.sequence, message.digest, message.text.ifBlank { message.attachmentName.orEmpty() }))); action = null }) { Text(str(R.string.text_reply_b4c9334)) }
                if (message.account == state.session?.account) {
                    TextButton(onClick = { editText = message.text }) { Text(str(R.string.text_edit_message_320c6f1)) }
                    TextButton(onClick = { retract = true }) { Text(str(R.string.text_withdraw_message_aad22ea)) }
                }
            }
        }
    }, confirmButton = { TextButton(onClick = { action = null }) { Text(str(R.string.text_close_6cf4a77)) } }) }
    if ((editText != null || retract) && action != null) AlertDialog(onDismissRequest = { editText = null; retract = false }, title = { Text(if (retract) str(R.string.text_withdraw_this_message_6ad534b) else str(R.string.text_edit_message_320c6f1)) }, text = {
        if (retract) Text(str(R.string.text_copies_already_read_or_saved_cannot_be_recalled_0717a55)) else OutlinedTextField(editText.orEmpty(), { editText = it }, minLines = 3)
    }, confirmButton = { TextButton(onClick = {
        val message = action!!; val actionName = if (retract) "retract" else "edit"; val text = if (retract) "" else editText.orEmpty()
        client.perform({ it.queueMessageAction(channel, actionName, message.sequence, message.digest, text) }, key = "message:action:" + message.id) { client.messages.retry(channel, it); action = null; editText = null; retract = false }
    }, enabled = !state.working("message:action:" + action!!.id)) { Text(str(R.string.text_confirm_563fd5a)) } }, dismissButton = { TextButton(onClick = { editText = null; retract = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

internal fun messageTime(ms: ULong?): String = if (ms == null || ms == 0uL || ms > Long.MAX_VALUE.toULong()) "—" else displayTime(ms)
internal fun messageDay(ms: ULong?): String = if (ms == null || ms == 0uL || ms > Long.MAX_VALUE.toULong()) "—" else java.text.DateFormat.getDateInstance(java.text.DateFormat.MEDIUM).format(java.util.Date(ms.toLong()))

@Composable fun ReferencedMessageScreen(client: ClientController, channel: String, sequence: ULong) {
    val state by client.state.collectAsStateWithLifecycle()
    val reference by client.messages.reference.state.collectAsStateWithLifecycle()
    val blocked by client.safety.blocked.state.collectAsStateWithLifecycle()
    val shown = reference.valueOrNull()?.takeIf { it.channel == channel && it.sequence == sequence }?.message
    LaunchedEffect(channel, sequence, state.epoch) { client.safety.loadBlocks(); client.messages.locate(channel, sequence) }
    DisposableEffect(channel, sequence) { onDispose { client.messages.leaveReference() } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(str(R.string.text_original_message_f73147f) + sequence, style = MaterialTheme.typography.headlineSmall)
        ReadFeedback(reference, { client.messages.locate(channel, sequence) })
        when {
            blocked.valueOrNull() == null -> ReadFeedback(blocked, { client.safety.loadBlocks() })
            shown != null && shown.account in blocked.valueOrNull().orEmpty() -> Notice(str(R.string.text_this_account_is_blocked_on_this_device_26320f6))
            reference is ReadState.Empty -> Notice(str(R.string.text_the_original_message_is_unavailable_in_the_history_you_can_access_7dc55f5))
            shown?.unreadable == true -> Notice(str(R.string.text_this_message_cannot_be_verified_or_opened_0021683), true)
            shown?.retracted == true -> Notice(str(R.string.text_message_withdrawn_c19a5e2))
            shown != null -> {
                ParticipantIdentity(client, shown.account, showId = true)
                Text(messageTime(shown.createdAtMs), style = MaterialTheme.typography.labelSmall)
                SelectionContainer { Text(shown.text) }
                shown.attachmentName?.let { Text(it) }
                if (shown.edited) Text(str(R.string.text_edited_91efa39))
                TextButton(onClick = { client.clipboard.copy(shown.text) }, enabled = shown.text.isNotEmpty()) { Text(str(R.string.text_copy_d6e9651)) }
            }
        }
    }
}
