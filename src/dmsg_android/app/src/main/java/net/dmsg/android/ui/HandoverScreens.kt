package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import kotlinx.serialization.json.*
import net.dmsg.android.core.*

@Composable fun HandoversScreen(client: ClientController, id: String?) {
    val listing by client.handovers.listing.state.collectAsStateWithLifecycle()
    val copies by client.handovers.copies.state.collectAsStateWithLifecycle()
    val results by client.handovers.results.collectAsStateWithLifecycle()
    val local by client.accountWorkspace.local.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var confirming by remember(id) { mutableStateOf<Pair<Handover, String>?>(null) }
    val fresh = listing is ReadState.Ready
    LaunchedEffect(state.epoch, id) { client.handovers.load() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            Text(str(R.string.text_the_new_device_asks_to_replace_an_administrator_the_old_device_re_4c92532))
            ListRow(str(R.string.text_review_a_replacement_request_2ef5a94)) { client.navigation.push(Route("handover-review")) }
            ReadFeedback(listing, client.handovers::load)
            ReadFeedback(local, client.accountWorkspace::local)
            local.valueOrNull()?.let { LocalContentPanel(it) }
            if (listing is ReadState.Empty) EmptyState(str(R.string.text_no_handovers_on_this_device_a165d2c))
        }
        listing.valueOrNull().orEmpty().filter { id == null || it.id == id }.forEach { task ->
            item(key = task.id) {
                Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(if (task.side == "successor") str(R.string.text_this_device_replaces_defb22c) else str(R.string.text_this_device_is_replaced_by_03bfeef), style = MaterialTheme.typography.titleMedium)
                    TransferIdentity(str(R.string.text_other_device_id_9cd966d), if (task.side == "successor") task.predecessor else task.successor)
                    TransferIdentity(str(R.string.text_handover_id_ef2f040), task.id)
                    val busy = state.working("handover:step:" + task.id)
                    if (task.cancelledAt != null) Notice(str(R.string.text_handover_stopped_completed_changes_remain_b4f2752))
                    task.steps.forEach { step ->
                        HorizontalDivider()
                        Text(handoverStepTitle(step.name), style = MaterialTheme.typography.titleSmall)
                        Text(when { step.completed -> str(R.string.text_done_c5272e0); step.left -> str(R.string.text_left_behind_cbb9cc3); task.canRun(step.name) -> str(R.string.text_ready_380c7f1); else -> str(R.string.text_waiting_e141313) }, style = MaterialTheme.typography.labelMedium)
                        if (step.left) Text(step.skipped.joinToString("\n") { if (it == "library") str(R.string.text_file_library_92b2dcd) else it })
                        if (task.canRun(step.name)) {
                            Text(handoverStepHint(step.name), style = MaterialTheme.typography.bodySmall)
                            when (step.name) {
                                "content" -> {
                                    OutlinedButton(onClick = { client.navigation.push(Route("device-transfer", task.id, field = task.side)) }, enabled = fresh && !busy) { Text(if (task.side == "successor") str(R.string.text_receive_here_3ea6c72) else str(R.string.text_send_file_library_44ec367)) }
                                    if (task.side == "successor") copies.valueOrNull().orEmpty().filter { it.account == task.account && it.device == task.predecessor }.forEach { copy ->
                                        TextButton(onClick = { client.handovers.import(copy, task) }, enabled = fresh && copies is ReadState.Ready && !state.working("handover:import:" + copy.id)) { Text(str(R.string.text_import_from_saved_copy_c2bd375)) }
                                    }
                                    TextButton(onClick = { confirming = task to "content" }, enabled = fresh && !busy) { Text(str(R.string.text_leave_file_library_behind_d01a572), color = MaterialTheme.colorScheme.error) }
                                }
                                "native_addresses" -> {
                                    OutlinedButton(onClick = { client.handovers.run(task, step.name) }, enabled = fresh && !busy) { Text(str(R.string.text_check_native_addresses_6942295)) }
                                    client.handovers.addresses(task.id).forEach { address ->
                                        ListRow(address.wallet + " #" + address.from) { client.navigation.push(Route("wallet-native-move", address.wallet, sequence = address.from.toULong())) }
                                    }
                                    TextButton(onClick = { confirming = task to "native_addresses" }, enabled = fresh && !busy && results[task.id]?.get("addresses") is JsonArray) { Text(str(R.string.text_leave_native_addresses_behind_691ba11), color = MaterialTheme.colorScheme.error) }
                                }
                                "revoke" -> Button(onClick = { confirming = task to "revoke" }, enabled = fresh && !busy, colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)) { Text(str(R.string.text_review_revoking_old_device_b2d8c39)) }
                                else -> OutlinedButton(onClick = { client.handovers.run(task, step.name) }, enabled = fresh && !busy) {
                                    Text(if (step.name == "cosigners" && task.side == "successor") str(R.string.text_check_key_handover_29f115f) else str(R.string.text_continue_this_step_c0f05bf))
                                }
                            }
                        }
                    }
                    results[task.id]?.let { result ->
                        (result["requests"] as? JsonArray)?.forEach { request -> ListRow(str(R.string.text_review_pending_wallet_approval_9e99b62)) { client.approvals.open(request.objectValue().required("id")) } }
                        if (result["rotations"] != null) Notice(str(R.string.text_some_wallets_still_need_their_own_approvals_check_those_wallets_t_4fa3689))
                    }
                    if (task.complete) Notice(str(R.string.text_handover_completed_keep_an_approval_key_and_cosigner_on_another_t_f404cb0))
                    else if (task.cancelledAt == null) TextButton(onClick = { confirming = task to "stop" }, enabled = fresh && !busy) { Text(str(R.string.text_stop_handover_ce6474f), color = MaterialTheme.colorScheme.error) }
                } }
            }
        }
        item {
            ReadFeedback(copies, client.handovers::loadCopies)
            copies.valueOrNull().orEmpty().filter { it.account == state.session?.account }.forEach { copy ->
                SectionTitle(str(R.string.text_saved_copy_from_another_device_a18a734))
                TransferIdentity(str(R.string.text_original_device_8637057), copy.device)
                Text(displayTime(copy.movedAt))
                TextButton(onClick = { client.handovers.import(copy, null) }, enabled = copies is ReadState.Ready && !state.working("handover:import:" + copy.id)) { Text(str(R.string.text_import_file_library_a7b7ac3)) }
            }
        }
    }
    confirming?.let { (task, action) -> AlertDialog(onDismissRequest = { confirming = null }, title = { Text(when(action) { "stop" -> str(R.string.text_stop_this_handover_f0b2cc5); "revoke" -> str(R.string.text_revoke_the_old_device_0a5eeb4); else -> str(R.string.text_leave_this_step_behind_82f82d9) }) }, text = {
        Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            TransferIdentity(str(R.string.text_old_device_ce381e3), task.predecessor)
            Text(when(action) { "stop" -> str(R.string.text_completed_changes_remain_revoke_the_new_device_separately_if_you_a52187b); "content" -> str(R.string.text_files_without_a_cloud_backup_remain_only_on_the_old_device_unsync_1e3326b); "native_addresses" -> str(R.string.text_l1_still_needs_the_old_device_members_may_sweep_through_l2_once_t_1e1b586); else -> str(R.string.text_the_old_device_will_lose_access_to_this_account_content_existing_e9fcbb1) })
            if (action == "revoke" && task.leftBehind.isNotEmpty()) LabelValue(str(R.string.text_left_behind_da0536b), task.leftBehind.joinToString("\n"))
            if (action == "native_addresses") client.handovers.addresses(task.id).forEach { Text(it.wallet + " #" + it.from) }
        }
    }, confirmButton = { TextButton(onClick = { confirming = null; when(action) { "stop" -> client.handovers.cancel(task); "revoke" -> client.handovers.run(task, action); else -> client.handovers.skip(task, action) } }) { Text(str(R.string.text_confirm_563fd5a), color = MaterialTheme.colorScheme.error) } }, dismissButton = { TextButton(onClick = { confirming = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}

@Composable fun HandoverReviewScreen(client: ClientController) {
    val needsFresh by client.handovers.needsFreshRequest.collectAsStateWithLifecycle()
    val token by client.handovers.token.collectAsStateWithLifecycle()
    val confirmation by client.handovers.confirmation.collectAsStateWithLifecycle()
    val review by client.handovers.review.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    val local by client.accountWorkspace.local.state.collectAsStateWithLifecycle()
    var now by remember { mutableStateOf(System.currentTimeMillis().toULong()) }
    val focus = LocalFocusManager.current
    LaunchedEffect(qr.result) { client.qr.consume("handover-review")?.let(client.handovers::editToken) }
    LaunchedEffect(state.epoch) { client.accountWorkspace.local() }
    LaunchedEffect(Unit) { while (true) { now = System.currentTimeMillis().toULong(); delay(1000) } }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Text(str(R.string.text_read_the_request_from_the_new_device_then_compare_and_type_its_co_b5449b1))
            if (needsFresh) Notice(str(R.string.text_complete_the_wallet_approvals_then_generate_a_fresh_replacement_r_d0f4549))
            OutlinedTextField(token, client.handovers::editToken, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_replacement_request_64790c0)) }, minLines = 2, maxLines = 6)
            OutlinedButton(onClick = { client.qr.open("handover-review") }) { Text(str(R.string.text_scan_replacement_request_e6f6043)) }
            Button(onClick = { focus.clearFocus(); client.handovers.checkRequest() }, enabled = token.isNotBlank() && token.length <= 16 * 1024 && !state.working("handover:review")) { Text(str(R.string.text_check_request_be49fc6)) }
            if (review !is ReadState.Idle) ReadFeedback(review, client.handovers::checkRequest)
            review.valueOrNull()?.let { value ->
                TransferIdentity(str(R.string.text_new_device_id_9d9457b), value.device)
                LabelValue(str(R.string.text_role_b9e0249), value.role)
                LabelValue(str(R.string.text_permissions_be9338a), value.capabilities.joinToString("\n"))
                LabelValue(str(R.string.text_approval_key_ebe1d40), if (value.approvalKey) str(R.string.text_created_on_new_device_a77d6d9) else str(R.string.text_unavailable_a19c84a))
                LabelValue(str(R.string.text_cosigner_262b6c6), if (value.cosigner) str(R.string.text_created_on_new_device_a77d6d9) else str(R.string.text_unavailable_a19c84a))
                LabelValue(str(R.string.text_expires_12414d7), displayTime(value.expires))
                value.wallets.forEach { wallet -> TransferIdentity(wallet.name.ifBlank { str(R.string.text_wallet_77c08be) }, wallet.id) }
                ReadFeedback(local, client.accountWorkspace::local); local.valueOrNull()?.let { LocalContentPanel(it) }
                OutlinedTextField(confirmation, client.handovers::editConfirmation, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_type_the_new_device_id_37f0790)) }, maxLines = 3)
            }
        }
        Surface {
            Button(onClick = { focus.clearFocus(); client.handovers.approve() }, enabled = client.handovers.canApprove(now) && !state.working("handover:approve"), modifier = Modifier.fillMaxWidth().padding(pagePadding())) { Text(str(R.string.text_review_replacement_approval_a633998)) }
        }
    }
}

@Composable fun HandoverRequestScreen(client: ClientController) {
    val copyApproval by client.handovers.copyApproval.collectAsStateWithLifecycle()
    val predecessor by client.handovers.predecessor.collectAsStateWithLifecycle()
    val requested by client.handovers.requested.state.collectAsStateWithLifecycle()
    val copies by client.handovers.copies.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    if (copyApproval != null) {
        androidx.activity.compose.BackHandler { client.handovers.closeCopyApproval() }
        Column(Modifier.fillMaxSize()) {
            TextButton(onClick = client.handovers::closeCopyApproval) { Text(str(R.string.text_back_to_replacement_d3f5aba)) }
            ApprovalsScreen(client, state, copyApproval!!, {})
        }
        return
    }
    var copyConfirm by remember { mutableStateOf<MovedCopy?>(null) }
    LaunchedEffect(state.epoch) { client.handovers.loadCopies() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Notice(str(R.string.text_this_device_creates_its_own_keys_the_old_administrator_reviews_it_097d2ec))
        OutlinedTextField(predecessor, client.handovers::editPredecessor, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_id_of_the_device_to_replace_7d565df)) }, maxLines = 3, enabled = !state.working("handover:request"))
        Button(onClick = client.handovers::createRequest, enabled = transferHex(predecessor.trim().lowercase()) && requested.valueOrNull() == null && !state.working("handover:request")) { Text(str(R.string.text_create_replacement_request_33db979)) }
        if (requested !is ReadState.Idle) ReadFeedback(requested, client.handovers::createRequest)
        requested.valueOrNull()?.let { value ->
            QrCode(value.token)
            TransferIdentity(str(R.string.text_new_device_id_9d9457b), value.device)
            LabelValue(str(R.string.text_approval_key_ebe1d40), if (value.approvalKey) str(R.string.text_created_36ba304) else str(R.string.text_unavailable_a19c84a))
            LabelValue(str(R.string.text_cosigner_262b6c6), if (value.cosigner) str(R.string.text_created_36ba304) else str(R.string.text_unavailable_a19c84a))
            Text(str(R.string.text_on_the_old_device_open_devices_replace_a_device_review_a_request_c03fa26))
            TextButton(onClick = { client.clipboard.copy(value.token) }) { Text(str(R.string.text_copy_replacement_request_58c84f4)) }
            Button(onClick = client.handovers::checkAuthorization, enabled = !state.busy) { Text(str(R.string.text_check_authorization_039f8a2)) }
        }
        ReadFeedback(copies, client.handovers::loadCopies)
        copies.valueOrNull().orEmpty().filter { it.account == state.session?.account }.forEach { copy ->
            TransferIdentity(str(R.string.text_saved_original_device_106ee51), copy.device)
            OutlinedButton(onClick = { copyConfirm = copy }, enabled = !state.working("handover:copy-approve:" + copy.id)) { Text(str(R.string.text_review_authorization_using_saved_copy_352bc69)) }
        }
    }
    copyConfirm?.let { copy -> AlertDialog(onDismissRequest = { copyConfirm = null }, title = { Text(str(R.string.text_use_this_saved_administrator_copy_1332ff2)) }, text = {
        Column { TransferIdentity(str(R.string.text_original_device_8637057), copy.device); Text(str(R.string.text_the_core_verifies_that_the_copy_can_authorize_this_replacement_de_a241c36)) }
    }, confirmButton = { TextButton(onClick = { copyConfirm = null; client.handovers.approveFromCopy(copy) }) { Text(str(R.string.text_review_9a1a906)) } }, dismissButton = { TextButton(onClick = { copyConfirm = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}

internal fun handoverStepTitle(value: String) = when (value) {
    "authorize" -> str(R.string.text_approve_new_device_6b1738a); "approval_key" -> str(R.string.text_approval_key_ebe1d40); "cosigners" -> str(R.string.text_cosigner_keys_1ad4ee6); "content" -> str(R.string.text_local_content_5ca80ce)
    "wallets" -> str(R.string.text_wallet_bindings_ce20aea); "native_addresses" -> str(R.string.text_native_addresses_8199a08); "revoke" -> str(R.string.text_revoke_old_device_a11c12a); else -> value
}
private fun handoverStepHint(value: String) = when (value) {
    "authorize" -> str(R.string.text_enrolls_the_new_device_and_rotates_the_content_root_c3f52e2)
    "approval_key" -> str(R.string.text_registers_the_new_device_s_approval_key_59cadb5)
    "cosigners" -> str(R.string.text_the_old_device_proposes_key_replacements_the_new_device_checks_th_7e292d8)
    "content" -> str(R.string.text_transfer_the_file_library_or_import_a_saved_copy_956f68a)
    "wallets" -> str(R.string.text_transfer_wallet_bindings_with_each_wallet_s_required_approvals_c807241)
    "native_addresses" -> str(R.string.text_move_assets_to_the_new_device_s_own_key_or_leave_them_for_l2_0ebeaeb)
    else -> str(R.string.text_revoke_only_after_reviewing_anything_left_on_the_old_device_536c317)
}
