package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun WalletCosignersScreen(client: ClientController) {
    val keys by client.wallets.localKeys.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val submissions by client.wallets.submissions.collectAsStateWithLifecycle()
    var label by remember { mutableStateOf("") }
    var wallet by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf(false) }
    LaunchedEffect(state.epoch) { client.wallets.keys() }
    val existing = keys.valueOrNull()?.find { it.label == label }
    val attempted = submissions["register-key:" + label]?.attempted == true
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            ReadFeedback(keys, client.wallets::keys)
            Text(str(R.string.text_a_hardware_cosigner_key_stays_in_this_device_s_keystore_register_627a57d))
            SectionTitle(str(R.string.text_keys_on_this_device_4d35fcf))
            keys.valueOrNull().orEmpty().forEach { key ->
                Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp)) {
                    Text(key.label, style = MaterialTheme.typography.titleMedium)
                    Text(if (key.hardware) str(R.string.text_hardware_key_a9a7fa0) else str(R.string.text_software_key_769216f))
                    SelectionContainer { Text(key.publicKey, style = MaterialTheme.typography.bodySmall) }
                    TextButton(onClick = { label = key.label }, enabled = !attempted) { Text(str(R.string.text_select_this_key_78ccea8)) }
                    TextButton(onClick = { client.wallets.testKey(key.label) }, enabled = !state.working("wallet:test-key")) { Text(str(R.string.text_test_this_key_ce34ec8)) }
                } }
            }
            OutlinedTextField(label, { label = it }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_key_label_666f49a)) }, enabled = !attempted, singleLine = true)
            OutlinedButton(onClick = { client.wallets.newKey(label) }, enabled = !attempted && label.isNotBlank() && label.length <= 80 && existing == null && !state.working("wallet:new-key")) { Text(str(R.string.text_create_hardware_cosigner_9914eec)) }
            OutlinedTextField(wallet, { wallet = it }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_wallet_canister_id_a1a88b6)) }, enabled = !attempted, singleLine = true)
            if (!attempted) Button(onClick = { confirm = true }, enabled = existing != null && wallet.isNotBlank() && wallet.length <= 63) { Text(str(R.string.text_propose_registering_this_key_8931d6d)) }
            WalletSubmissionStatus(client, "register-key:" + label, wallet)
        }
    }
    if (confirm) AlertDialog(onDismissRequest = { confirm = false }, title = { Text(str(R.string.text_register_this_hardware_cosigner_5256a5f)) }, text = {
        Column { SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { Text(wallet); Text(existing?.publicKey.orEmpty())  } }; Text(label); Text(str(R.string.text_this_creates_a_proposal_it_does_not_replace_the_wallet_s_approval_b19b5c7)) }
    }, confirmButton = { TextButton(onClick = { confirm = false; client.wallets.registerKey(wallet.trim(), label) }) { Text(str(R.string.text_create_proposal_4946bd1)) } }, dismissButton = { TextButton(onClick = { confirm = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
@Composable fun WalletDeviceKeysScreen(client: ClientController, wallet: String) {
    val native by client.wallets.native.state.collectAsStateWithLifecycle()
    val keys by client.wallets.localKeys.state.collectAsStateWithLifecycle()
    val details by client.wallets.details.state.collectAsStateWithLifecycle()
    val bindings by client.wallets.bindings.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var biometry by remember { mutableStateOf(false) }
    var action by remember { mutableStateOf<String?>(null) }
    var protector by remember { mutableStateOf<UInt?>(null) }
    var replacing by remember { mutableStateOf<UInt?>(null) }
    var label by remember { mutableStateOf("") }
    val busy = state.working("wallet:native-change")
    val value = native.valueOrNull()
    val walletDetails = details.valueOrNull()?.takeIf { it.id == wallet }
    LaunchedEffect(wallet, state.epoch) { client.wallets.deviceKeys(wallet) }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            ReadFeedback(native, { client.wallets.deviceKeys(wallet) }); ReadFeedback(details, { client.wallets.select(wallet, true) })
            SectionTitle(str(R.string.text_native_taproot_key_741b323))
            Text(str(R.string.text_l1_spends_need_this_device_s_bip_340_key_its_root_is_wrapped_by_a_22fb83d), style = MaterialTheme.typography.bodyMedium)
            value?.let {
                if (it.hardware == "biometrics") NoticeSwitch(str(R.string.text_biometrics_only_eaa354b), biometry, !busy) { choice -> biometry = choice }
                if (!it.created) Button(onClick = { action = "create" }, enabled = native is ReadState.Ready && it.hardware != null && !busy) { Text(str(R.string.text_create_key_root_c7e6566)) }
                else {
                    LabelValue(str(R.string.text_protection_e686a13), if (it.protection == "hardware") str(R.string.text_android_keystore_06568a0) else str(R.string.text_software_9fae690))
                    it.protectors.forEach { item ->
                        Text(str(R.string.text_protector_6e00429) + item.index, style = MaterialTheme.typography.titleSmall)
                        Text(if (item.biometry) str(R.string.text_biometrics_only_eaa354b) else str(R.string.text_system_authentication_0802d32))
                        TextButton(onClick = { protector = item.index; action = "remove" }, enabled = !busy && it.protectors.size > 1 && native is ReadState.Ready) { Text(str(R.string.text_remove_protector_1b8aa9c), color = MaterialTheme.colorScheme.error) }
                    }
                    OutlinedButton(onClick = { action = "add" }, enabled = !busy && native is ReadState.Ready) { Text(str(R.string.text_add_protector_8c4a831)) }
                    LabelValue(str(R.string.text_this_wallet_s_public_key_ec6ea29), it.key ?: str(R.string.text_derived_when_registered_94de52b))
                }
            }
            AccountRow(str(R.string.text_hardware_cosigners_dbe97a0)) { client.navigation.push(Route("wallet-cosigners")) }
        }
        walletDetails?.takeIf { it.canPropose }?.let { walletState ->
            item {
                val movable = walletState.addresses.filter { address -> address.control == "taproot_2of2" && !address.archived && walletState.cosigners.any { it.id == address.cosigner && it.local != true && it.member == walletState.me?.id } }
                if (movable.isNotEmpty()) {
                    SectionTitle(str(R.string.text_move_native_addresses_to_this_device_42b112f))
                    movable.forEach { address -> AccountRow(address.label.ifBlank { address.text }) { client.navigation.push(Route("wallet-native-move", wallet, sequence = address.id.toULong())) } }
                }
                val replaceable = walletState.cosigners.filter { key -> key.member == walletState.me?.id && key.local != true && key.curve != "Bip340" && walletState.addresses.none { it.cosigner == key.id } }
                val spare = keys.valueOrNull().orEmpty().filter { key -> walletState.cosigners.none { it.publicKey == key.publicKey } }
                if (replaceable.isNotEmpty()) {
                    SectionTitle(str(R.string.text_replace_a_lost_device_s_key_1c782e5))
                    Text(str(R.string.text_members_approve_the_replacement_the_wallet_s_policy_delay_still_a_8af0742), style = MaterialTheme.typography.bodySmall)
                    replaceable.forEach { key ->
                        Text(key.label.ifBlank { "#" + key.id }, style = MaterialTheme.typography.titleMedium)
                        SelectionContainer { Text(key.publicKey, style = MaterialTheme.typography.bodySmall) }
                        spare.forEach { replacement -> TextButton(onClick = { replacing = key.id; label = replacement.label; action = "replace" }) { Text(str(R.string.text_replace_with_273a6fd) + replacement.label) } }
                        WalletSubmissionStatus(client, "replace-key:" + wallet + ":" + key.id, wallet)
                    }
                }
            }
        }
        item {
            ReadFeedback(bindings, { client.wallets.deviceKeys(wallet) })
            val unbind = bindings.valueOrNull().orEmpty()
            if (unbind.isNotEmpty()) {
                SectionTitle(str(R.string.text_revoked_device_bindings_3456bd2))
                SelectionContainer { Text(unbind.joinToString("\n")) }
                Text(str(R.string.text_remove_at_most_four_bindings_in_each_approval_wallet_membership_i_55fccbb), style = MaterialTheme.typography.bodySmall)
                OutlinedButton(onClick = { client.wallets.unbind(wallet, unbind) }, enabled = bindings is ReadState.Ready) { Text(str(R.string.text_review_unbinding_5edabd8)) }
            }
        }
    }
    action?.let { chosen -> AlertDialog(onDismissRequest = { action = null }, title = { Text(when (chosen) { "remove" -> str(R.string.text_remove_this_protector_f0ae2bc); "replace" -> str(R.string.text_propose_this_key_replacement_3b412eb); else -> str(R.string.text_update_this_device_s_wallet_keys_c4057fe) }) }, text = {
        Column { SelectionContainer { Text(wallet) }; if (chosen == "replace") Text("#" + replacing + " → " + label); Text(str(R.string.text_hardware_keys_are_not_copied_between_devices_keep_a_working_way_t_d542f2c)) }
    }, confirmButton = { TextButton(onClick = { action = null; if (chosen == "replace") replacing?.let { client.wallets.replaceKey(wallet, it, label) } else client.wallets.nativeKey(wallet, chosen, biometry, protector) }) { Text(str(R.string.text_confirm_563fd5a)) } }, dismissButton = { TextButton(onClick = { action = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}
@Composable fun WalletNativeMoveScreen(client: ClientController, wallet: String, from: UInt) {
    val plan by client.wallets.move.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var l2 by remember(wallet, from) { mutableStateOf(false) }
    var confirm by remember { mutableStateOf(false) }
    LaunchedEffect(wallet, from, l2, state.epoch) { client.wallets.move(wallet, from, l2) }
    val value = plan.valueOrNull()
    val busy = state.working("wallet:move-next")
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            Text(str(R.string.text_the_old_address_commits_to_the_old_device_s_key_register_this_dev_7c7c099))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(!l2, { l2 = false }, enabled = !busy, label = { Text(str(R.string.text_l1_old_device_cosigns_01361d3)) })
                FilterChip(l2, { l2 = true }, enabled = !busy, label = { Text(str(R.string.text_l2_after_delay_36207ca)) })
            }
            ReadFeedback(plan, { client.wallets.move(wallet, from, l2) })
            value?.let { move ->
                LabelValue(str(R.string.text_old_address_70627d4), move.source)
                move.destination?.let { LabelValue(str(R.string.text_new_address_d88a217), it) }
                LabelValue(str(R.string.text_next_step_f524c42), walletMoveStep(move.next))
                if (l2) LabelValue(str(R.string.text_required_block_depth_3641e4e), move.chainKeyAfter.toString())
                else Notice(str(R.string.text_the_old_device_must_cosign_the_l1_sweep_8e6c5a5))
                move.pending?.let { id -> AccountRow(str(R.string.text_proposal_4301b5a) + id + " · " + walletProposalState(move.pendingState.orEmpty())) { client.navigation.push(Route("wallet-proposal", wallet, sequence = id)) } }
                move.cooldown?.let { LabelValue(str(R.string.text_wait_until_d7bc115), displayTime(it)) }
                if (move.next == "root") AccountRow(str(R.string.text_create_this_device_s_key_root_d7c3d57)) { client.navigation.push(Route("wallet-keys", wallet)) }
                move.sweep?.let { sweep ->
                    SectionTitle(str(R.string.text_sweep_preview_b682610))
                    LabelValue(str(R.string.text_chain_tip_c986d2d), sweep.tip.toString())
                    LabelValue(str(R.string.text_fee_rate_885dc55), sweep.feeRate.toString() + " sat/vB")
                    LabelValue(str(R.string.text_selected_inputs_2a8a226), sweep.inputs.toString())
                    LabelValue(str(R.string.text_total_3843d4c), AtomicAmount.display(sweep.total, 8) + " BTC")
                    sweep.fee?.let { LabelValue(str(R.string.text_fee_92d2050), AtomicAmount.display(it, 8) + " BTC") }
                    sweep.amount?.let { LabelValue(str(R.string.text_arrives_0071d70), AtomicAmount.display(it, 8) + " BTC") }
                    sweep.utxos.forEach { utxo ->
                        SelectionContainer { Text(utxo.txid + ":" + utxo.vout, style = MaterialTheme.typography.bodySmall) }
                        Text(AtomicAmount.display(utxo.value, 8) + " BTC")
                        Text(if (!l2 || utxo.spendableAt?.let { it <= sweep.tip || it - sweep.tip == 1uL } == true) str(R.string.text_spendable_now_8f7ddfb) else utxo.spendableAt?.let { str(R.string.text_from_block_d9a486e) + it } ?: str(R.string.text_unconfirmed_a60145e))
                    }
                    if (sweep.dust) Notice(str(R.string.text_no_amount_above_the_dust_limit_can_move_through_this_path_yet_096503d))
                    if (sweep.empty) Text(str(R.string.text_the_old_address_holds_nothing_6094780))
                }
                val enabled = plan is ReadState.Ready && move.next in setOf("register", "address", "book", "sweep", "archive") && !(move.next == "sweep" && move.sweep?.dust == true) && !busy
                Button(onClick = { confirm = true }, enabled = enabled) { Text(if (move.next == "archive") str(R.string.text_archive_old_address_cf85f39) else str(R.string.text_propose_next_step_7edc384)) }
            }
        }
    }
    if (confirm) AlertDialog(onDismissRequest = { confirm = false }, title = { Text(str(R.string.text_continue_the_reviewed_move_428f83b)) }, text = { Column { Text(walletMoveStep(value?.next.orEmpty())); SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { Text(value?.source.orEmpty()); Text(value?.destination.orEmpty())  } }; Text(str(R.string.text_each_transfer_still_needs_its_own_approvals_and_execution_70c66b9)) } }, confirmButton = { TextButton(onClick = { confirm = false; client.wallets.moveNext(wallet, from, l2) }) { Text(str(R.string.text_continue_fd19155)) } }, dismissButton = { TextButton(onClick = { confirm = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
private fun walletMoveStep(step: String) = when (step) {
    "root" -> str(R.string.text_create_device_key_root_5699bc2); "register" -> str(R.string.text_register_this_device_s_key_1265cbd); "address" -> str(R.string.text_create_a_new_address_7cdbf66)
    "book" -> str(R.string.text_add_to_address_book_9064696); "cooldown" -> str(R.string.text_wait_for_address_book_cooldown_2ccc5de); "wait" -> str(R.string.text_wait_for_proposal_782426e)
    "sweep" -> str(R.string.text_sweep_old_address_80041cc); "archive" -> str(R.string.text_archive_old_address_cf85f39); "done" -> str(R.string.text_moved_db6e264); else -> step
}
