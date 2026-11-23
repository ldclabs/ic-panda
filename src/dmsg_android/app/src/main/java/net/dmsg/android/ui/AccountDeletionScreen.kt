package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun AccountDeletionScreen(client: ClientController) {
    val status by client.accountDeletion.status.state.collectAsStateWithLifecycle()
    val preview by client.accountDeletion.preview.state.collectAsStateWithLifecycle()
    val draft by client.accountDeletion.draft.collectAsStateWithLifecycle()
    val failure by client.accountDeletion.error.collectAsStateWithLifecycle()
    val devices by client.account.devices.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var confirming by remember { mutableStateOf(false) }
    val job = status.valueOrNull()?.deletion
    val working = state.working("account-deletion:request")
    val canDelete = status is ReadState.Ready && preview is ReadState.Ready && devices is ReadState.Ready &&
        draft.let { client.accountDeletion.ready() } && !working
    LaunchedEffect(state.epoch) { client.accountDeletion.load() }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val currentFailure = failure
            val review = preview.valueOrNull()
            val banner = when {
                currentFailure != null -> errorText(currentFailure) to true
                job?.lastError != null -> errorText(job.lastError) to true
                job == null && devices.valueOrNull()?.canManage == false -> str(R.string.text_an_administrator_with_account_management_permission_must_request_c8f3fa3) to false
                job == null && review != null && review.pendingSync > 0u -> str(R.string.text_this_device_has_unsynced_changes_save_anything_you_need_before_de_eaddda5) to true
                else -> str(R.string.text_deleting_this_service_account_permanently_removes_its_cloud_ciphe_77889e6) to true
            }
            Notice(banner.first, banner.second)
            Section(footer = str(R.string.text_account_deletion_does_not_cancel_a_subscription_purchased_through_6e817fb)) {
                ListRow(str(R.string.text_membership_and_subscription_source_58d228f), icon = Icons.Outlined.CardMembership) { client.navigation.push(Route("membership")) }
            }
            ReadFeedback(status, client.accountDeletion::load)
            if (job != null) {
                Section(title = str(R.string.text_service_deletion_status_16da4e6)) {
                    InfoRow(
                        str(R.string.text_state_91c2d40),
                        when (job.state) {
                            "deleted" -> str(R.string.text_cloud_deletion_completed_93fa5b0)
                            "requested", "purging" -> str(R.string.text_deletion_is_in_progress_33700e0)
                            else -> str(R.string.text_deletion_needs_checking_4b9bcd2)
                        },
                    )
                    CodeRow(str(R.string.text_request_id_3182520), job.id) { client.clipboard.copy(job.id) }
                    job.requestedAt?.let { ListRow(str(R.string.text_requested_at_878a0ba), value = displayTime(it)) }
                }
                OutlinedButton(onClick = client.accountDeletion::load, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                    Text(str(R.string.text_check_deletion_status_32d09ee))
                }
                if (job.state == "deleted") Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                    ListRow(str(R.string.text_remove_this_account_s_local_data_a2189f8), icon = Icons.Outlined.DeleteOutline, destructive = true) {
                        client.navigation.push(Route("account-remove"))
                    }
                }
            } else {
                ReadFeedback(preview, client.accountDeletion::load)
                review?.let { info ->
                    Section {
                        CodeRow(str(R.string.text_account_id_43f0a66), info.account) { client.clipboard.copy(info.account) }
                        ListRow(str(R.string.text_changes_waiting_to_sync_344814b), value = info.pendingSync.toString())
                        if (info.pendingOperation) ListRow(str(R.string.text_resolve_the_pending_account_operation_92a2f53), glyph = R.drawable.glyph_approvals) {
                            client.navigation.push(Route("approvals"))
                        }
                    }
                    if (info.channels.isNotEmpty()) {
                        Section(title = str(R.string.text_transfer_or_archive_owned_conversations_first_eb26d5f)) {
                            info.channels.forEach { channel ->
                                ListRow(channel.name, icon = Icons.Outlined.Forum) { client.navigation.push(Route("channel-settings", channel = channel.id)) }
                            }
                        }
                    }
                    if (info.requiresWalletReview) {
                        Section(
                            title = str(R.string.text_wallet_and_external_credential_access_7351124),
                            footer = buildString {
                                if (!info.walletsAvailable) append(str(R.string.text_wallet_access_could_not_be_checked_verify_it_independently_before_9b16c58)).append("\n")
                                append(str(R.string.text_deletion_does_not_transfer_funds_removing_keys_or_cloud_copies_ca_548d61b))
                            },
                        ) {
                            info.wallets.forEach { wallet ->
                                ListRow(wallet.name, glyph = R.drawable.glyph_wallet) { client.navigation.push(Route("wallet", wallet.id)) }
                            }
                            NoticeSwitch(str(R.string.text_i_have_secured_wallet_access_and_external_credentials_6899552), draft.walletsReviewed, !working) {
                                client.accountDeletion.edit(draft.copy(walletsReviewed = it))
                            }
                        }
                    }
                    ReadFeedback(devices, client.account::devices)
                    DetailCard {
                        OutlinedTextField(
                            draft.confirmation,
                            { client.accountDeletion.edit(draft.copy(confirmation = it)) },
                            Modifier.fillMaxWidth().revealFocusedInput().testTag("deletion-confirmation"),
                            label = { Text(str(R.string.text_type_this_account_id_to_confirm_303e2ed)) },
                            singleLine = true,
                            enabled = !working,
                        )
                    }
                }
            }
            Section {
                ListRow(str(R.string.text_privacy_policy_d216307), icon = Icons.Outlined.Policy) { client.navigation.push(Route("privacy")) }
            }
        }
        if (job == null) BottomActionBar {
            Button(
                onClick = { confirming = true },
                enabled = canDelete,
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error, contentColor = MaterialTheme.colorScheme.onError),
            ) { Text(str(R.string.text_request_account_deletion_b443c94)) }
        }
    }
    if (confirming) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_permanently_delete_this_service_account_cf40644)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SelectionContainer { Text(preview.valueOrNull()?.account.orEmpty(), style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            Text(str(R.string.text_this_cannot_be_undone_the_app_will_ask_for_system_confirmation_an_183ced9))
        }
    }, confirmButton = { TextButton(onClick = { confirming = false; client.accountDeletion.submit() }) { Text(str(R.string.text_delete_account_4899915), color = MaterialTheme.colorScheme.error) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
