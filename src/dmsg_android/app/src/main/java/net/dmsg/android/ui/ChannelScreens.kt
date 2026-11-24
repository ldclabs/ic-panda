package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import android.content.Intent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ExitToApp
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun ChannelSettingsScreen(client: ClientController, channel: String) {
    val state by client.state.collectAsStateWithLifecycle()
    val loaded by client.channels.review.state.collectAsStateWithLifecycle()
    val names by client.channels.names.collectAsStateWithLifecycle()
    val changes by client.channels.changes.collectAsStateWithLifecycle()
    val review = loaded.valueOrNull()?.takeIf { it.details.id == channel }
    val detail = review?.details
    val name = names[channel] ?: detail?.name.orEmpty()
    val mine = detail?.member(state.session?.account)
    val pending = changes[channel]
    val working = state.working("channel:control:$channel") || state.working("channel:settings:$channel")
    var action by remember(channel) { mutableStateOf<ChannelAction?>(null) }
    var managing by remember(channel) { mutableStateOf<ChannelMember?>(null) }
    var leaveDraft by remember(channel) { mutableStateOf(false) }
    LaunchedEffect(channel, state.epoch) { client.channels.load(channel) }
    BackHandler(LocalPageActive.current && detail != null && name != detail.name) { leaveDraft = true }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                ReadFeedback(loaded, { client.channels.load(channel) })
                detail?.let { info ->
                    when {
                        info.status == "archived" -> Notice(str(R.string.text_the_owner_closed_this_conversation_history_remains_readable_this_bdb050e))
                        info.left -> Notice(str(R.string.text_you_left_this_conversation_the_cached_membership_is_historical_sh_252342d))
                        info.status == "rotation_required" -> Notice(str(R.string.text_membership_change_recorded_sending_is_paused_until_the_new_conver_ce3b7ac)) {
                            TextButton(onClick = { client.messages.refresh(channel); client.channels.load(channel) }) { Text(str(R.string.text_check_key_update_4fc68c6)) }
                        }
                        info.status == null -> Notice(str(R.string.text_the_control_log_is_not_available_yet_refresh_before_changing_memb_d1e7415))
                    }
                }
                DetailCard {
                    detail?.let { info ->
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Avatar(info.name, channel, size = 32.dp,
                                icon = if (info.kind == "distribution") rememberVectorPainter(Icons.Outlined.Campaign) else null)
                            Text(channelType(info.kind), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                        }
                    }
                    OutlinedTextField(name, { client.channels.editName(channel, it) }, Modifier.fillMaxWidth().revealFocusedInput(),
                        label = { Text(str(R.string.text_conversation_name_428c56e)) }, singleLine = true, enabled = detail != null && !working,
                        supportingText = { Text(localizedNumber(name.codePointCount(0, name.length)) + " / " + localizedNumber(100)) },
                        isError = name.isNotEmpty() && !validChannelName(name))
                    if (detail != null && name != detail.name) {
                        Button(onClick = { client.channels.saveName(channel) }, enabled = validChannelName(name) && !working,
                            modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_save_name_93b4376)) }
                    }
                }
            }
        }
        detail?.let { info ->
            item {
                Section(
                    title = str(R.string.text_notifications_3e7769a),
                    footer = str(R.string.text_hiding_keeps_your_membership_and_history_find_it_again_in_hidden_8871afa),
                    iconColor = MaterialTheme.colorScheme.secondaryContainer,
                ) {
                    val muteSummary = listOfNotNull(
                        info.mutedUntil?.let { str(R.string.text_muted_until_50fbca1) + displayTime(it) },
                        str(R.string.text_notification_preference_awaits_synchronization_2c34eab).takeIf { info.notificationPending },
                    ).joinToString("\n").ifEmpty { null }
                    SettingToggle(str(R.string.text_mute_notifications_005e602), info.muted, { value -> client.channels.mute(channel, value) },
                        enabled = !working, description = muteSummary, icon = Icons.Outlined.NotificationsOff)
                    ListItem(
                        headlineContent = {
                            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                listOf(15 to str(R.string.text_15_minutes_b81c469), 60 to str(R.string.text_1_hour_0ac0bd6)).forEach { (minutes, label) ->
                                    AssistChip(onClick = { client.channels.mute(channel, true, (System.currentTimeMillis() + minutes * 60_000L).toULong()) },
                                        enabled = !working, label = { Text(label) })
                                }
                                AssistChip(onClick = {
                                    val tomorrow = java.util.Calendar.getInstance().apply {
                                        add(java.util.Calendar.DAY_OF_YEAR, 1); set(java.util.Calendar.HOUR_OF_DAY, 9)
                                        set(java.util.Calendar.MINUTE, 0); set(java.util.Calendar.SECOND, 0); set(java.util.Calendar.MILLISECOND, 0)
                                    }
                                    client.channels.mute(channel, true, tomorrow.timeInMillis.toULong())
                                }, enabled = !working, label = { Text(str(R.string.text_until_tomorrow_4bfab87)) })
                            }
                        },
                        modifier = Modifier.clipTo(rowShape()),
                    )
                    if (info.notificationConflict) ListRow(str(R.string.text_retry_this_notification_preference_aad4fbd),
                        icon = Icons.Outlined.Refresh, enabled = !working) { client.channels.mute(channel, info.muted, info.mutedUntil) }
                    SettingToggle(str(R.string.text_hide_from_conversation_list_f37326b), info.hidden,
                        { value -> client.channels.hide(channel, value) }, enabled = !working, icon = Icons.Outlined.VisibilityOff)
                }
            }
            item { SectionTitle(str(R.string.text_members_437a10e) + " · " + localizedNumber(info.members.size)) }
            items(info.members, key = { it.account }) { member ->
                val otherMember = !info.left && info.status != "archived" && member.account != state.session?.account && pending == null
                val manageable = otherMember && (mine?.role == "owner" || info.canRemove(state.session?.account, member))
                val meta = buildList {
                    add(channelRole(member.role) + if (member.canRotate) str(R.string.text_may_update_keys_df6de73) else "")
                    add(str(R.string.text_joined_at_key_epoch_0f80f11) + member.joinedEpoch)
                    if (member.history.isNotEmpty()) add(str(R.string.text_granted_older_history_c0fd324) + member.history.joinToString { "${it.first}–${it.second}" })
                }.joinToString("\n")
                Section {
                    ListItem(
                        headlineContent = { ParticipantIdentity(client, member.account, showId = true) },
                        supportingContent = { Text(meta, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) },
                        trailingContent = if (manageable) { {
                            IconButton(onClick = { managing = member }, enabled = !working) {
                                Icon(Icons.Outlined.MoreVert, str(R.string.more) + " · " + member.account)
                            }
                        } } else null,
                        modifier = Modifier.clipTo(rowShape()),
                    )
                }
            }
            item {
                Section(footer = str(R.string.text_new_members_may_read_the_current_key_epoch_including_earlier_mess_8fd599a)) {
                    if (info.canManage(state.session?.account)) {
                        ListRow(str(R.string.text_invite_an_account_96d0472), icon = Icons.Outlined.PersonAdd, enabled = !working) {
                            client.navigation.push(Route("channel-invite", channel = channel))
                        }
                    }
                }
            }
            item {
                Section(title = str(R.string.text_details_8b33589)) {
                    InfoRow(
                        str(R.string.text_readable_key_epochs_on_this_device_2c98a15).trimEnd('·', ':', '：', ' '),
                        info.readableEpochs.joinToString().ifEmpty { "—" },
                    )
                    if (!info.left && info.status != "archived" && mine != null && pending == null &&
                        (mine.role in setOf("owner", "admin") || mine.canRotate)) {
                        ListRow(str(R.string.text_update_conversation_keys_66bc367), icon = Icons.Outlined.Key, enabled = !working) {
                            action = ChannelAction("rekey")
                        }
                    }
                    CodeRow(str(R.string.text_copy_conversation_id_0defedf), channel) { client.clipboard.copy(channel) }
                }
            }
            if (pending != null || review.pending.isNotEmpty()) item {
                DetailCard {
                    if (pending != null) {
                        OutlinedButton(onClick = { client.channels.change(channel, pending.action) }, enabled = !working,
                            modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_retry_original_change_670ac66)) }
                    }
                    review.pending.forEach { operation ->
                        val opName = operation.action.substringAfterLast('/').takeIf { it != "v1" } ?: operation.key.substringBefore(':')
                        CodeRow(opName, operation.request)
                        TextButton(onClick = { client.channels.resume(channel, operation.key) }, enabled = !working) {
                            Text(str(R.string.text_check_original_result_30dd950))
                        }
                    }
                }
            }
            if (!info.left && info.status != "archived" && mine != null && pending == null) item {
                Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                    ListRow(
                        headline = if (mine.role == "owner") str(R.string.text_archive_conversation_c2331b7) else str(R.string.text_leave_conversation_fc5eb24),
                        icon = if (mine.role == "owner") Icons.Outlined.Archive else Icons.AutoMirrored.Outlined.ExitToApp,
                        destructive = true,
                        enabled = !working,
                    ) { action = ChannelAction(if (mine.role == "owner") "archive" else "leave") }
                }
            }
        }
    }
    managing?.let { selectedMember ->
        val member = detail?.members?.find { it.account == selectedMember.account }
        if (member == null || detail.left || detail.status == "archived" || pending != null) {
            managing = null
        } else {
            val isOwner = mine?.role == "owner"
            val canRemove = detail.canRemove(state.session?.account, member)
            ModalBottomSheet(onDismissRequest = { managing = null }) {
                Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = pageMargin()).padding(bottom = 20.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    ParticipantIdentity(client, member.account, showId = true)
                    if (isOwner) {
                        Section(title = str(R.string.text_role_b9e0249)) {
                            listOf("admin", "publisher", "member").forEach { role ->
                                RadioChoiceRow(channelRole(role), member.role == role, enabled = !working) {
                                    if (role != member.role) {
                                        managing = null
                                        action = ChannelAction("role", member.account, role, member.canRotate)
                                    }
                                }
                            }
                        }
                        Section {
                            SettingToggle(str(R.string.text_allow_key_updates_b8aee14), member.canRotate, { allow ->
                                managing = null
                                action = ChannelAction("role", member.account, member.role, allow)
                            }, enabled = !working, icon = Icons.Outlined.Key)
                        }
                    }
                    if (canRemove) {
                        Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                            ListRow(str(R.string.text_remove_member_9443c64), icon = Icons.Outlined.PersonRemove, destructive = true, enabled = !working) {
                                managing = null
                                action = ChannelAction("remove", member.account)
                            }
                        }
                    }
                }
            }
        }
    }
    action?.let { chosen -> AlertDialog(onDismissRequest = { action = null }, title = { Text(channelActionTitle(chosen.action)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            chosen.account?.let { ParticipantIdentity(client, it, true) }
            chosen.role?.let { Text(channelRole(it) + if (chosen.canRotate) str(R.string.text_may_update_keys_df6de73) else "") }
            Text(when (chosen.action) {
                "archive" -> str(R.string.text_this_closes_the_conversation_for_all_members_existing_history_sta_7f4f973)
                "leave", "remove", "rekey" -> str(R.string.text_future_messages_wait_for_new_keys_this_does_not_erase_content_alr_4e7c4a7)
                else -> str(R.string.text_apply_these_permissions_to_this_exact_account_6bf3eaa)
            })
        }
    }, confirmButton = { TextButton(onClick = { action = null; client.channels.change(channel, chosen) }) { Text(str(R.string.text_confirm_change_595ac7f)) } }, dismissButton = { TextButton(onClick = { action = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
    if (leaveDraft) AlertDialog(onDismissRequest = { leaveDraft = false }, title = { Text(str(R.string.text_keep_this_name_draft_9beac5b)) }, text = { Text(str(R.string.text_it_stays_in_this_unlocked_session_until_saved_or_locked_24805d7)) }, confirmButton = { TextButton(onClick = { leaveDraft = false; client.navigation.back() }) { Text(str(R.string.text_keep_and_leave_fec37fb)) } }, dismissButton = { TextButton(onClick = { leaveDraft = false }) { Text(str(R.string.text_keep_editing_4d9eb3d)) } })
}

@Composable fun HiddenChannelsScreen(client: ClientController) {
    val hidden by client.channels.hidden.state.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { client.channels.browseHidden() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            ReadFeedback(hidden, client.channels::browseHidden)
            if (hidden is ReadState.Empty) EmptyState(str(R.string.text_no_hidden_conversations_1471673), icon = Icons.Outlined.VisibilityOff)
        }
        items(hidden.valueOrNull().orEmpty(), key = { it.id }) { channel ->
            Section {
                ListItem(
                    headlineContent = { Text(channel.name) },
                    supportingContent = { Text(channelType(channel.kind)) },
                    leadingContent = { Avatar(channel.name, channel.id) },
                    trailingContent = {
                        TextButton(onClick = { client.channels.hide(channel.id, false) }) { Text(str(R.string.text_show_again_1409301)) }
                    },
                )
            }
        }
    }
}

@Composable fun ChannelSetupScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    val form by client.channels.setup.collectAsStateWithLifecycle()
    val creations by client.channels.creations.state.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    val summary = remember(form.invitation) { ChannelInvitation.parse(form.invitation) }
    var editingInvitation by remember { mutableStateOf(false) }
    val working = state.working("channel:setup")
    var now by remember { mutableStateOf(System.currentTimeMillis().toULong()) }
    LaunchedEffect(Unit) { client.channels.loadCreations(); while (true) { kotlinx.coroutines.delay(1000); now = System.currentTimeMillis().toULong() } }
    LaunchedEffect(qr.result) { client.qr.consume("channel-join")?.let { client.channels.editSetup(form.copy(joining = true, invitation = it)) } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        SegmentedChoice(listOf(str(R.string.text_create_7570cfc), str(R.string.text_join_5f4ca2d)), if (form.joining) 1 else 0, enabled = !working) { index ->
            client.channels.editSetup(form.copy(joining = index == 1))
        }
        val formError = form.error
        val banner = when {
            formError != null -> errorText(formError) to true
            form.joining && summary != null && summary.expiresAt <= now -> str(R.string.text_this_invitation_has_expired_ask_for_a_new_invitation_d58d62e) to true
            form.joining && summary != null && summary.account != state.session?.account -> str(R.string.text_switch_to_the_invited_account_before_joining_f24b50c) to false
            form.joining && summary == null && form.invitation.isNotEmpty() -> str(R.string.text_the_invitation_is_invalid_0374805) to true
            else -> null
        }
        banner?.let { (message, warning) -> Notice(message, warning) }
        if (form.joining) {
            DetailCard {
                if (summary == null || editingInvitation) {
                    OutlinedTextField(form.invitation, { client.channels.editSetup(form.copy(invitation = it)) }, Modifier.fillMaxWidth().revealFocusedInput(),
                        label = { Text(str(R.string.text_paste_invitation_0989300)) }, minLines = 3, maxLines = 4, enabled = !working)
                    if (summary != null) TextButton(onClick = { editingInvitation = false }) { Text(str(R.string.text_review_invitation_0f9ebe4)) }
                } else Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.Link, null, tint = MaterialTheme.colorScheme.primary)
                    Text(str(R.string.text_invitation_loaded_eebc00b), Modifier.weight(1f).padding(start = 8.dp))
                    TextButton(onClick = { editingInvitation = true }, enabled = !working) { Text(str(R.string.text_change_c6b27c7)) }
                }
                OutlinedButton(onClick = { client.qr.open("channel-join") }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.QrCodeScanner, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_scan_invitation_27f0014))
                }
            }
            if (summary != null) {
                DetailCard {
                    Text(summary.name, style = MaterialTheme.typography.titleMedium)
                    ParticipantIdentity(client, summary.account, showId = true)
                    LabelValue(str(R.string.text_expires_1efbbe3).trimEnd(' ', '·', ':'), displayTime(summary.expiresAt))
                    Text(str(R.string.text_these_details_come_from_the_invitation_choosing_join_verifies_the_1fe792b),
                        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        } else {
            DetailCard {
                OutlinedTextField(form.name, { client.channels.editSetup(form.copy(name = it)) }, Modifier.fillMaxWidth().revealFocusedInput(),
                    label = { Text(str(R.string.text_conversation_name_428c56e)) }, enabled = !working && form.request == null, singleLine = true,
                    isError = form.name.isNotEmpty() && !validChannelName(form.name))
            }
            Section(footer = str(R.string.text_after_creation_invite_an_account_to_start_the_conversation_b389453)) {
                listOf("direct" to R.string.text_a_conversation_for_two_accounts_7ba9e18,
                    "collaboration" to R.string.text_every_member_can_send_messages_020f52c,
                    "distribution" to R.string.text_only_the_owner_administrators_and_publishers_can_send_messages_6811c29).forEach { (kind, description) ->
                    RadioChoiceRow(channelType(kind), form.kind == kind, enabled = !working && form.request == null,
                        subtitle = str(description)) { client.channels.editSetup(form.copy(kind = kind)) }
                }
            }
        }
        Button(
            onClick = client.channels::completeSetup,
            enabled = !working && if (form.joining) summary != null && summary.account == state.session?.account && summary.expiresAt > now else validChannelName(form.name),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(if (form.joining) str(R.string.text_join_conversation_2b8512c) else if (form.request != null) str(R.string.text_retry_original_creation_bc1e0df) else str(R.string.text_create_conversation_e2ac99b))
        }
        if (working) {
            LinearProgressIndicator(Modifier.fillMaxWidth())
            TextButton(onClick = { client.cancelOperation("channel:setup"); client.channels.loadCreations() }) { Text(str(R.string.text_stop_waiting_be4e25a)) }
        }
        ReadFeedback(creations, client.channels::loadCreations)
        creations.valueOrNull().orEmpty().forEach { pending ->
            DetailCard {
                Text(pending.name, style = MaterialTheme.typography.titleMedium)
                Text(channelType(pending.kind), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = { client.channels.create(pending) }, enabled = !working) { Text(str(R.string.text_continue_original_creation_631000f)) }
                    TextButton(onClick = { client.channels.discard(pending) }, enabled = !working) {
                        Text(str(R.string.text_discard_if_not_yet_signed_7c61b1f), color = MaterialTheme.colorScheme.error)
                    }
                }
            }
        }
        Text(str(R.string.text_form_drafts_stay_only_in_this_unlocked_session_started_creations_d6a8bc0),
            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable fun ChannelInviteScreen(client: ClientController, channel: String) {
    val state by client.state.collectAsStateWithLifecycle()
    val forms by client.channels.invites.collectAsStateWithLifecycle()
    val previous by client.channels.invitationAttempts.state.collectAsStateWithLifecycle()
    LaunchedEffect(channel, state.epoch) { client.channels.loadInvites(channel) }
    val form = forms[channel] ?: ChannelInvite()
    val working = state.working("channel:invite:$channel")
    val context = LocalContext.current
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        form.error?.let { Notice(errorText(it), true) }
        DetailCard {
            RecipientField(client, form.account, { client.channels.editInvite(channel, form.copy(account = it)) }, enabled = !working && form.request == null, label = str(R.string.text_member_account_id_73d9865))
            Text(str(R.string.text_only_this_account_can_accept_the_invitation_it_may_read_the_curre_7898d3d),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (form.link == null) {
                Button(onClick = { client.channels.invite(channel) }, enabled = validAccount(form.account) && !working, modifier = Modifier.fillMaxWidth()) {
                    Text(if (form.request == null) str(R.string.text_create_invitation_bb66682) else str(R.string.text_retry_original_invitation_2751ed3))
                }
            }
            if (working) LinearProgressIndicator(Modifier.fillMaxWidth())
        }
        form.link?.let { link ->
            DetailCard {
                Text(str(R.string.text_invitation_ready_fe53899), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                ChannelInvitation.parse(link)?.let {
                    LabelValue(str(R.string.text_expires_1efbbe3).trimEnd(' ', '·', ':'), displayTime(it.expiresAt))
                }
                QrCode(link)
                Button(
                    onClick = { client.beginExternalInteraction(); context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, link) }, null)) },
                    modifier = Modifier.fillMaxWidth(),
                ) { Icon(Icons.Outlined.Share, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_share_invitation_c716745)) }
                CopyButton(str(R.string.text_copy_invitation_6a08c36), outlined = true) { client.clipboard.copy(link) }
                TextButton(onClick = { client.channels.editInvite(channel, ChannelInvite()) }) { Text(str(R.string.text_invite_another_account_67d5fa8)) }
            }
        }
        ReadFeedback(previous, { client.channels.loadInvites(channel) })
        previous.valueOrNull()?.takeIf { it.channel == channel }?.items.orEmpty().filter { it.request != form.request }.forEach { attempt ->
            DetailCard {
                ParticipantIdentity(client, attempt.account, true)
                Text(str(R.string.text_expires_1efbbe3) + displayTime(attempt.expiresAt), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (attempt.expiresAt > System.currentTimeMillis().toULong()) TextButton(onClick = { client.channels.continueInvite(channel, attempt) }, enabled = !working) {
                    Text(if (attempt.link == null) str(R.string.text_continue_original_invitation_b67cddf) else str(R.string.text_show_saved_invitation_12e2d3b))
                } else Text(str(R.string.text_invitation_expired_f730b0f), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
            }
        }
        TextButton(onClick = { client.navigation.back() }) { Text(str(R.string.text_done_9ffd599)) }
    }
}

@Composable private fun channelRole(role: String) = when (role) { "owner" -> str(R.string.text_owner_6fa387e); "admin" -> str(R.string.text_administrator_f8e6ab3); "publisher" -> str(R.string.text_publisher_6c5ec4a); "member" -> str(R.string.text_member_9b8c32d); else -> role }
@Composable private fun channelType(kind: String?) = when (kind) { "direct" -> str(R.string.text_direct_conversation_0217cd3); "distribution" -> str(R.string.text_announcements_2a5fe32); "collaboration" -> str(R.string.text_group_conversation_9d71b3b); else -> str(R.string.text_unverified_conversation_70b8a57) }
@Composable private fun channelActionTitle(action: String) = when (action) { "role" -> str(R.string.text_change_role_f7524ab); "remove" -> str(R.string.text_remove_this_member_33453eb); "archive" -> str(R.string.text_archive_this_conversation_5b3cbfa); "leave" -> str(R.string.text_leave_this_conversation_bf635a4); else -> str(R.string.text_update_conversation_keys_54d96eb) }


@Composable fun InviteLetterSenderScreen(client: ClientController, account: String) {
    val channels by client.messages.channels.state.collectAsStateWithLifecycle()
    LaunchedEffect(account) { client.messages.browse() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            DetailCard { ParticipantIdentity(client, account, true) }
            ReadFeedback(channels, client.messages::browse)
        }
        items(channels.valueOrNull().orEmpty().filter { it.canInvite }, key = { it.id }) { channel ->
            Section {
                ListRow(channel.name, icon = Icons.Outlined.Forum) {
                    client.channels.editInvite(channel.id, ChannelInvite(account))
                    client.navigation.push(Route("channel-invite", channel = channel.id))
                }
            }
        }
        item {
            Button(onClick = { client.channels.openSetup(false, invitee = account) }, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Outlined.Add, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_new_conversation_8687862))
            }
        }
    }
}
