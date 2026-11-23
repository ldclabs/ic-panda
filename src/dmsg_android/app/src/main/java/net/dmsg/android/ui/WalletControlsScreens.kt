package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import android.content.Intent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import net.dmsg.core.WalletChange

@Composable fun WalletProposalScreen(client: ClientController, wallet: String, id: ULong) {
    val proposal by client.wallets.proposal.state.collectAsStateWithLifecycle()
    val details by client.wallets.details.state.collectAsStateWithLifecycle()
    val signatures by client.wallets.cosign.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    var packet by remember(wallet, id) { mutableStateOf("") }
    var confirming by remember(wallet, id) { mutableStateOf<String?>(null) }
    LaunchedEffect(wallet, id, state.epoch) { client.wallets.select(wallet); client.wallets.proposal(wallet, id) }
    val current = proposal.valueOrNull()?.takeIf { it.id == id }
    val working = state.working("wallet:execute:" + wallet + ":" + id) || state.working("wallet:import-packet")
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item { ReadFeedback(proposal, { client.wallets.proposal(wallet, id) }); Text(str(R.string.text_proposal_4301b5a) + id, style = MaterialTheme.typography.headlineSmall); SelectionContainer { Text(wallet) } }
        current?.let { value ->
            item {
                Text(walletProposalState(value.status), style = MaterialTheme.typography.titleLarge, modifier = if (proposal is ReadState.Ready) Modifier.reminderVisible(client, NoticeTarget("wallet", id.toString(), wallet), revision = value) else Modifier)
                WalletIntentContent(value.intent)
                LabelValue(str(R.string.text_proposer_397122c), value.proposer)
                LabelValue(str(R.string.text_required_approvals_ebd8da6), value.approvals.toString())
                LabelValue(str(R.string.text_approval_role_f6186e1), walletRole(if (value.admin) "Admin" else "Approver"))
                LabelValue(str(R.string.text_expires_12414d7), displayTime(value.expires))
                value.ready?.let { LabelValue(str(R.string.text_available_after_753cb73), displayTime(it)) }
                value.failure?.let { Notice(errorText(it), true) }
                SectionTitle(str(R.string.text_reviewed_digest_ea1e6f9))
                SelectionContainer { Text(value.digest, style = MaterialTheme.typography.bodySmall) }
                if (value.record.rows("votes").isNotEmpty()) { SectionTitle(str(R.string.text_member_decisions_8d95e95)); HumanRecord(value.record["votes"]!!) }
                if (value.record.rows("signed").isNotEmpty()) { SectionTitle(str(R.string.text_signed_plans_dc7eb36)); HumanRecord(value.record["signed"]!!) }
                if (value.record.rows("broadcasts").isNotEmpty()) { SectionTitle(str(R.string.text_broadcast_history_f561fda)); HumanRecord(value.record["broadcasts"]!!) }
                value.record["outcome"]?.takeIf { it !is kotlinx.serialization.json.JsonNull }?.let { SectionTitle(str(R.string.text_execution_result_6668869)); HumanRecord(it) }
            }
            signatures.valueOrNull()?.let { signing ->
                item {
                    SectionTitle(str(R.string.text_device_cosignatures_d2b01c6))
                    ReadFeedback(signatures, { client.wallets.cosign(wallet, id, value.plans) })
                    LabelValue(str(R.string.text_collected_required_4d32c42), signing.collected.toString() + " / " + signing.required)
                    if (signing.stale.isNotEmpty()) Notice(str(R.string.text_these_cosigners_changed_keys_collect_new_signatures_c79e210) + signing.stale.joinToString())
                    if (signing.canCosign == true) OutlinedButton(onClick = { client.wallets.cosignHere(wallet, id, value.plans) }, enabled = !working) { Text(str(R.string.text_review_cosigning_on_this_device_43f69ff)) }
                    else Text(str(R.string.text_this_device_has_no_matching_cosigner_key_b3c2786), style = MaterialTheme.typography.bodySmall)
                    if (signing.collected > 0u) TextButton(onClick = {
                        client.beginExternalInteraction()
                        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, signing.packet) }, null))
                    }) { Text(str(R.string.text_share_signature_packet_5b2b8a4)) }
                    OutlinedTextField(packet, { if (it.length <= 256 * 1024) packet = it }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_signature_packet_from_another_device_cbbf123)) }, minLines = 2, maxLines = 5)
                    OutlinedButton(onClick = { client.wallets.importPacket(wallet, id, value.plans, packet.trim()) }, enabled = !working && packet.isNotBlank()) { Text(str(R.string.text_import_signature_packet_76bdd76)) }
                    Text(str(R.string.text_importing_collects_signatures_execution_is_a_separate_action_8bc9b79), style = MaterialTheme.typography.bodySmall)
                    Button(onClick = { confirming = "cosigned" }, enabled = signing.ready && signatures is ReadState.Ready && !working) { Text(str(R.string.text_execute_with_collected_signatures_9f128ac)) }
                }
            }
            item {
                val fresh = proposal is ReadState.Ready && !working
                val me = details.valueOrNull()?.takeIf { it.id == wallet }
                if (value.status == "Open" && me?.canDecide == true) {
                    OutlinedButton(onClick = { client.wallets.decide(wallet, id, true) }, enabled = fresh) { Text(str(R.string.text_review_approval_db00f36)) }
                    OutlinedButton(onClick = { client.wallets.decide(wallet, id, false) }, enabled = fresh) { Text(str(R.string.text_review_rejection_cd501eb)) }
                }
                if (value.status == "Ready") Button(onClick = { confirming = "execute" }, enabled = fresh) { Text(str(R.string.text_execute_approved_proposal_99a02a0)) }
                if (value.status in setOf("Signed", "Broadcast")) OutlinedButton(onClick = { confirming = "broadcast" }, enabled = fresh) { Text(str(R.string.text_broadcast_original_signed_transaction_a9260d4)) }
                if (value.status in setOf("Executing", "Signed", "Broadcast")) OutlinedButton(onClick = { client.wallets.run(wallet, id, "reconcile") }, enabled = fresh) { Text(str(R.string.text_reconcile_original_execution_6835496)) }
                if (value.status in setOf("Open", "Delayed", "Ready") && me?.me?.id == value.proposer) TextButton(onClick = { confirming = "cancel" }, enabled = fresh) { Text(str(R.string.text_cancel_proposal_393da17), color = MaterialTheme.colorScheme.error) }
            }
        }
    }
    confirming?.let { action ->
        AlertDialog(onDismissRequest = { confirming = null }, title = { Text(when (action) { "cancel" -> str(R.string.text_cancel_this_proposal_d1921f1); "broadcast" -> str(R.string.text_broadcast_the_original_transaction_3a87306); else -> str(R.string.text_execute_this_approved_proposal_e012aa8) }) }, text = {
            Column { SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { Text(wallet); Text("#" + id); current?.let { Text(it.digest, style = MaterialTheme.typography.bodySmall) }  } }; Text(str(R.string.text_the_wallet_rechecks_the_current_policy_and_approvals_any_hardware_e74deef)) }
        }, confirmButton = { TextButton(onClick = { confirming = null; if (action == "cosigned") current?.let { client.wallets.executeCosigned(wallet, id, it.plans) } else client.wallets.run(wallet, id, action) }) { Text(str(R.string.text_confirm_563fd5a)) } }, dismissButton = { TextButton(onClick = { confirming = null }) { Text(str(R.string.text_back_5db5cac)) } })
    }
}

@Composable fun WalletControlsScreen(client: ClientController, wallet: String) {
    val details by client.wallets.details.state.collectAsStateWithLifecycle()
    val controls by client.wallets.controls.collectAsStateWithLifecycle()
    val submissions by client.wallets.submissions.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val current = details.valueOrNull()?.takeIf { it.id == wallet }
    LaunchedEffect(wallet, state.epoch) { client.wallets.select(wallet) }
    LaunchedEffect(current) { current?.let(client.wallets::initializeControls) }
    val form = controls[wallet] ?: WalletControlDraft()
    val submission = submissions["controls:" + wallet]
    val writable = current?.canPropose == true && details is ReadState.Ready && submission?.attempted != true && !state.working("wallet:freeze:" + wallet)
    val memberWritable = current?.canProposeMembers == true && details is ReadState.Ready && submission?.attempted != true
    val threshold = form.threshold.toUByteOrNull()?.takeIf { it.toInt() in 1..64 }
    val cosignValid = form.cosigners.isEmpty() || threshold != null && threshold.toInt() <= form.cosigners.size
    var confirming by remember { mutableStateOf<WalletChange?>(null) }
    var freezing by remember { mutableStateOf(false) }
    fun edit(value: WalletControlDraft) = client.wallets.editControls(wallet, value)
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item { ReadFeedback(details, { client.wallets.select(wallet, true) }); Notice(str(R.string.text_each_change_creates_a_proposal_under_the_wallet_s_existing_rules_47f37be)) }
        current?.let { value ->
            item {
                SectionTitle(str(R.string.text_receiving_addresses_3fc4d1e))
                value.addresses.forEach { address ->
                    Text(address.label.ifBlank { address.chain }, style = MaterialTheme.typography.titleMedium)
                    SelectionContainer { Text(address.text) }
                    if (address.verified) OutlinedButton(onClick = { confirming = WalletChange.SetCosign(address.id, form.cosigners.sorted(), threshold ?: 1u) }, enabled = writable && cosignValid) { Text(if (form.cosigners.isEmpty()) str(R.string.text_propose_removing_additional_cosigning_05d47fb) else str(R.string.text_propose_cosigning_policy_01d4494)) }
                    else Notice(str(R.string.text_address_verification_failed_b36aa77), true)
                }
                SectionTitle(str(R.string.text_cosigners_for_the_proposed_policy_aff3355))
                value.cosigners.forEach { signer ->
                    SelectionRow(signer.label.ifBlank { "#" + signer.id }, signer.id in form.cosigners, enabled = writable) { chosen -> edit(form.copy(cosigners = if (chosen) form.cosigners + signer.id else form.cosigners - signer.id)) }
                    TextButton(onClick = { confirming = WalletChange.RemoveCosigner(signer.id) }, enabled = writable) { Text(str(R.string.text_propose_removing_cosigner_d06f252)) }
                }
                OutlinedTextField(form.threshold, { edit(form.copy(threshold = it)) }, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_required_cosignatures_1_64_18e5e0a)) }, singleLine = true, enabled = writable)
                Text(str(R.string.text_selecting_no_cosigners_explicitly_removes_the_additional_cosignin_58c5106), style = MaterialTheme.typography.bodySmall)
                SectionTitle(str(R.string.text_add_receiving_address_a0c8782))
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { listOf("Icp", "Bitcoin", "Evm", "Solana").forEach { chain -> FilterChip(form.chain == chain, { edit(form.copy(chain = chain)) }, enabled = writable, label = { Text(chain) }) } }
                OutlinedTextField(form.addressLabel, { edit(form.copy(addressLabel = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_address_label_3285031)) }, enabled = writable)
                OutlinedButton(onClick = { confirming = WalletChange.AddAddress(form.chain, form.addressLabel, form.cosigners.sorted(), threshold ?: 1u) }, enabled = writable && cosignValid) { Text(str(R.string.text_propose_address_8d3b2df)) }
            }
            item { SectionTitle(str(R.string.text_members_437a10e)) }
            items(value.members, key = { it.id }) { member ->
                Column { Text(member.label.ifBlank { member.id }, style = MaterialTheme.typography.titleMedium); SelectionContainer { Text(member.id) }; Text(walletRole(member.role) + " · " + member.status)
                    Row {
                        TextButton(onClick = { edit(form.copy(member = member.id, memberLabel = member.label, role = member.role)) }, enabled = memberWritable) { Text(str(R.string.text_edit_member_8fa4135)) }
                        TextButton(onClick = { confirming = WalletChange.RemoveMember(member.id) }, enabled = memberWritable) { Text(str(R.string.text_propose_removal_4c7a264)) }
                    }
                }
            }
            item {
                OutlinedTextField(form.member, { edit(form.copy(member = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_member_account_or_principal_8693292)) }, enabled = memberWritable)
                OutlinedTextField(form.memberLabel, { edit(form.copy(memberLabel = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_member_label_3a9b131)) }, enabled = memberWritable)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { listOf("Admin", "Approver", "Proposer", "Viewer", "Recovery").forEach { role -> FilterChip(form.role == role, { edit(form.copy(role = role)) }, enabled = memberWritable, label = { Text(walletRole(role)) }) } }
                OutlinedButton(onClick = { confirming = if (value.members.any { it.id == form.member.trim() }) WalletChange.UpdateMember(form.member.trim(), form.role, form.memberLabel) else WalletChange.AddMember(form.member.trim(), form.role, form.memberLabel) }, enabled = memberWritable && form.member.isNotBlank()) { Text(str(R.string.text_propose_member_change_d86bf75)) }
                SectionTitle(str(R.string.text_approval_thresholds_002a730))
                OutlinedTextField(form.approvers, { edit(form.copy(approvers = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_spending_approvals_1f20bcf)) }, singleLine = true, enabled = writable)
                OutlinedTextField(form.administrators, { edit(form.copy(administrators = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_administrative_approvals_9bcbce8)) }, singleLine = true, enabled = writable)
                val approvers = form.approvers.toUByteOrNull()?.takeIf { it.toInt() in 1..64 }
                val admins = form.administrators.toUByteOrNull()?.takeIf { it.toInt() in 1..64 }
                OutlinedButton(onClick = { confirming = WalletChange.ApprovalThresholds(approvers!!, admins!!) }, enabled = writable && approvers != null && admins != null) { Text(str(R.string.text_propose_thresholds_6554e5a)) }
                if (value.frozen) Button(onClick = { confirming = WalletChange.Unfreeze }, enabled = writable) { Text(str(R.string.text_propose_unfreezing_d275134)) }
                else TextButton(onClick = { freezing = true }, enabled = value.canDecide && details is ReadState.Ready) { Text(str(R.string.text_freeze_wallet_b3bf77c), color = MaterialTheme.colorScheme.error) }
                WalletSubmissionStatus(client, "controls:" + wallet, wallet)
            }
        }
    }
    confirming?.let { change -> AlertDialog(onDismissRequest = { confirming = null }, title = { Text(str(R.string.text_create_this_wallet_proposal_08ed869)) }, text = { WalletChangeReview(change) }, confirmButton = { TextButton(onClick = { confirming = null; client.wallets.change(wallet, change) }) { Text(str(R.string.text_create_proposal_4946bd1)) } }, dismissButton = { TextButton(onClick = { confirming = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
    if (freezing) AlertDialog(onDismissRequest = { freezing = false }, title = { Text(str(R.string.text_freeze_this_wallet_887f613)) }, text = { SelectionContainer { Text(wallet) } }, confirmButton = { TextButton(onClick = { freezing = false; client.wallets.freeze(wallet) }) { Text(str(R.string.text_freeze_wallet_b3bf77c)) } }, dismissButton = { TextButton(onClick = { freezing = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
@Composable internal fun WalletSubmissionStatus(client: ClientController, key: String, wallet: String) {
    val submissions by client.wallets.submissions.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    submissions[key]?.let { submission ->
        submission.error?.let { Notice(errorText(it), true) }
        if (submission.result != null) {
            Text(str(R.string.text_proposal_created_a7589ff))
            TextButton(onClick = { client.navigation.push(Route("wallet-proposal", wallet, sequence = submission.result.requiredUnsigned("id"))) }) { Text(str(R.string.text_view_proposal_a94973a)) }
            TextButton(onClick = { client.wallets.newSubmission(key) }) { Text(str(R.string.text_start_another_proposal_f51acf2)) }
        } else if (submission.attempted) {
            SelectionContainer { Text(submission.request, style = MaterialTheme.typography.bodySmall) }
            Button(onClick = { client.wallets.submit(key, wallet) { _, _ -> error("EXECUTION_UNKNOWN") } }, enabled = !state.working("wallet:submit:" + key)) { Text(str(R.string.text_reconcile_original_proposal_5974116)) }
        }
    }
}
@Composable private fun WalletChangeReview(change: WalletChange) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        when (change) {
            is WalletChange.AddAddress -> { Text(str(R.string.text_add_address_73149b0)); Text(change.chain); Text(change.label); Text(str(R.string.text_cosigners_3368e16) + change.cosigners.joinToString()); Text(str(R.string.text_threshold_cf7cbe2) + change.threshold) }
            is WalletChange.SetCosign -> { Text(str(R.string.text_address_e63ee43) + change.address); if (change.cosigners.isEmpty()) Notice(str(R.string.text_remove_the_additional_cosigning_requirement_3d48325), true) else { Text(str(R.string.text_cosigners_3368e16) + change.cosigners.joinToString()); Text(str(R.string.text_threshold_cf7cbe2) + change.threshold) } }
            is WalletChange.AddMember -> { Text(str(R.string.text_add_member_44ea6cc)); SelectionContainer { Text(change.identity) }; Text(change.label); Text(walletRole(change.role)) }
            is WalletChange.UpdateMember -> { Text(str(R.string.text_update_member_a6345ca)); SelectionContainer { Text(change.identity) }; Text(change.label); Text(walletRole(change.role)) }
            is WalletChange.RemoveMember -> { Text(str(R.string.text_remove_member_9443c64)); SelectionContainer { Text(change.identity) } }
            is WalletChange.RemoveCosigner -> Text(str(R.string.text_remove_cosigner_99be9eb) + change.cosigner)
            is WalletChange.ApprovalThresholds -> { LabelValue(str(R.string.text_spending_approvals_1f20bcf), change.approvers.toString()); LabelValue(str(R.string.text_administrative_approvals_9bcbce8), change.administrators.toString()) }
            WalletChange.Unfreeze -> Text(str(R.string.text_unfreeze_wallet_under_the_existing_policy_delay_4aff5de))
        }
    }
}
