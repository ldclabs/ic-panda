package net.dmsg.android.ui

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
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

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
    var leaveDraft by remember(channel) { mutableStateOf(false) }
    LaunchedEffect(channel, state.epoch) { client.channels.load(channel) }
    BackHandler(detail != null && name != detail.name) { leaveDraft = true }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            ReadFeedback(loaded, { client.channels.load(channel) })
            OutlinedTextField(name, { client.channels.editName(channel, it) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_conversation_name_428c56e)) }, singleLine = true, enabled = detail != null && !working,
                supportingText = { Text(localizedNumber(name.codePointCount(0, name.length)) + " / " + localizedNumber(100)) }, isError = name.isNotEmpty() && !validChannelName(name))
            if (detail != null && name != detail.name) Button(onClick = { client.channels.saveName(channel) }, enabled = validChannelName(name) && !working) { Text(str(R.string.text_save_name_93b4376)) }
            detail?.let {
                Text(channelType(it.kind), style = MaterialTheme.typography.bodyMedium)
                if (it.status == null) Notice(str(R.string.text_the_control_log_is_not_available_yet_refresh_before_changing_memb_d1e7415))
                if (it.status == "rotation_required") {
                    Notice(str(R.string.text_membership_change_recorded_sending_is_paused_until_the_new_conver_ce3b7ac))
                    TextButton(onClick = { client.messages.refresh(channel); client.channels.load(channel) }) { Text(str(R.string.text_check_key_update_4fc68c6)) }
                }
                if (it.left) Notice(str(R.string.text_you_left_this_conversation_the_cached_membership_is_historical_sh_252342d))
                if (it.status == "archived") Notice(str(R.string.text_the_owner_closed_this_conversation_history_remains_readable_this_bdb050e))
                SettingToggle(str(R.string.text_mute_notifications_005e602), it.muted, { value -> client.channels.mute(channel, value) }, enabled = !working)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(15 to str(R.string.text_15_minutes_b81c469), 60 to str(R.string.text_1_hour_0ac0bd6)).forEach { (minutes, label) -> TextButton(onClick = { client.channels.mute(channel, true, (System.currentTimeMillis() + minutes * 60_000L).toULong()) }, enabled = !working) { Text(label) } }
                    TextButton(onClick = { val tomorrow = java.util.Calendar.getInstance().apply { add(java.util.Calendar.DAY_OF_YEAR, 1); set(java.util.Calendar.HOUR_OF_DAY, 9); set(java.util.Calendar.MINUTE, 0); set(java.util.Calendar.SECOND, 0); set(java.util.Calendar.MILLISECOND, 0) }; client.channels.mute(channel, true, tomorrow.timeInMillis.toULong()) }, enabled = !working) { Text(str(R.string.text_until_tomorrow_4bfab87)) }
                }
                it.mutedUntil?.let { until -> Text(str(R.string.text_muted_until_50fbca1) + displayTime(until)) }
                if (it.notificationPending) Text(str(R.string.text_notification_preference_awaits_synchronization_2c34eab), style = MaterialTheme.typography.bodySmall)
                if (it.notificationConflict) TextButton(onClick = { client.channels.mute(channel, it.muted, it.mutedUntil) }, enabled = !working) { Text(str(R.string.text_retry_this_notification_preference_aad4fbd)) }
                SettingToggle(str(R.string.text_hide_from_conversation_list_f37326b), it.hidden, { value -> client.channels.hide(channel, value) }, enabled = !working)
                Text(str(R.string.text_hiding_keeps_your_membership_and_history_find_it_again_in_hidden_8871afa), style = MaterialTheme.typography.bodySmall)
                HorizontalDivider(Modifier.padding(vertical = 12.dp))
                Text(str(R.string.text_members_437a10e), style = MaterialTheme.typography.titleMedium)
            }
        }
        items(detail?.members.orEmpty(), key = { it.account }) { member ->
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                ParticipantIdentity(client, member.account, showId = true)
                Text(channelRole(member.role) + if (member.canRotate) str(R.string.text_may_update_keys_df6de73) else "", style = MaterialTheme.typography.bodySmall)
                Text(str(R.string.text_joined_at_key_epoch_0f80f11) + member.joinedEpoch, style = MaterialTheme.typography.bodySmall)
                if (member.history.isNotEmpty()) Text(str(R.string.text_granted_older_history_c0fd324) + member.history.joinToString { "${it.first}–${it.second}" }, style = MaterialTheme.typography.bodySmall)
                if (detail?.left == false && detail.status != "archived" && member.account != state.session?.account && pending == null) {
                    if (mine?.role == "owner") {
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            listOf("admin", "member", "publisher").filter { it != member.role }.forEach { role -> TextButton(onClick = { action = ChannelAction("role", member.account, role, member.canRotate) }, enabled = !working) { Text(channelRole(role)) } }
                        }
                        SettingToggle(str(R.string.text_allow_key_updates_b8aee14), member.canRotate, { action = ChannelAction("role", member.account, member.role, it) }, enabled = !working)
                    }
                    if (detail?.canRemove(state.session?.account, member) == true) TextButton(onClick = { action = ChannelAction("remove", member.account) }, enabled = !working) { Text(str(R.string.text_remove_member_9443c64), color = MaterialTheme.colorScheme.error) }
                }
                HorizontalDivider()
            }
        }
        item {
            detail?.let {
                if (it.canManage(state.session?.account)) OutlinedButton(onClick = { client.navigation.push(Route("channel-invite", channel = channel)) }, enabled = !working) { Icon(Icons.Outlined.PersonAdd, null); Text(str(R.string.text_invite_an_account_96d0472)) }
                if (!it.left && it.status != "archived" && mine != null && pending == null) {
                    if (mine.role in setOf("owner", "admin") || mine.canRotate) TextButton(onClick = { action = ChannelAction("rekey") }, enabled = !working) { Text(str(R.string.text_update_conversation_keys_66bc367)) }
                    TextButton(onClick = { action = ChannelAction(if (mine.role == "owner") "archive" else "leave") }, enabled = !working) { Text(if (mine.role == "owner") str(R.string.text_archive_conversation_c2331b7) else str(R.string.text_leave_conversation_fc5eb24), color = MaterialTheme.colorScheme.error) }
                }
                Text(str(R.string.text_readable_key_epochs_on_this_device_2c98a15) + it.readableEpochs.joinToString().ifEmpty { "—" }, style = MaterialTheme.typography.bodySmall)
                Notice(str(R.string.text_new_members_may_read_the_current_key_epoch_including_earlier_mess_8fd599a))
            }
            if (pending != null) OutlinedButton(onClick = { client.channels.change(channel, pending.action) }, enabled = !working) { Text(str(R.string.text_retry_original_change_670ac66)) }
            review?.pending.orEmpty().forEach { operation ->
                Text(operation.action.substringAfterLast('/').takeIf { it != "v1" } ?: operation.key.substringBefore(':'), style = MaterialTheme.typography.labelMedium)
                SelectionContainer { Text(operation.request, style = MaterialTheme.typography.bodySmall) }
                TextButton(onClick = { client.channels.resume(channel, operation.key) }, enabled = !working) { Text(str(R.string.text_check_original_result_30dd950)) }
            }
            TextButton(onClick = { client.clipboard.copy(channel) }) { Text(str(R.string.text_copy_conversation_id_0defedf)) }
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
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp)) {
        item { ReadFeedback(hidden, client.channels::browseHidden); if (hidden is ReadState.Empty) EmptyPanel(str(R.string.text_no_hidden_conversations_1471673)) }
        items(hidden.valueOrNull().orEmpty(), key = { it.id }) { channel -> ListItem(headlineContent = { Text(channel.name) }, supportingContent = { Text(channelType(channel.kind)) }, trailingContent = {
            TextButton(onClick = { client.channels.hide(channel.id, false) }) { Text(str(R.string.text_show_again_1409301)) }
        }) }
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
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(!form.joining, { client.channels.editSetup(form.copy(joining = false)) }, { Text(str(R.string.text_create_7570cfc)) }, enabled = !working)
            FilterChip(form.joining, { client.channels.editSetup(form.copy(joining = true)) }, { Text(str(R.string.text_join_5f4ca2d)) }, enabled = !working)
        }
        if (form.joining) {
            if (summary == null || editingInvitation) {
                OutlinedTextField(form.invitation, { client.channels.editSetup(form.copy(invitation = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_paste_invitation_0989300)) }, minLines = 3, maxLines = 4, enabled = !working)
                if (summary != null) TextButton(onClick = { editingInvitation = false }) { Text(str(R.string.text_review_invitation_0f9ebe4)) }
            } else Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.Link, null)
                Text(str(R.string.text_invitation_loaded_eebc00b), Modifier.weight(1f).padding(start = 8.dp))
                TextButton(onClick = { editingInvitation = true }, enabled = !working) { Text(str(R.string.text_change_c6b27c7)) }
            }
            TextButton(onClick = { client.qr.open("channel-join") }, enabled = !working) { Icon(Icons.Outlined.QrCodeScanner, null); Text(str(R.string.text_scan_invitation_27f0014)) }
            if (summary != null) {
                Text(summary.name, style = MaterialTheme.typography.titleMedium)
                ParticipantIdentity(client, summary.account, showId = true)
                Text(str(R.string.text_expires_1efbbe3) + displayTime(summary.expiresAt))
                if (summary.account != state.session?.account) Notice(str(R.string.text_switch_to_the_invited_account_before_joining_f24b50c))
                if (summary.expiresAt <= now) Notice(str(R.string.text_this_invitation_has_expired_ask_for_a_new_invitation_d58d62e), true)
                Notice(str(R.string.text_these_details_come_from_the_invitation_choosing_join_verifies_the_1fe792b))
            } else if (form.invitation.isNotEmpty()) Notice(str(R.string.text_the_invitation_is_invalid_0374805), true)
        } else {
            OutlinedTextField(form.name, { client.channels.editSetup(form.copy(name = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_conversation_name_428c56e)) }, enabled = !working && form.request == null, singleLine = true, isError = form.name.isNotEmpty() && !validChannelName(form.name))
            listOf("direct", "collaboration", "distribution").forEach { kind -> FilterChip(form.kind == kind, { client.channels.editSetup(form.copy(kind = kind)) }, { Text(channelType(kind)) }, enabled = !working && form.request == null) }
            Text(when (form.kind) { "distribution" -> str(R.string.text_only_the_owner_administrators_and_publishers_can_send_messages_6811c29); "direct" -> str(R.string.text_a_conversation_for_two_accounts_7ba9e18); else -> str(R.string.text_every_member_can_send_messages_020f52c) })
            Text(str(R.string.text_after_creation_invite_an_account_to_start_the_conversation_b389453), style = MaterialTheme.typography.bodySmall)
        }
        form.error?.let { Notice(errorText(it), true) }
        Button(onClick = client.channels::completeSetup, enabled = !working && if (form.joining) summary != null && summary.account == state.session?.account && summary.expiresAt > now else validChannelName(form.name)) {
            Text(if (form.joining) str(R.string.text_join_conversation_2b8512c) else if (form.request != null) str(R.string.text_retry_original_creation_bc1e0df) else str(R.string.text_create_conversation_e2ac99b))
        }
        if (working) { LinearProgressIndicator(Modifier.fillMaxWidth()); TextButton(onClick = { client.cancelOperation("channel:setup"); client.channels.loadCreations() }) { Text(str(R.string.text_stop_waiting_be4e25a)) } }
        ReadFeedback(creations, client.channels::loadCreations)
        creations.valueOrNull().orEmpty().forEach { pending ->
            HorizontalDivider(); Text(pending.name, style = MaterialTheme.typography.titleMedium); Text(channelType(pending.kind))
            TextButton(onClick = { client.channels.create(pending) }, enabled = !working) { Text(str(R.string.text_continue_original_creation_631000f)) }
            TextButton(onClick = { client.channels.discard(pending) }, enabled = !working) { Text(str(R.string.text_discard_if_not_yet_signed_7c61b1f)) }
        }
        Text(str(R.string.text_form_drafts_stay_only_in_this_unlocked_session_started_creations_d6a8bc0), style = MaterialTheme.typography.bodySmall)
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
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        RecipientField(client, form.account, { client.channels.editInvite(channel, form.copy(account = it)) }, enabled = !working && form.request == null, label = str(R.string.text_member_account_id_73d9865))
        Notice(str(R.string.text_only_this_account_can_accept_the_invitation_it_may_read_the_curre_7898d3d))
        form.error?.let { Notice(errorText(it), true) }
        if (form.link == null) Button(onClick = { client.channels.invite(channel) }, enabled = validAccount(form.account) && !working) { Text(if (form.request == null) str(R.string.text_create_invitation_bb66682) else str(R.string.text_retry_original_invitation_2751ed3)) }
        if (working) LinearProgressIndicator(Modifier.fillMaxWidth())
        form.link?.let { link ->
            Text(str(R.string.text_invitation_ready_fe53899), style = MaterialTheme.typography.titleMedium)
            ChannelInvitation.parse(link)?.let { Text(str(R.string.text_expires_1efbbe3) + displayTime(it.expiresAt)) }
            QrCode(link)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { client.beginExternalInteraction(); context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, link) }, null)) }) { Icon(Icons.Outlined.Share, null); Text(str(R.string.text_share_invitation_c716745)) }
                OutlinedButton(onClick = { client.clipboard.copy(link) }) { Text(str(R.string.text_copy_invitation_6a08c36)) }
            }
            TextButton(onClick = { client.channels.editInvite(channel, ChannelInvite()) }) { Text(str(R.string.text_invite_another_account_67d5fa8)) }
        }
        ReadFeedback(previous, { client.channels.loadInvites(channel) })
        previous.valueOrNull()?.takeIf { it.channel == channel }?.items.orEmpty().filter { it.request != form.request }.forEach { attempt ->
            HorizontalDivider()
            ParticipantIdentity(client, attempt.account, true)
            Text(str(R.string.text_expires_1efbbe3) + displayTime(attempt.expiresAt), style = MaterialTheme.typography.bodySmall)
            if (attempt.expiresAt > System.currentTimeMillis().toULong()) TextButton(onClick = { client.channels.continueInvite(channel, attempt) }, enabled = !working) { Text(if (attempt.link == null) str(R.string.text_continue_original_invitation_b67cddf) else str(R.string.text_show_saved_invitation_12e2d3b)) }
            else Text(str(R.string.text_invitation_expired_f730b0f), style = MaterialTheme.typography.bodySmall)
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
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp)) {
        item { ParticipantIdentity(client, account, true); ReadFeedback(channels, client.messages::browse) }
        items(channels.valueOrNull().orEmpty().filter { it.canInvite }, key = { it.id }) { channel ->
            AccountRow(channel.name) { client.channels.editInvite(channel.id, ChannelInvite(account)); client.navigation.push(Route("channel-invite", channel = channel.id)) }
        }
        item { Button(onClick = { client.channels.openSetup(false, invitee = account) }) { Text(str(R.string.text_new_conversation_8687862)) } }
    }
}
