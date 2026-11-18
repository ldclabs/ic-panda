package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun DeviceTransferScreen(client: ClientController, succession: String? = null, sending: Boolean? = null) {
    val state by client.state.collectAsStateWithLifecycle()
    val status by client.deviceTransfers.status.state.collectAsStateWithLifecycle()
    val draft by client.deviceTransfers.draft.collectAsStateWithLifecycle()
    val pending by client.deviceTransfers.pending.collectAsStateWithLifecycle()
    val notice by client.deviceTransfers.notice.collectAsStateWithLifecycle()
    val suspended by client.deviceTransfers.suspended.collectAsStateWithLifecycle()
    val value = status.valueOrNull()
    val busy = state.working("device-transfer:start") || state.working("device-transfer:approval")
    val running = value?.running == true
    val focus = LocalFocusManager.current
    var sendMode by rememberSaveable(succession, sending) { mutableStateOf(sending ?: false) }
    var manualAddress by rememberSaveable(succession) { mutableStateOf(draft.address.isNotBlank()) }
    val discoveryFailed = notice in setOf("DISCOVERY_UNAVAILABLE", "TIMEOUT")
    LaunchedEffect(discoveryFailed) { if (discoveryFailed) manualAddress = true }
    val view = LocalView.current
    DisposableEffect(view, running) { view.keepScreenOn = running; onDispose { view.keepScreenOn = false } }
    LaunchedEffect(state.epoch) { client.deviceTransfers.load(); client.accountWorkspace.local() }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Notice(str(R.string.text_keep_both_devices_open_on_the_same_network_discovery_finds_a_rece_75399e1))
            Text(str(R.string.text_only_the_file_library_moves_here_device_identities_private_signin_033e3f0), style = MaterialTheme.typography.bodySmall)
            if (sending == null && !running && pending == null) FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(!sendMode, { focus.clearFocus(); sendMode = false }, { Text(str(R.string.review_receiving)) })
                FilterChip(sendMode, { focus.clearFocus(); sendMode = true }, { Text(str(R.string.text_send_06acd73)) })
            }
            ReadFeedback(status, client.deviceTransfers::load)
            notice?.let { Notice(if (it == "DISCOVERY_UNAVAILABLE" || it == "TIMEOUT") str(R.string.text_discovery_could_not_find_the_other_device_enter_its_displayed_add_0504a42) else errorText(it), true) }
            if (suspended) Notice(str(R.string.text_the_transfer_stopped_while_the_app_was_away_receiving_can_resume_0997670))
            value?.let { transfer ->
                Text(when (transfer.state) { "waiting" -> str(R.string.text_waiting_for_the_other_device_dc2dcde); "running" -> str(R.string.text_transferring_b3d0623); "done" -> str(R.string.text_transfer_verified_and_complete_ee91d34); "failed" -> str(R.string.text_transfer_interrupted_58769ff); else -> str(R.string.text_transfer_stopped_1a0018f) }, style = MaterialTheme.typography.titleLarge)
                if (transfer.role == "receive") {
                    TransferIdentity(str(R.string.text_session_code_78cc2a3), transfer.session)
                    TextButton(onClick = { client.clipboard.copy(transfer.session) }) { Text(str(R.string.text_copy_session_code_9c60941)) }
                    TransferIdentity(str(R.string.text_receiving_device_id_2b4bb35), transfer.to)
                    if (transfer.running) transfer.addresses.forEach { address -> TransferIdentity(str(R.string.text_address_d082ce5), address) }
                } else TransferIdentity(str(R.string.text_receiving_device_id_2b4bb35), transfer.to)
                if (transfer.state == "running") {
                    LinearProgressIndicator(progress = { if (transfer.report.total == 0uL) 0f else (transfer.report.bytes.toDouble() / transfer.report.total.toDouble()).toFloat().coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                    Text(fileSize(transfer.report.bytes) + " / " + fileSize(transfer.report.total))
                }
                if (!transfer.running && transfer.report.peer != null) {
                    LabelValue(str(R.string.text_records_added_3967796), transfer.report.records.toString())
                    LabelValue(str(R.string.text_already_here_5ec81c5), transfer.report.kept.toString())
                    LabelValue(str(R.string.text_left_out_923d274), transfer.report.skipped.toString())
                }
                transfer.error?.let { Notice(errorText(it), true) }
                if (transfer.state in setOf("failed", "cancelled") && !busy) OutlinedButton(onClick = client.deviceTransfers::resume) { Text(str(R.string.text_resume_transfer_30cedd1)) }
            }
            pending?.let { request ->
                Notice(str(R.string.text_the_export_is_waiting_for_your_approval_after_approval_return_her_317b84d))
                AccountRow(str(R.string.text_review_export_approval_b30cfe8)) { client.deviceTransfers.openApproval() }
                OutlinedButton(onClick = client.deviceTransfers::sendApproved, enabled = !busy) { Text(str(R.string.text_send_approved_library_d949500)) }
                TransferIdentity(str(R.string.text_approval_id_46a3c7c), request.approval.id)
            }
            if (!running && pending == null) {
                if (!sendMode) Button(onClick = { focus.clearFocus(); client.deviceTransfers.receive(succession) }, enabled = !busy) { Text(str(R.string.text_receive_here_3ea6c72)) }
                if (sendMode) {
                    OutlinedTextField(draft.session, { client.deviceTransfers.edit(draft.copy(session = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_session_code_78cc2a3)) }, maxLines = 3)
                    if (succession == null) OutlinedTextField(draft.device, { client.deviceTransfers.edit(draft.copy(device = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_receiving_device_id_2b4bb35)) }, maxLines = 3)
                    TextButton(onClick = {
                        manualAddress = !manualAddress
                        if (!manualAddress) client.deviceTransfers.edit(draft.copy(address = ""))
                    }) { Text(str(R.string.text_address_optional_75b3cae)) }
                    if (manualAddress) OutlinedTextField(draft.address, { client.deviceTransfers.edit(draft.copy(address = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_address_optional_75b3cae)) }, supportingText = { Text(str(R.string.text_use_the_receiver_s_address_if_automatic_discovery_is_unavailable_29566bb)) })
                }
            }
            val local by client.accountWorkspace.local.state.collectAsStateWithLifecycle()
            ReadFeedback(local, client.accountWorkspace::local)
            local.valueOrNull()?.let { LocalContentPanel(it) }
        }
        Surface {
            if (running || busy || pending != null) OutlinedButton(onClick = client.deviceTransfers::cancel, modifier = Modifier.fillMaxWidth().padding(20.dp)) { Text(str(R.string.text_stop_transfer_4135785)) }
            else if (sendMode) Button(onClick = { focus.clearFocus(); client.deviceTransfers.send(succession) }, enabled = draft.valid(succession), modifier = Modifier.fillMaxWidth().padding(20.dp)) { Text(str(R.string.text_review_sending_a1b3596)) }
        }
    }
}
@Composable internal fun TransferIdentity(label: String, value: String) {
    Text(label, style = MaterialTheme.typography.labelMedium)
    SelectionContainer { Text(value, style = MaterialTheme.typography.bodySmall) }
}
