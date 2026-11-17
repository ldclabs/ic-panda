package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun BackgroundTransferPanel(client: ClientController) {
    val current by client.backgroundFiles.current.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val job = current.valueOrNull()
    var confirming by remember { mutableStateOf(false) }
    var now by remember { mutableStateOf(System.currentTimeMillis().toULong()) }
    LaunchedEffect(Unit) { while (true) { now = System.currentTimeMillis().toULong(); kotlinx.coroutines.delay(1000) } }
    val busy = state.working("cipher:prepare") || state.working("cipher:finish") || state.working("cipher:cancel")
    if (current is ReadState.Failed || current is ReadState.Stale) ReadFeedback(current, client.backgroundFiles::load)
    if (job == null) return
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(str(R.string.text_background_encrypted_transfer_d77c9cc), style = MaterialTheme.typography.titleMedium)
            if (job.plan.account != state.session?.account) {
                Notice(str(R.string.text_this_device_has_a_transfer_for_another_account_switch_to_that_acc_061f573))
                AccountRow(str(R.string.text_switch_account_52f71df)) { client.navigation.push(Route("accounts")) }
            } else {
                Text(if (job.plan.method == "PUT") str(R.string.text_encrypted_backup_upload_5b29d9b) else str(R.string.text_encrypted_backup_download_9382fee))
                LinearProgressIndicator(progress = { if (job.plan.parts.isEmpty()) 1f else job.completed.size.toFloat() / job.plan.parts.size }, modifier = Modifier.fillMaxWidth())
                Text(fileSize(job.bytes.toULong()) + " / " + fileSize(job.plan.size.toULong()))
                Text(when(job.phase) {
                    "verifying" -> str(R.string.text_bytes_transferred_unlock_this_account_to_verify_and_commit_891a98d)
                    "paused" -> str(R.string.text_paused_cf54b60); "failed" -> str(R.string.text_needs_attention_f50f34d); else -> str(R.string.text_waiting_for_system_or_transferring_4fb8f12)
                })
                job.error?.let { if (it != "PREPARING_CIPHERTEXT") Notice(cipherError(it), true) }
                LabelValue(str(R.string.text_authorization_expires_b43a709), displayTime(job.plan.expires))
                if (job.ready) Button(onClick = client.backgroundFiles::finish, enabled = !busy) { Text(str(R.string.text_verify_and_finish_82a0fce)) }
                else if (job.plan.expires <= now) Notice(str(R.string.text_the_transfer_authorization_expired_cancel_this_job_then_continue_8fb56dd))
                else if (job.phase != "transferring") OutlinedButton(onClick = client.backgroundFiles::retry, enabled = !busy) { Text(str(R.string.text_retry_original_encrypted_blocks_8091317)) }
                else OutlinedButton(onClick = client.backgroundFiles::pause, enabled = !busy) { Text(str(R.string.text_pause_background_transfer_1a5d000)) }
                TextButton(onClick = { confirming = true }, enabled = !busy) { Text(str(R.string.text_cancel_background_transfer_606bbfe), color = MaterialTheme.colorScheme.error) }
            }
        }
    }
    if (confirming) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_cancel_this_background_transfer_98b5509)) }, text = {
        Text(str(R.string.text_stops_this_device_s_system_task_and_revokes_its_scoped_transfer_a_4d47f3a))
    }, confirmButton = { TextButton(onClick = { confirming = false; client.backgroundFiles.cancel() }) { Text(str(R.string.text_cancel_transfer_23833f9)) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_keep_transfer_6c3b2ef)) } })
}
private fun cipherError(code: String) = when(code) {
    "TRANSFER_EXPIRED" -> str(R.string.text_authorization_has_expired_416c560)
    "TRANSFER_STOPPED" -> str(R.string.text_the_system_stopped_this_task_resume_it_explicitly_when_ready_c7c6d4f)
    "SYSTEM_TRANSFER_UNAVAILABLE" -> str(R.string.text_the_system_could_not_schedule_this_transfer_retry_while_dmsg_is_o_d9e1588)
    "CIPHERTEXT_UNAVAILABLE", "MISSING_CIPHERTEXT" -> str(R.string.text_a_stored_encrypted_block_is_missing_or_invalid_retry_from_the_ori_ecabad7)
    else -> errorText(code)
}
