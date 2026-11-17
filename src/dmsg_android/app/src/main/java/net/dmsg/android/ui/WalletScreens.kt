package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.quantity
import net.dmsg.android.platform.str
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import net.dmsg.core.WalletChange

@Composable fun WalletsScreen(client: ClientController) {
    val wallets by client.wallets.wallets.state.collectAsStateWithLifecycle()
    val pending by client.wallets.pending.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var wallet by remember { mutableStateOf("") }
    LaunchedEffect(state.epoch) { client.wallets.list() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item { ReadFeedback(wallets, { client.wallets.list(true) }); Text(str(R.string.text_manage_personal_or_shared_assets_with_the_wallet_s_approval_and_d_851a18f)) }
        items(wallets.valueOrNull().orEmpty(), key = { it.id }) { value ->
            Card(Modifier.fillMaxWidth().clickable { client.navigation.push(Route("wallet", value.id)) }) {
                Column(Modifier.padding(18.dp)) { Text(value.name, style = MaterialTheme.typography.titleMedium); Text(value.id, style = MaterialTheme.typography.bodySmall) }
            }
        }
        if (wallets is ReadState.Empty) item { EmptyPanel(str(R.string.text_no_wallets_in_this_account_f1c61ea)) }
        item {
            SectionTitle(str(R.string.text_join_an_existing_wallet_88a36c6))
            OutlinedTextField(wallet, { wallet = it }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_wallet_canister_id_a1a88b6)) }, singleLine = true)
            Button(onClick = { client.wallets.join(wallet) }, enabled = wallet.isNotBlank() && wallet.length <= 63 && !state.working("approval:request:wallet:join:" + wallet)) { Text(str(R.string.text_review_joining_wallet_e505988)) }
            AccountRow(str(R.string.text_hardware_cosigners_dbe97a0)) { client.navigation.push(Route("wallet-cosigners")) }
        }
        item { SectionTitle(str(R.string.text_unconfirmed_proposals_a6e0a01)); ReadFeedback(pending, client.wallets::pending) }
        items(pending.valueOrNull().orEmpty(), key = { it.id }) { item ->
            Card {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { Text(item.wallet); Text(item.id, style = MaterialTheme.typography.bodySmall)  } }
                    WalletIntentContent(item.intent)
                    OutlinedButton(onClick = { client.wallets.resume(item) }, enabled = !state.working("wallet:submit:pending:" + item.id)) { Text(str(R.string.text_reconcile_original_proposal_5974116)) }
                }
            }
        }
    }
}
@Composable fun WalletScreen(client: ClientController, wallet: String) {
    val details by client.wallets.details.state.collectAsStateWithLifecycle()
    val history by client.wallets.history.state.collectAsStateWithLifecycle()
    val balances by client.wallets.balances.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var limit by remember(wallet) { mutableIntStateOf(30) }
    LaunchedEffect(wallet, state.epoch) { client.wallets.select(wallet) }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item { ReadFeedback(details, { client.wallets.select(wallet, true) }); SelectionContainer { Text(wallet, style = MaterialTheme.typography.bodySmall) } }
        details.valueOrNull()?.takeIf { it.id == wallet }?.let { value ->
            item {
                Text(value.name, style = MaterialTheme.typography.headlineSmall)
                Text(if (value.frozen) str(R.string.text_frozen_427478b) else str(R.string.text_wallet_available_87d44fd), style = MaterialTheme.typography.labelLarge)
                value.me?.let { LabelValue(str(R.string.text_your_role_2f39a6c), walletRole(it.role) + " · " + it.status) }
                if (value.canPropose && !value.frozen) AccountRow(str(R.string.text_propose_transfer_275bacf)) { client.navigation.push(Route("wallet-transfer", wallet)) }
                AccountRow(str(R.string.text_wallet_controls_8173ebf)) { client.navigation.push(Route("wallet-controls", wallet)) }
                AccountRow(str(R.string.text_this_device_s_wallet_keys_41832c8)) { client.navigation.push(Route("wallet-keys", wallet)) }
                SectionTitle(str(R.string.text_receiving_addresses_3fc4d1e))
                value.addresses.filterNot { it.archived }.forEach { address ->
                    Text(address.label.ifBlank { address.chain }, style = MaterialTheme.typography.titleSmall)
                    SelectionContainer { Text(address.text) }
                    if (address.verified) TextButton(onClick = { client.clipboard.copy(address.text) }) { Text(str(R.string.text_copy_verified_address_2f7f0e4)) }
                    else Notice(str(R.string.text_address_verification_failed_b36aa77), true)
                }
                OutlinedButton(onClick = { client.wallets.balances(wallet) }, enabled = !state.working("wallet:display:balances")) { Text(str(R.string.text_refresh_balances_c045967)) }
                if (balances !is ReadState.Idle) ReadFeedback(balances, { client.wallets.balances(wallet) })
                balances.valueOrNull()?.takeIf { it.wallet == wallet }?.let { view ->
                    Text(str(R.string.text_balance_read_at_bbbadd3) + displayTime(view.readAt), style = MaterialTheme.typography.labelSmall)
                    view.values.forEach { balance ->
                        val available = balance.atoms != null && balance.asset.decimals != null && balance.asset.error == null && balance.error == null
                        LabelValue((balance.asset.symbol ?: balance.asset.key) + " · #" + balance.address, if (available) AtomicAmount.display(balance.atoms!!, balance.asset.decimals!!) else str(R.string.text_unavailable_4ae32d2))
                        (balance.error ?: balance.asset.error)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error) }
                    }
                }
            }
            item {
                SectionTitle(str(R.string.text_members_and_policy_898078a))
                LabelValue(str(R.string.text_spending_approvals_1f20bcf), value.policy.approvers.toString())
                LabelValue(str(R.string.text_administrative_approvals_9bcbce8), value.policy.administrators.toString())
                value.members.forEach { member -> Text(member.label.ifBlank { member.id }, style = MaterialTheme.typography.titleSmall); SelectionContainer { Text(member.id, style = MaterialTheme.typography.bodySmall) }; Text(walletRole(member.role) + " · " + member.status) }
            }
        }
        item { SectionTitle(str(R.string.text_proposals_e12178d)); ReadFeedback(history, { client.wallets.select(wallet, true) }) }
        items(history.valueOrNull()?.proposals.orEmpty().take(limit), key = { it.id.toString() }) { proposal ->
            ListItem(headlineContent = { Text(str(R.string.text_proposal_4301b5a) + proposal.id) }, supportingContent = { Text(walletProposalState(proposal.status)) }, modifier = Modifier.clickable { client.navigation.push(Route("wallet-proposal", wallet, sequence = proposal.id)) })
        }
        if ((history.valueOrNull()?.proposals?.size ?: 0) > limit) item { TextButton(onClick = { limit += 30 }) { Text(str(R.string.text_load_earlier_proposals_8040d65)) } }
    }
}
@Composable fun WalletTransferScreen(client: ClientController, wallet: String) {
    val forms by client.wallets.transfers.collectAsStateWithLifecycle()
    val previews by client.wallets.previews.collectAsStateWithLifecycle()
    val submissions by client.wallets.submissions.collectAsStateWithLifecycle()
    val errors by client.wallets.errors.collectAsStateWithLifecycle()
    val assets by client.wallets.assets.state.collectAsStateWithLifecycle()
    val details by client.wallets.details.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val draft = forms[wallet] ?: WalletTransferDraft(wallet)
    val preview = previews[wallet]
    val submission = submissions["transfer:" + wallet]
    val locked = submission?.attempted == true || state.working("wallet:submit:transfer:" + wallet)
    var confirming by remember(wallet) { mutableStateOf(false) }
    LaunchedEffect(wallet, state.epoch) { client.wallets.transfer(wallet) }
    val asset = assets.valueOrNull()?.find { it.key == draft.asset }
    val amount = asset?.decimals?.let { AtomicAmount.parse(draft.amount, it) }
    Column(Modifier.fillMaxSize()) {
    LazyColumn(Modifier.weight(1f).testTag("wallet-transfer"), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            ReadFeedback(assets, { client.wallets.transfer(wallet) }); ReadFeedback(details, { client.wallets.select(wallet, true) })
            SelectionContainer { Text(wallet, style = MaterialTheme.typography.bodySmall) }
            SectionTitle(str(R.string.text_asset_b2cb743))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                assets.valueOrNull().orEmpty().forEach { item -> FilterChip(draft.asset == item.key, { client.wallets.edit(draft.copy(asset = item.key)) }, enabled = !locked && item.error == null && item.decimals != null, label = { Text((item.symbol ?: item.key) + " · " + item.key) }) }
            }
            if (asset != null && (asset.error != null || asset.decimals == null)) Notice(str(R.string.text_asset_precision_is_unavailable_refresh_before_entering_an_amount_be6074e), true)
            SectionTitle(str(R.string.text_source_address_b68a3f4))
            details.valueOrNull()?.addresses.orEmpty().filter { it.verified && !it.archived && it.chain == asset?.chain }.forEach { address ->
                RadioChoiceRow(address.label.ifBlank { address.text }, draft.from == address.id, enabled = !locked, subtitle = address.text) { client.wallets.edit(draft.copy(from = address.id)) }
            }
            OutlinedTextField(draft.recipient, { client.wallets.edit(draft.copy(recipient = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_recipient_address_f218971)) }, enabled = !locked)
            OutlinedTextField(draft.amount, { client.wallets.edit(draft.copy(amount = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_amount_bfc0adf)) }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal), enabled = !locked)
            if (draft.amount.isNotEmpty() && amount == null) Notice(str(R.string.text_enter_an_exact_amount_within_the_asset_s_precision_6e00b69), true)
            amount?.let { LabelValue(str(R.string.text_base_units_6187b3a), it) }
            if (client.wallets.feePrecision(wallet) != null) OutlinedTextField(draft.maxFee, { client.wallets.edit(draft.copy(maxFee = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_maximum_fee_789f0df)) }, placeholder = { Text(str(R.string.text_suggested_fee_1939c10)) }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal), enabled = !locked)
            Text(str(R.string.text_previewing_does_not_create_a_proposal_or_move_funds_execution_sti_25530bb), style = MaterialTheme.typography.bodySmall)
            errors["transfer:" + wallet]?.let { Notice(errorText(it), true) }
        }
        preview?.let { value ->
            item {
                SectionTitle(str(R.string.text_reviewed_transfer_2d20332))
                LabelValue(str(R.string.text_balance_e54eeed), asset?.decimals?.let { AtomicAmount.display(value.assetBalance, it) }.orEmpty())
                LabelValue(str(R.string.text_estimated_fee_4475126), AtomicAmount.display(value.estimatedFee, value.feeDecimals) + " " + value.feeAsset)
                LabelValue(str(R.string.text_maximum_fee_789f0df), AtomicAmount.display(value.maxFee, value.feeDecimals) + " " + value.feeAsset)
                if (value.reserve != "0") LabelValue(str(R.string.text_additional_reserve_7f6ee37), AtomicAmount.display(value.reserve, value.feeDecimals))
                LabelValue(str(R.string.text_required_approvals_ebd8da6), value.approvals.toString())
                LabelValue(str(R.string.text_approval_role_f6186e1), walletRole(if (value.admin) "Admin" else "Approver"))
                LabelValue(str(R.string.text_preview_read_at_53f938c), displayTime(value.readAt))
                if (value.delay > 0u) Text(str(R.string.text_policy_delay_0fcad36) + quantity(R.plurals.minutes, value.delay / 60_000u))
                value.blockers.forEach { Notice(errorText(it), true) }
            }
        }
    }
    Surface {
        Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp)) {
            if (submission?.result != null) WalletSubmissionStatus(client, "transfer:" + wallet, wallet)
            else if (submission?.attempted == true) {
                Text(str(R.string.text_check_the_original_request_before_submitting_anything_new_cd7ce38), style = MaterialTheme.typography.bodySmall)
                Button(onClick = { client.wallets.submitTransfer(wallet) }, modifier = Modifier.fillMaxWidth(), enabled = !state.working("wallet:submit:transfer:" + wallet)) { Text(str(R.string.text_reconcile_original_proposal_5974116)) }
            } else if (preview?.blockers?.isEmpty() == true) {
                Button(onClick = { confirming = true }, modifier = Modifier.fillMaxWidth(), enabled = !state.working("wallet:preview:" + wallet)) { Text(str(R.string.text_create_transfer_proposal_b198142)) }
            } else Button(onClick = { client.wallets.preview(wallet) }, modifier = Modifier.fillMaxWidth(), enabled = client.wallets.input(draft) != null && !state.working("wallet:preview:" + wallet)) { Text(str(R.string.text_preview_transfer_4baa969)) }
        }
    }
    }
    if (confirming) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_create_this_transfer_proposal_52293a6)) }, text = {
        Column { SelectionContainer { Text(draft.recipient) }; Text(draft.amount + " " + (asset?.symbol ?: draft.asset)); Text(str(R.string.text_the_reviewed_maximum_fee_remains_fixed_this_does_not_execute_the_3bcde84)) }
    }, confirmButton = { TextButton(onClick = { confirming = false; client.wallets.submitTransfer(wallet) }) { Text(str(R.string.text_create_proposal_4946bd1)) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
@Composable internal fun WalletIntentContent(value: kotlinx.serialization.json.JsonObject) {
    // Complete canonical details remain inspectable for uncommon proposal kinds.
    Text(walletIntentTitle(value.required("kind")), style = MaterialTheme.typography.titleMedium)
    HumanRecord(value)
}
internal fun walletRole(role: String) = when (role) { "Admin" -> str(R.string.text_administrator_f8e6ab3); "Approver" -> str(R.string.text_approver_43b6413); "Proposer" -> str(R.string.text_proposer_738e11a); "Viewer" -> str(R.string.text_viewer_8a8ac26); "Recovery" -> str(R.string.text_recovery_b51aee4); else -> role }
internal fun walletIntentTitle(kind: String) = when (kind) { "transfer" -> str(R.string.text_transfer_ed3786e); "set_policy" -> str(R.string.text_change_policy_43317e3); "add_member" -> str(R.string.text_add_member_44ea6cc); "update_member" -> str(R.string.text_update_member_a6345ca); "remove_member" -> str(R.string.text_remove_member_9443c64); "set_cosign" -> str(R.string.text_change_cosigning_policy_df8a82e); "unfreeze" -> str(R.string.text_unfreeze_wallet_4782fca); else -> kind.replace('_', ' ') }
internal fun walletProposalState(state: String) = when (state) {
    "Open" -> str(R.string.text_awaiting_approvals_94a0931); "Delayed" -> str(R.string.text_waiting_for_policy_delay_7d2eaa2); "Ready" -> str(R.string.text_ready_to_execute_19eeb90)
    "Executing" -> str(R.string.text_execution_in_progress_777231f); "Signed" -> str(R.string.text_signed_not_yet_broadcast_47c6b06); "Broadcast" -> str(R.string.text_broadcast_awaiting_settlement_412b03b); "Settled" -> str(R.string.text_settled_bf4fec2)
    "Failed" -> str(R.string.text_failed_948e2ae); "Rejected" -> str(R.string.text_rejected_056421e); "Vetoed" -> str(R.string.text_vetoed_a0ea43a); "Cancelled" -> str(R.string.text_cancelled_bf41d7d); "Expired" -> str(R.string.text_expired_70446d7); "Stale" -> str(R.string.text_policy_changed_9d62739); else -> state
}
