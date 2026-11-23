package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
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
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                ReadFeedback(keys, client.wallets::keys)
                Text(str(R.string.text_a_hardware_cosigner_key_stays_in_this_device_s_keystore_register_627a57d),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                SectionTitle(str(R.string.text_keys_on_this_device_4d35fcf))
                keys.valueOrNull().orEmpty().forEach { key ->
                    DetailCard {
                        Text(key.label, style = MaterialTheme.typography.titleMedium)
                        Text(if (key.hardware) str(R.string.text_hardware_key_a9a7fa0) else str(R.string.text_software_key_769216f),
                            style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                        SelectionContainer { Text(key.publicKey, style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(onClick = { label = key.label }, enabled = !attempted) { Text(str(R.string.text_select_this_key_78ccea8)) }
                            TextButton(onClick = { client.wallets.testKey(key.label) }, enabled = !state.working("wallet:test-key")) { Text(str(R.string.text_test_this_key_ce34ec8)) }
                        }
                    }
                }
                DetailCard {
                    OutlinedTextField(label, { label = it }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_key_label_666f49a)) }, enabled = !attempted, singleLine = true)
                    OutlinedButton(onClick = { client.wallets.newKey(label) }, enabled = !attempted && label.isNotBlank() && label.length <= 80 && existing == null && !state.working("wallet:new-key"), modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_create_hardware_cosigner_9914eec))
                    }
                    OutlinedTextField(wallet, { wallet = it }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_wallet_canister_id_a1a88b6)) }, enabled = !attempted, singleLine = true)
                    if (!attempted) Button(onClick = { confirm = true }, enabled = existing != null && wallet.isNotBlank() && wallet.length <= 63, modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_propose_registering_this_key_8931d6d))
                    }
                }
                WalletSubmissionStatus(client, "register-key:" + label, wallet)
            }
        }
    }
    if (confirm) AlertDialog(onDismissRequest = { confirm = false }, title = { Text(str(R.string.text_register_this_hardware_cosigner_5256a5f)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SelectionContainer {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(wallet, style = MaterialTheme.typography.bodySmall.code())
                    Text(existing?.publicKey.orEmpty(), style = MaterialTheme.typography.bodySmall.code())
                }
            }
            Text(label)
            Text(str(R.string.text_this_creates_a_proposal_it_does_not_replace_the_wallet_s_approval_b19b5c7))
        }
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
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                ReadFeedback(native, details, { client.wallets.deviceKeys(wallet) }, { client.wallets.select(wallet, true) })
                SectionTitle(str(R.string.text_native_taproot_key_741b323))
                Text(str(R.string.text_l1_spends_need_this_device_s_bip_340_key_its_root_is_wrapped_by_a_22fb83d),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                value?.let {
                    if (it.hardware == "biometrics") Section {
                        NoticeSwitch(str(R.string.text_biometrics_only_eaa354b), biometry, !busy) { choice -> biometry = choice }
                    }
                    if (!it.created) Button(onClick = { action = "create" }, enabled = native is ReadState.Ready && it.hardware != null && !busy, modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_create_key_root_c7e6566))
                    } else {
                        Section {
                            ListRow(str(R.string.text_protection_e686a13),
                                value = if (it.protection == "hardware") str(R.string.text_android_keystore_06568a0) else str(R.string.text_software_9fae690))
                            if (it.key != null) CodeRow(str(R.string.text_this_wallet_s_public_key_ec6ea29), it.key) { client.clipboard.copy(it.key) }
                            else InfoRow(str(R.string.text_this_wallet_s_public_key_ec6ea29), str(R.string.text_derived_when_registered_94de52b))
                        }
                        it.protectors.forEach { item ->
                            DetailCard {
                                Text(str(R.string.text_protector_6e00429) + item.index, style = MaterialTheme.typography.titleSmall)
                                Text(if (item.biometry) str(R.string.text_biometrics_only_eaa354b) else str(R.string.text_system_authentication_0802d32),
                                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                TextButton(onClick = { protector = item.index; action = "remove" }, enabled = !busy && it.protectors.size > 1 && native is ReadState.Ready) {
                                    Text(str(R.string.text_remove_protector_1b8aa9c), color = MaterialTheme.colorScheme.error)
                                }
                            }
                        }
                        OutlinedButton(onClick = { action = "add" }, enabled = !busy && native is ReadState.Ready, modifier = Modifier.fillMaxWidth()) {
                            Text(str(R.string.text_add_protector_8c4a831))
                        }
                    }
                }
                Section {
                    ListRow(str(R.string.text_hardware_cosigners_dbe97a0), icon = Icons.Outlined.Key) { client.navigation.push(Route("wallet-cosigners")) }
                }
            }
        }
        walletDetails?.takeIf { it.canPropose }?.let { walletState ->
            item {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    val movable = walletState.addresses.filter { address -> address.control == "taproot_2of2" && !address.archived && walletState.cosigners.any { it.id == address.cosigner && it.local != true && it.member == walletState.me?.id } }
                    if (movable.isNotEmpty()) {
                        Section(title = str(R.string.text_move_native_addresses_to_this_device_42b112f)) {
                            movable.forEach { address ->
                                ListRow(address.label.ifBlank { address.text }, icon = Icons.Outlined.SwapHoriz) {
                                    client.navigation.push(Route("wallet-native-move", wallet, sequence = address.id.toULong()))
                                }
                            }
                        }
                    }
                    val replaceable = walletState.cosigners.filter { key -> key.member == walletState.me?.id && key.local != true && key.curve != "Bip340" && walletState.addresses.none { it.cosigner == key.id } }
                    val spare = keys.valueOrNull().orEmpty().filter { key -> walletState.cosigners.none { it.publicKey == key.publicKey } }
                    if (replaceable.isNotEmpty()) {
                        SectionTitle(str(R.string.text_replace_a_lost_device_s_key_1c782e5))
                        Text(str(R.string.text_members_approve_the_replacement_the_wallet_s_policy_delay_still_a_8af0742),
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        replaceable.forEach { key ->
                            DetailCard {
                                Text(key.label.ifBlank { "#" + key.id }, style = MaterialTheme.typography.titleMedium)
                                SelectionContainer { Text(key.publicKey, style = MaterialTheme.typography.bodySmall.code()) }
                                spare.forEach { replacement ->
                                    TextButton(onClick = { replacing = key.id; label = replacement.label; action = "replace" }) {
                                        Text(str(R.string.text_replace_with_273a6fd) + replacement.label)
                                    }
                                }
                            }
                            WalletSubmissionStatus(client, "replace-key:" + wallet + ":" + key.id, wallet)
                        }
                    }
                }
            }
        }
        item {
            ReadFeedback(bindings, { client.wallets.deviceKeys(wallet) })
            val unbind = bindings.valueOrNull().orEmpty()
            if (unbind.isNotEmpty()) {
                DetailCard {
                    Text(str(R.string.text_revoked_device_bindings_3456bd2), style = MaterialTheme.typography.titleMedium)
                    SelectionContainer { Text(unbind.joinToString("\n"), style = MaterialTheme.typography.bodySmall.code()) }
                    Text(str(R.string.text_remove_at_most_four_bindings_in_each_approval_wallet_membership_i_55fccbb),
                        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    OutlinedButton(onClick = { client.wallets.unbind(wallet, unbind) }, enabled = bindings is ReadState.Ready, modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_review_unbinding_5edabd8))
                    }
                }
            }
        }
    }
    action?.let { chosen -> AlertDialog(onDismissRequest = { action = null }, title = { Text(when (chosen) { "remove" -> str(R.string.text_remove_this_protector_f0ae2bc); "replace" -> str(R.string.text_propose_this_key_replacement_3b412eb); else -> str(R.string.text_update_this_device_s_wallet_keys_c4057fe) }) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SelectionContainer { Text(wallet, style = MaterialTheme.typography.bodySmall.code()) }
            if (chosen == "replace") Text("#" + replacing + " → " + label)
            Text(str(R.string.text_hardware_keys_are_not_copied_between_devices_keep_a_working_way_t_d542f2c))
        }
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
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(str(R.string.text_the_old_address_commits_to_the_old_device_s_key_register_this_dev_7c7c099),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                SegmentedChoice(
                    listOf(str(R.string.text_l1_old_device_cosigns_01361d3), str(R.string.text_l2_after_delay_36207ca)),
                    if (l2) 1 else 0,
                    enabled = !busy,
                ) { l2 = it == 1 }
                ReadFeedback(plan, { client.wallets.move(wallet, from, l2) })
                value?.let { move ->
                    when {
                        move.sweep?.dust == true -> Notice(str(R.string.text_no_amount_above_the_dust_limit_can_move_through_this_path_yet_096503d), true)
                        !l2 -> Notice(str(R.string.text_the_old_device_must_cosign_the_l1_sweep_8e6c5a5))
                    }
                    Section {
                        CodeRow(str(R.string.text_old_address_70627d4), move.source) { client.clipboard.copy(move.source) }
                        move.destination?.let { CodeRow(str(R.string.text_new_address_d88a217), it) { client.clipboard.copy(it) } }
                        InfoRow(str(R.string.text_next_step_f524c42), walletMoveStep(move.next))
                        if (l2) ListRow(str(R.string.text_required_block_depth_3641e4e), value = move.chainKeyAfter.toString())
                        move.cooldown?.let { ListRow(str(R.string.text_wait_until_d7bc115), value = displayTime(it)) }
                    }
                    if (move.pending != null || move.next == "root") Section {
                        move.pending?.let { id ->
                            ListRow(str(R.string.text_proposal_4301b5a) + id, supporting = walletProposalState(move.pendingState.orEmpty()), icon = Icons.Outlined.Description) {
                                client.navigation.push(Route("wallet-proposal", wallet, sequence = id))
                            }
                        }
                        if (move.next == "root") ListRow(str(R.string.text_create_this_device_s_key_root_d7c3d57), icon = Icons.Outlined.Key) {
                            client.navigation.push(Route("wallet-keys", wallet))
                        }
                    }
                    move.sweep?.let { sweep ->
                        DetailCard {
                            Text(str(R.string.text_sweep_preview_b682610), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                            LabelValue(str(R.string.text_chain_tip_c986d2d), sweep.tip.toString())
                            LabelValue(str(R.string.text_fee_rate_885dc55), sweep.feeRate.toString() + " sat/vB")
                            LabelValue(str(R.string.text_selected_inputs_2a8a226), sweep.inputs.toString())
                            LabelValue(str(R.string.text_total_3843d4c), AtomicAmount.display(sweep.total, 8) + " BTC")
                            sweep.fee?.let { LabelValue(str(R.string.text_fee_92d2050), AtomicAmount.display(it, 8) + " BTC") }
                            sweep.amount?.let { LabelValue(str(R.string.text_arrives_0071d70), AtomicAmount.display(it, 8) + " BTC") }
                            if (sweep.empty) Text(str(R.string.text_the_old_address_holds_nothing_6094780), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            sweep.utxos.forEach { utxo ->
                                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                                SelectionContainer { Text(utxo.txid + ":" + utxo.vout, style = MaterialTheme.typography.bodySmall.code()) }
                                Text(AtomicAmount.display(utxo.value, 8) + " BTC · " + (if (!l2 || utxo.spendableAt?.let { it <= sweep.tip || it - sweep.tip == 1uL } == true) str(R.string.text_spendable_now_8f7ddfb) else utxo.spendableAt?.let { str(R.string.text_from_block_d9a486e) + it } ?: str(R.string.text_unconfirmed_a60145e)),
                                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    }
                    val enabled = plan is ReadState.Ready && move.next in setOf("register", "address", "book", "sweep", "archive") && !(move.next == "sweep" && move.sweep?.dust == true) && !busy
                    Button(onClick = { confirm = true }, enabled = enabled, modifier = Modifier.fillMaxWidth()) {
                        Text(if (move.next == "archive") str(R.string.text_archive_old_address_cf85f39) else str(R.string.text_propose_next_step_7edc384))
                    }
                }
            }
        }
    }
    if (confirm) AlertDialog(onDismissRequest = { confirm = false }, title = { Text(str(R.string.text_continue_the_reviewed_move_428f83b)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(walletMoveStep(value?.next.orEmpty()), style = MaterialTheme.typography.titleSmall)
            SelectionContainer {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(value?.source.orEmpty(), style = MaterialTheme.typography.bodySmall.code())
                    Text(value?.destination.orEmpty(), style = MaterialTheme.typography.bodySmall.code())
                }
            }
            Text(str(R.string.text_each_transfer_still_needs_its_own_approvals_and_execution_70c66b9))
        }
    }, confirmButton = { TextButton(onClick = { confirm = false; client.wallets.moveNext(wallet, from, l2) }) { Text(str(R.string.text_continue_fd19155)) } }, dismissButton = { TextButton(onClick = { confirm = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

private fun walletMoveStep(step: String) = when (step) {
    "root" -> str(R.string.text_create_device_key_root_5699bc2); "register" -> str(R.string.text_register_this_device_s_key_1265cbd); "address" -> str(R.string.text_create_a_new_address_7cdbf66)
    "book" -> str(R.string.text_add_to_address_book_9064696); "cooldown" -> str(R.string.text_wait_for_address_book_cooldown_2ccc5de); "wait" -> str(R.string.text_wait_for_proposal_782426e)
    "sweep" -> str(R.string.text_sweep_old_address_80041cc); "archive" -> str(R.string.text_archive_old_address_cf85f39); "done" -> str(R.string.text_moved_db6e264); else -> step
}
