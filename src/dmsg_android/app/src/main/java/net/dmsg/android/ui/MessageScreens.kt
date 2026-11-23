package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.quantity
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.material.icons.automirrored.outlined.Reply
import androidx.compose.material.icons.automirrored.outlined.Undo
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.flow.distinctUntilChanged
import net.dmsg.android.core.*
import net.dmsg.core.MessageItem

@Composable fun MessagesScreen(client: ClientController, state: UiState, select: (String) -> Unit, inbox: () -> Unit) {
    val listing by client.messages.channels.state.collectAsStateWithLifecycle()
    val drafts by client.messages.composers.collectAsStateWithLifecycle()
    val search by client.messages.directoryQuery.collectAsStateWithLifecycle()
    val matches by client.messages.matches.state.collectAsStateWithLifecycle()
    val revision by client.messages.directoryRevision.collectAsStateWithLifecycle()
    val blocks by client.safety.blocked.state.collectAsStateWithLifecycle()
    val channels = listing.valueOrNull().orEmpty()
    val ids = remember(channels) { channels.map { it.id } }
    val blocked = blocks.valueOrNull()?.takeIf { blocks is ReadState.Ready || blocks is ReadState.Empty }
    val query = search.trim()
    val tooLong = query.codePointCount(0, query.length) > 200
    val matchingChannels = channels.filter { query.isEmpty() || it.name.contains(query, true) }
    LaunchedEffect(state.epoch) { client.messages.browse(); client.safety.loadBlocks() }
    LaunchedEffect(search, ids, blocked, state.epoch, revision) {
        if (blocked != null) client.messages.searchDirectory(search, ids, blocked)
    }
    val scroll = workspaceListState(client.navigation, "messages:list")
    WorkspaceBrowser(str(R.string.text_messages_3a9ab2d), search, {
        if (it != search) scroll.requestScrollToItem(0)
        client.messages.editDirectoryQuery(it)
    },
        str(R.string.text_search_messages_ee3eb59), actions = {
            IconButton(onClick = { client.navigation.push(Route("message-settings")) }) { Icon(Icons.Outlined.MoreVert, str(R.string.text_message_settings_1a28455)) }
        }) { searching ->
        if (!searching) ScreenFab {
            val atTop by remember { derivedStateOf { scroll.firstVisibleItemIndex == 0 } }
            ExtendedFloatingActionButton(onClick = { client.channels.openSetup(false) }, expanded = atTop,
                icon = { Icon(Icons.Outlined.Add, str(R.string.text_new_conversation_8687862)) }, text = { Text(str(R.string.text_new_conversation_8687862)) })
        }
        LazyColumn(Modifier.fillMaxSize(), state = scroll,
            contentPadding = pagePadding(top = 0.dp, bottom = FabClearance)) {
            browseScopesItem(searching) {
                BrowseScopes {
                    BrowseChoice(str(R.string.text_conversations_23a085e), true) {}
                    BrowseChoice(str(R.string.text_letters_cfbff5c), false, onClick = inbox)
                }
            }
            item { ReadFeedback(listing, client.messages::browse) }
            items(matchingChannels, key = { it.id }) { channel ->
                val draft = drafts[channel.id]?.text ?: channel.draft
                val attachmentCount = drafts[channel.id]?.attachments?.size ?: 0
                val unread = channel.unread > 0u
                BrowseRow(channel.name, Modifier.animateItem().clickable { select(channel.id) },
                    leading = { Avatar(channel.name, channel.id, icon = if (channel.news) Icons.Outlined.Campaign else null) },
                    emphasized = unread,
                    supporting = {
                        Column {
                            if (query.isEmpty()) {
                                val drafted = if (draft.isNotEmpty()) draft else if (attachmentCount > 0) quantity(R.plurals.draft_attachments, attachmentCount) else null
                                // A draft reads quieter than the last message; unread previews read louder.
                                if (drafted != null) Text(str(R.string.text_draft_4a4143d) + drafted, maxLines = 1, overflow = TextOverflow.Ellipsis, fontStyle = FontStyle.Italic)
                                else if (channel.preview.isNotEmpty()) Text(channel.preview, maxLines = 1, overflow = TextOverflow.Ellipsis,
                                    color = if (unread) MaterialTheme.colorScheme.onSurface else Color.Unspecified)
                            }
                            if (channel.pendingSends > 0u) Text(str(R.string.text_waiting_to_send_93d9311) + channel.pendingSends)
                        }
                    }, trailing = {
                        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            listTime(channel.lastAt).takeIf { it.isNotEmpty() }?.let { time ->
                                Text(time, style = MaterialTheme.typography.labelSmall, color = if (unread) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                            Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                if (channel.muted) Icon(Icons.Outlined.NotificationsOff, str(R.string.text_muted_8d0508d), Modifier.size(16.dp))
                                if (channel.unread > 0u) Badge { Text(localizedNumber(channel.unread)) }
                            }
                        }
                    })
            }
            if (query.isNotEmpty()) {
                item {
                    Text(str(if (tooLong) R.string.search_query_limit else R.string.search_local_messages), style = MaterialTheme.typography.bodySmall)
                    if (!tooLong) {
                        if (blocked == null) ReadFeedback(blocks, client.safety::loadBlocks)
                        else ReadFeedback(matches, { client.messages.searchDirectory(search, ids, blocked) })
                    }
                }
                if (!tooLong && blocked != null) {
                    matches.valueOrNull().orEmpty().forEach { group ->
                        val channel = channels.firstOrNull { it.id == group.channel } ?: return@forEach
                        val visible = group.items.filter { it.account !in blocked }
                        if (visible.isNotEmpty()) item { SectionTitle(channel.name) }
                        items(visible, key = { "hit:" + group.channel + ":" + it.id }) { hit ->
                            BrowseRow(hit.text.ifBlank { hit.attachmentName.orEmpty() },
                                Modifier.clickable { client.navigation.push(Route("message-reference", channel = group.channel, sequence = hit.sequence)) },
                                leading = { BrowseIcon(Icons.Outlined.Forum) }) {
                                Column {
                                    if (hit.text.isNotBlank() && !hit.attachmentDeleted) hit.attachmentName?.let { Text(it, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                                    Text(messageTime(hit.createdAtMs))
                                }
                            }
                        }
                        if (group.before != null) item {
                            TextButton(onClick = { client.messages.moreMatches(group, blocked) }, enabled = matches !is ReadState.Loading) { Text(str(R.string.text_load_earlier_messages_1e9b531)) }
                        }
                    }
                    if (matchingChannels.isEmpty() && matches is ReadState.Empty && (listing is ReadState.Ready || listing is ReadState.Empty)) item { EmptyState(str(R.string.no_matches)) }
                }
            } else if (listing is ReadState.Empty) item { EmptyState(str(R.string.no_conversations_yet), glyph = R.drawable.glyph_channels, detail =
                str(R.string.text_start_a_private_channel_or_join_with_an_invitation_no_contacts_ar_ac9dfa2)) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable internal fun ConversationScreen(client: ClientController, state: UiState, channel: String, searchMode: BrowseSearchMode, setSearchMode: (BrowseSearchMode) -> Unit) {
    val channels by client.messages.channels.state.collectAsStateWithLifecycle()
    val blocked by client.safety.blocked.state.collectAsStateWithLifecycle()
    val timeline by client.messages.timeline.state.collectAsStateWithLifecycle()
    val outgoing by client.messages.outbox.state.collectAsStateWithLifecycle()
    val syncFailure by client.messages.channelSyncError.collectAsStateWithLifecycle()
    val composers by client.messages.composers.collectAsStateWithLifecycle()
    val draftsReady by client.messages.draftsReady.collectAsStateWithLifecycle()
    val files by client.messages.attachmentChoices.state.collectAsStateWithLifecycle()
    val composer = composers[channel] ?: client.messages.composer(channel)
    val info = channels.valueOrNull()?.firstOrNull { it.id == channel }
    var attachment by remember { mutableStateOf(false) }
    var attachmentFolder by remember(channel) { mutableStateOf("root") }
    var attachmentParents by remember(channel) { mutableStateOf(listOf("root")) }
    var search by remember(channel) { mutableStateOf("") }
    var action by remember(channel) { mutableStateOf<MessageItem?>(null) }
    var editText by remember { mutableStateOf<String?>(null) }
    var retract by remember { mutableStateOf(false) }
    var queuedId by remember(channel) { mutableStateOf<String?>(null) }
    val composerFocus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    val haptics = LocalHapticFeedback.current
    var focusReply by remember(channel) { mutableStateOf(false) }
    val positionKey = "conversation:" + channel
    val savedPosition = remember(channel) { client.navigation.savedScrollPosition(positionKey) }
    var opened by remember(channel) { mutableStateOf(false) }
    val threadScroll = workspaceListState(client.navigation, positionKey)
    val scroll = if (search.isBlank()) threadScroll else key(search) { rememberLazyListState() }
    val noticeOwner = remember(channel) { java.util.UUID.randomUUID().toString() }
    val noticeView = androidx.compose.ui.platform.LocalView.current
    val noticeFocused = androidx.compose.ui.platform.LocalWindowInfo.current.isWindowFocused
    var noticeActive by remember(channel) { mutableStateOf(true) }
    var visibleThrough by remember(channel) { mutableStateOf<ULong?>(null) }
    DisposableEffect(channel) { onDispose { noticeActive = false; client.notifications.hide(noticeOwner) } }
    val currentTimeline = timeline.valueOrNull()?.takeIf { it.channel == channel && it.query == search }
    val blockedAccounts = blocked.valueOrNull()
    val rows = remember(currentTimeline, blockedAccounts) { currentTimeline?.items.orEmpty().filter { blockedAccounts?.contains(it.account) == false } }
    val sequences = remember(rows) { rows.associate { "message:" + it.id to it.sequence } }
    val firstUnread = if (search.isBlank() && info != null && info.unread > 0u) {
        rows.firstOrNull { it.sequence > info.readThrough && it.account != state.session?.account }?.id
    } else null
    val pending = outgoing.valueOrNull()?.takeIf { it.channel == channel }?.items.orEmpty().filter {
        search.isBlank() || it.text.contains(search, true) || it.attachmentName?.contains(search, true) == true
    }
    LaunchedEffect(focusReply) {
        if (focusReply) { composerFocus.requestFocus(); keyboard?.show(); focusReply = false }
    }
    val timelineReady = (timeline is ReadState.Ready || timeline is ReadState.Empty) &&
        (outgoing is ReadState.Ready || outgoing is ReadState.Empty) && blockedAccounts != null && info != null && currentTimeline != null
    LaunchedEffect(queuedId, rows, pending, search, timelineReady) {
        if (!timelineReady || search.isNotBlank() || rows.isEmpty() && pending.isEmpty()) return@LaunchedEffect
        val firstRow = if (currentTimeline?.hasMore == true) 1 else 0
        val id = queuedId
        if (id == null) {
            if (!opened) {
                when {
                    savedPosition != null -> scroll.scrollToItem(savedPosition.first, savedPosition.offset)
                    firstUnread != null -> scroll.scrollToItem(firstRow + rows.indexOfFirst { it.id == firstUnread })
                    else -> scroll.scrollToItem(firstRow + rows.size + pending.size)
                }
                opened = true
            }
            return@LaunchedEffect
        }
        val confirmed = rows.indexOfFirst { it.id == id }
        val queued = pending.indexOfFirst { it.id == id }
        if (confirmed < 0 && queued < 0) return@LaunchedEffect
        // The outbox status occupies one list item between received and pending messages.
        scroll.scrollToItem(firstRow + if (confirmed >= 0) confirmed else rows.size + 1 + queued)
        opened = true
        queuedId = null
    }
    LaunchedEffect(channel, state.epoch) { client.messages.browse(); client.safety.loadBlocks(); client.messages.load(channel, search = "") }
    LaunchedEffect(channel, rows, search, noticeFocused, timeline is ReadState.Ready, state.unlocked, state.concealed) {
        if (search.isNotBlank()) {
            visibleThrough = null
            client.notifications.hide(noticeOwner)
            return@LaunchedEffect
        }
        snapshotFlow { scroll.layoutInfo.visibleItemsInfo.mapNotNull { visible -> sequences[visible.key] }.maxOrNull() }
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
    val syncError = syncFailure?.takeIf { it.first == channel }?.second
    var dismissedError by remember(channel) { mutableStateOf<String?>(null) }
    ScreenBar(info?.name, subtitle = str(R.string.text_encrypted_conversation_9f9e912), leading = { Avatar(info?.name.orEmpty(), channel, size = 36.dp, icon = if (info?.news == true) Icons.Outlined.Campaign else null) }) {
        IconButton(onClick = { setSearchMode(BrowseSearchMode.Editing) }) { Icon(Icons.Outlined.Search, str(R.string.text_search_messages_ee3eb59)) }
        IconButton(onClick = { client.navigation.push(Route("channel-settings", channel = channel)) }) { Icon(Icons.Outlined.MoreVert, str(R.string.text_conversation_settings_f07a685)) }
    }
    Column(Modifier.fillMaxSize().imePadding()) {
        if (searchMode != BrowseSearchMode.Browse) {
            BrowseSearchRow(search, { search = it; client.messages.search(it) }, str(R.string.text_search_messages_and_attachments_ff5cf7f),
                searchMode, setMode = setSearchMode)
        } else syncError?.takeIf { it != dismissedError }?.let { error ->
            Box(Modifier.padding(horizontal = pageMargin(), vertical = 4.dp)) {
                Notice(errorText(error), true) { TextButton(onClick = { dismissedError = error }) { Text(str(R.string.text_close_6cf4a77)) } }
            }
        }
        ReadFeedback(blocked, { client.safety.loadBlocks() })
        ReadFeedback(timeline, { client.messages.load(channel) })
        LazyColumn(Modifier.weight(1f), state = scroll, contentPadding = pagePadding()) {
            if (currentTimeline?.hasMore == true) item { TextButton(onClick = client.messages::older) { Text(str(R.string.text_load_earlier_messages_1e9b531)) } }
            itemsIndexed(rows, key = { _, message -> "message:" + message.id }) { index, message ->
                val mine = message.account == state.session?.account
                val previous = rows.getOrNull(index - 1)
                val next = rows.getOrNull(index + 1)
                val day = messageDay(message.createdAtMs)
                val newDay = previous == null || messageDay(previous.createdAtMs) != day
                // Consecutive messages from one sender on one day form a group: 2 dp apart, inner corners tucked in.
                val joinsPrevious = !newDay && previous?.account == message.account && message.id != firstUnread
                val joinsNext = next != null && next.account == message.account && messageDay(next.createdAtMs) == day && next.id != firstUnread
                Column(Modifier.fillMaxWidth().padding(top = if (joinsPrevious) 2.dp else 12.dp), horizontalAlignment = if (mine) Alignment.End else Alignment.Start) {
                    if (newDay) Box(Modifier.fillMaxWidth().padding(vertical = 8.dp), contentAlignment = Alignment.Center) { Text(day, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    if (message.id == firstUnread) Text(str(R.string.text_unread_messages_15b52a0), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                    if (!joinsPrevious) ParticipantIdentity(client, message.account)
                    Surface(Modifier.padding(start = if (mine) 48.dp else 0.dp, end = if (mine) 0.dp else 48.dp).widthIn(max = 440.dp)
                        .longPressActions(message, str(R.string.message_actions), haptics) { action = message },
                        shape = bubbleShape(mine, joinsPrevious, joinsNext), color = if (mine) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerHighest) {
                        Column(Modifier.padding(horizontal = 14.dp, vertical = 10.dp)) {
                            message.replySequence?.let { sequence ->
                                val original = rows.find { it.sequence == sequence }
                                QuoteBar(Modifier.padding(bottom = 6.dp).clickable { client.navigation.push(Route("message-reference", channel = channel, sequence = sequence)) }) {
                                    Text(str(R.string.text_reply_to_4ac60e6) + sequence, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                                    Text(original?.takeIf { !it.hidden && !it.unreadable && !it.retracted }?.let { it.text.ifBlank { if (it.attachmentDeleted) "" else it.attachmentName.orEmpty() } }?.takeIf(String::isNotBlank) ?: str(R.string.text_view_original_message_a5b610b),
                                        style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                                }
                            }
                            when {
                                message.unreadable -> Text(str(R.string.text_this_message_cannot_be_verified_or_opened_0021683))
                                message.retracted -> Text(str(R.string.text_message_withdrawn_c19a5e2))
                                else -> {
                                    if (message.text.isNotEmpty()) Text(message.text)
                                    if (message.attachmentName != null) {
                                        if (message.text.isNotEmpty()) Spacer(Modifier.height(8.dp))
                                        MessageAttachmentRow(client, channel, message)
                                    }
                                }
                            }
                            // Time and state sit at the bubble's tail.
                            Text(listOfNotNull(str(R.string.text_edited_91efa39).takeIf { message.edited && !message.retracted }, messageClock(message.createdAtMs)).joinToString(" · "),
                                Modifier.align(Alignment.End), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
            item { ReadFeedback(outgoing, { client.messages.load(channel, false) }) }
            items(pending, key = { "outgoing:" + it.id }) { queued ->
                Column(Modifier.fillMaxWidth().padding(top = 12.dp), horizontalAlignment = Alignment.End) {
                    Text(queued.text.ifBlank { queued.attachmentName.orEmpty() }, Modifier.padding(start = 48.dp)
                        .background(MaterialTheme.colorScheme.surfaceContainerHighest, bubbleShape(mine = true, joinsPrevious = false, joinsNext = false)).padding(horizontal = 14.dp, vertical = 10.dp))
                    if (queued.text.isNotBlank()) queued.attachmentName?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                    val sending = state.working("message:send:" + queued.id)
                    val failed = !sending && queued.error != null && queued.error != "EXECUTION_UNKNOWN"
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        if (failed) Icon(Icons.Outlined.ErrorOutline, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.error)
                        Text(str(when {
                            sending -> R.string.message_sending
                            queued.error == "EXECUTION_UNKNOWN" -> R.string.text_waiting_for_confirmation_9c87aad
                            queued.error != null -> R.string.text_send_failed_8c3e812
                            else -> R.string.text_saved_locally_waiting_to_send_7e2484e
                        }), style = MaterialTheme.typography.labelSmall, color = if (failed) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant)
                        if (!sending) TextButton(onClick = { client.messages.retry(channel, queued.id) }) { Text(str(R.string.text_retry_original_message_695da75)) }
                    }
                    // The banner above already names the conversation's sync error; show only a different one here.
                    if (!sending && queued.error != null && queued.error != syncError) Text(errorText(queued.error!!), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                }
            }
            if (currentTimeline != null && (timeline is ReadState.Ready || timeline is ReadState.Empty) && !currentTimeline.hasMore &&
                blockedAccounts != null && (outgoing is ReadState.Ready || outgoing is ReadState.Empty) && rows.isEmpty() && pending.isEmpty()) {
                item { EmptyState(str(if (search.isBlank()) R.string.text_no_messages_yet_1d23127 else R.string.no_matches)) }
            }
        }
        composer.reply?.let { reply -> Row(Modifier.padding(start = pageMargin(), top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            QuoteBar(Modifier.weight(1f)) {
                Text(str(R.string.text_reply_b4c9334), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                Text(reply.preview ?: rows.find { it.sequence == reply.sequence }?.text ?: "#${reply.sequence}", style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            IconButton(onClick = { update(composer.copy(reply = null)) }) { Icon(Icons.Outlined.Close, str(R.string.text_cancel_reply_b66716c)) }
        } }
        if (composer.attachments.isNotEmpty()) LazyRow(Modifier.fillMaxWidth(), contentPadding = PaddingValues(horizontal = pageMargin()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(composer.attachments, key = { "${it.file}:${it.version}" }) { file -> InputChip(selected = true, onClick = {}, label = { Text(file.name, Modifier.widthIn(max = 180.dp), maxLines = 2, overflow = TextOverflow.Ellipsis) },
                leadingIcon = { Icon(Icons.Outlined.AttachFile, null) }, trailingIcon = { IconButton(onClick = { update(composer.copy(attachments = composer.attachments.filterNot { it.file == file.file && it.version == file.version })) }) { Icon(Icons.Outlined.Close, str(R.string.text_remove_attachment_d043b7e) + file.name) } }) }
        }
        if (!draftsReady) ReadFeedback(channels, client.messages::browse)
        if (info?.canPublish == false) Notice(str(R.string.text_you_cannot_publish_in_the_current_channel_state_a56d35a))
        // A filled capsule holds the attachment menu and the text; the send button grows in once there is something to send.
        Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp), verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val writable = draftsReady && info?.canPublish == true
            Surface(Modifier.weight(1f), shape = MaterialTheme.shapes.extraLarge, color = MaterialTheme.colorScheme.surfaceContainerHigh) {
                Row(verticalAlignment = Alignment.Bottom) {
                    // The 48dp button sits 4dp up so its centre meets the last text line, as the send button does.
                    Box(Modifier.padding(start = 4.dp, bottom = 4.dp)) {
                        FileIntakeButton(client, channel = channel, enabled = writable, chooseExisting = { attachmentFolder = "root"; attachmentParents = listOf("root"); client.messages.chooseFolder("root"); attachment = true })
                    }
                    TextField(composer.text, { update(composer.copy(text = it)) }, Modifier.weight(1f).focusRequester(composerFocus).revealFocusedInput(), enabled = writable,
                        placeholder = { Text(str(R.string.text_say_something_c413667)) }, maxLines = 5,
                        colors = TextFieldDefaults.colors(focusedContainerColor = Color.Transparent, unfocusedContainerColor = Color.Transparent, disabledContainerColor = Color.Transparent,
                            focusedIndicatorColor = Color.Transparent, unfocusedIndicatorColor = Color.Transparent, disabledIndicatorColor = Color.Transparent))
                }
            }
            val sendable = composer.sendable && !state.working("message:queue:" + channel) && writable
            val scale by animateFloatAsState(if (sendable) 1f else 0.82f, Motion.spatial(), label = "send")
            FilledIconButton(onClick = { client.messages.send(channel) { id -> search = ""; setSearchMode(BrowseSearchMode.Browse); queuedId = id } }, enabled = sendable,
                modifier = Modifier.padding(bottom = 4.dp).graphicsLayer { scaleX = scale; scaleY = scale }) { Icon(Icons.AutoMirrored.Outlined.Send, str(R.string.text_send_06acd73)) }
        }
    }
    if (attachment) AlertDialog(onDismissRequest = { attachment = false }, title = { Text(str(R.string.text_attach_from_files_d9fc92f)) }, text = {
        Column(Modifier.heightIn(max = 350.dp).verticalScroll(rememberScrollState())) {
            TextButton(onClick = { attachment = false; client.secretSharing.picker.reset(); client.navigation.push(Route("secret-picker", channel = channel)) }) { Icon(Icons.Outlined.Shield, null); Text(str(R.string.text_share_a_secret_snapshot_7502b61)) }
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
    action?.takeIf { editText == null && !retract }?.let { message -> ModalBottomSheet(onDismissRequest = { action = null }) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(bottom = 16.dp)) {
            if (message.text.isNotEmpty() && !message.unreadable && !message.retracted) Text(message.text, Modifier.padding(horizontal = pageMargin()).padding(bottom = 8.dp),
                style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 3, overflow = TextOverflow.Ellipsis)
            if (!message.unreadable && !message.retracted && info?.canPublish == true) SheetAction(str(R.string.text_reply_b4c9334), Icons.AutoMirrored.Outlined.Reply) {
                update(composer.copy(reply = ComposerReply(message.sequence, message.digest, message.text.ifBlank { message.attachmentName.orEmpty() }))); action = null; focusReply = true
            }
            if (message.text.isNotEmpty() && !message.unreadable && !message.retracted) SheetAction(str(R.string.text_copy_d6e9651), Icons.Outlined.ContentCopy) {
                client.clipboard.copy(message.text); action = null
            }
            if (!message.unreadable && !message.retracted && info?.canPublish == true && message.account == state.session?.account) {
                if (message.text.isNotEmpty()) SheetAction(str(R.string.text_edit_message_320c6f1), Icons.Outlined.Edit) { editText = message.text }
                SheetAction(str(R.string.text_withdraw_message_aad22ea), Icons.AutoMirrored.Outlined.Undo) { retract = true }
            }
            if (!message.unreadable) SheetAction(str(R.string.text_report_message_9354138), Icons.Outlined.Flag) {
                action = null; client.safety.openReport(ReportTarget("channel", message.account, channel, message.sequence))
            }
            if (message.account != state.session?.account) SheetAction(str(R.string.text_block_account_11bc76b), Icons.Outlined.Block, destructive = true) {
                action = null; client.safety.openBlock(message.account)
            }
        }
    } }
    if ((editText != null || retract) && action != null) AlertDialog(onDismissRequest = { editText = null; retract = false }, title = { Text(if (retract) str(R.string.text_withdraw_this_message_6ad534b) else str(R.string.text_edit_message_320c6f1)) }, text = {
        if (retract) Text(str(R.string.text_copies_already_read_or_saved_cannot_be_recalled_0717a55)) else {
            val edit = MessageComposer(editText.orEmpty())
            OutlinedTextField(edit.text, { editText = it }, minLines = 3, isError = edit.tooLong, supportingText = {
                Text(localizedNumber(edit.bytes) + " / " + localizedNumber(MessageComposer.MAX_TEXT_BYTES) + " " + str(R.string.text_utf_8_bytes_121f5ad))
            })
        }
    }, confirmButton = { TextButton(onClick = {
        val message = action!!; val actionName = if (retract) "retract" else "edit"; val text = if (retract) "" else editText.orEmpty()
        client.perform({ it.queueMessageAction(channel, actionName, message.sequence, message.digest, text) }, key = "message:action:" + message.id) { client.messages.retry(channel, it); action = null; editText = null; retract = false }
    }, enabled = !state.working("message:action:" + action!!.id) && (retract || (editText != action!!.text && MessageComposer(editText.orEmpty()).sendable))) { Text(str(R.string.text_confirm_563fd5a)) } }, dismissButton = { TextButton(onClick = { editText = null; retract = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

internal fun messageTime(ms: ULong?): String = if (ms == null || ms == 0uL || ms > Long.MAX_VALUE.toULong()) "—" else displayTime(ms)
internal fun messageClock(ms: ULong): String = if (ms == 0uL || ms > Long.MAX_VALUE.toULong()) "—" else
    java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT, net.dmsg.android.platform.AppLanguage.locale()).format(java.util.Date(ms.toLong()))
internal fun messageDay(ms: ULong?): String = if (ms == null || ms == 0uL || ms > Long.MAX_VALUE.toULong()) "—" else java.text.DateFormat.getDateInstance(java.text.DateFormat.MEDIUM).format(java.util.Date(ms.toLong()))

@Composable fun ReferencedMessageScreen(client: ClientController, channel: String, sequence: ULong) {
    val state by client.state.collectAsStateWithLifecycle()
    val reference by client.messages.reference.state.collectAsStateWithLifecycle()
    val blocked by client.safety.blocked.state.collectAsStateWithLifecycle()
    val shown = reference.valueOrNull()?.takeIf { it.channel == channel && it.sequence == sequence }?.message
    LaunchedEffect(channel, sequence, state.epoch) { client.safety.loadBlocks(); client.messages.locate(channel, sequence) }
    DisposableEffect(channel, sequence) { onDispose { client.messages.leaveReference() } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(str(R.string.text_original_message_f73147f) + sequence, style = MaterialTheme.typography.headlineSmall)
        ReadFeedback(reference, { client.messages.locate(channel, sequence) })
        when {
            blocked.valueOrNull() == null -> ReadFeedback(blocked, { client.safety.loadBlocks() })
            shown != null && shown.account in blocked.valueOrNull().orEmpty() -> Notice(str(R.string.text_this_account_is_blocked_on_this_device_26320f6))
            reference is ReadState.Empty -> Notice(str(R.string.text_the_original_message_is_unavailable_in_the_history_you_can_access_7dc55f5))
            shown?.hidden == true -> Notice(str(R.string.text_the_original_message_is_unavailable_in_the_history_you_can_access_7dc55f5))
            shown?.unreadable == true -> Notice(str(R.string.text_this_message_cannot_be_verified_or_opened_0021683), true)
            shown?.retracted == true -> Notice(str(R.string.text_message_withdrawn_c19a5e2))
            shown != null -> {
                ParticipantIdentity(client, shown.account, showId = true)
                Text(messageTime(shown.createdAtMs), style = MaterialTheme.typography.labelSmall)
                SelectionContainer { Text(shown.text) }
                if (!shown.attachmentDeleted) shown.attachmentName?.let { Text(it) }
                if (shown.edited) Text(str(R.string.text_edited_91efa39))
                TextButton(onClick = { client.clipboard.copy(shown.text) }, enabled = shown.text.isNotEmpty()) { Text(str(R.string.text_copy_d6e9651)) }
            }
        }
    }
}

/** Today's clock time, a weekday within the last week, or a short date; empty when there is no time. */
internal fun listTime(ms: ULong?, now: Long = System.currentTimeMillis()): String {
    if (ms == null || ms == 0uL || ms > Long.MAX_VALUE.toULong()) return ""
    val zone = java.time.ZoneId.systemDefault()
    val date = java.time.Instant.ofEpochMilli(ms.toLong()).atZone(zone).toLocalDate()
    val today = java.time.Instant.ofEpochMilli(now).atZone(zone).toLocalDate()
    val days = java.time.temporal.ChronoUnit.DAYS.between(date, today)
    val skeleton = when {
        days <= 0L -> "jm"
        days < 7L -> "EEE"
        date.year == today.year -> "MMMd"
        else -> "yMMMd"
    }
    return android.icu.text.DateFormat.getInstanceForSkeleton(skeleton, net.dmsg.android.platform.AppLanguage.locale()).format(java.util.Date(ms.toLong()))
}

/** A quoted message: a short accent bar at the start, then the quote. */
@Composable internal fun QuoteBar(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Row(modifier.height(IntrinsicSize.Min)) {
        Box(Modifier.width(3.dp).fillMaxHeight().background(MaterialTheme.colorScheme.primary, MaterialTheme.shapes.extraSmall))
        Column(Modifier.padding(start = 8.dp), content = content)
    }
}

/** A message's actions open on a long press; a tap does nothing, so reading never opens a menu by accident. */
private fun Modifier.longPressActions(key: Any, label: String, haptics: HapticFeedback, open: () -> Unit): Modifier =
    pointerInput(key) { detectTapGestures(onLongPress = { haptics.performHapticFeedback(HapticFeedbackType.LongPress); open() }) }
        .semantics { onLongClick(label) { open(); true } }

@Composable private fun SheetAction(label: String, icon: ImageVector, destructive: Boolean = false, onClick: () -> Unit) {
    val color = if (destructive) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface
    ListItem(headlineContent = { Text(label, color = color) }, leadingContent = { Icon(icon, null, tint = color) },
        colors = ListItemDefaults.colors(containerColor = Color.Transparent), modifier = Modifier.clickable(role = Role.Button, onClick = onClick))
}
