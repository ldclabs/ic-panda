package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.CipherPhase
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun TransferScreen(client: ClientController) {
    val tasks by client.transfers.tasks.collectAsStateWithLifecycle()
    val durable by client.fileWork.jobs.state.collectAsStateWithLifecycle()
    val failures by client.fileWork.passFailures.collectAsStateWithLifecycle()
    val attachments by client.messageAttachments.states.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val background by client.backgroundFiles.current.state.collectAsStateWithLifecycle()
    LaunchedEffect(state.epoch) { client.fileWork.load(); client.backgroundFiles.load() }
    val transferring = background.valueOrNull()?.phase == CipherPhase.TRANSFERRING
    LaunchedEffect(transferring) {
        if (transferring) while (true) { kotlinx.coroutines.delay(1000); client.backgroundFiles.load() }
    }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            BackgroundTransferPanel(client)
            Notice(str(R.string.text_the_foreground_queue_runs_while_dmsg_is_open_and_unlocked_explici_a1289c4))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = { client.fileWork.resume() }) { Text(str(R.string.text_resume_backups_and_restores_69e121e)) }
                TextButton(onClick = client.fileWork::pause) { Text(str(R.string.text_pause_backup_and_restore_queue_459a1c8)) }
            }
            if (durable.valueOrNull() == null || durable is ReadState.Stale || durable is ReadState.Failed) ReadFeedback(durable, { client.fileWork.load() })
            failures.forEach { (kind, failure) ->
                Notice((if (kind == "upload") str(R.string.text_backup_paused_e3353a0) else str(R.string.text_restore_paused_2b71ae2)) + errorText(failure.code), true)
                Text(str(R.string.text_automatic_retry_uses_a_delay_retry_now_when_the_service_is_availa_0d6f2b7), style = MaterialTheme.typography.bodySmall)
            }
        }
        items(attachments.filterValues { it.saving || it.error != null }.entries.toList(), key = { "attachment:${it.key}" }) { (key, task) ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(task.name.ifBlank { str(R.string.text_message_attachment_5c83cad) }, style = MaterialTheme.typography.titleMedium)
                if (task.saving) {
                    Text(if (task.waiting) str(R.string.text_waiting_for_an_attachment_slot_1882c88) else str(R.string.text_downloading_verifying_and_saving_c1571eb))
                    if (task.total > 0u) LinearProgressIndicator(progress = { (task.completed.toDouble() / task.total.toDouble()).toFloat().coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth()) else LinearProgressIndicator(Modifier.fillMaxWidth())
                    TextButton(onClick = { client.messageAttachments.stop(key) }) { Text(str(R.string.text_cancel_download_1a40278)) }
                }
                task.error?.let { Notice(errorText(it), true) }
                TextButton(onClick = { client.messageAttachments.show(key) }) { Text(str(R.string.text_open_conversation_e4fc2cf)) }
                HorizontalDivider()
            }
        }
        items(durable.valueOrNull().orEmpty(), key = { it.kind + ":" + it.id }) { job ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(job.name ?: str(R.string.text_encrypted_file_2ff58b2), style = MaterialTheme.typography.titleMedium)
                Text((if (job.kind == "upload") str(R.string.text_backup_2a0ff4c) else str(R.string.text_restore_c7c5c0e)) + transferStage(job.stage))
                if (job.stage != "done") {
                    if (job.totalBytes > 0u) LinearProgressIndicator(progress = { (job.bytes.toDouble() / job.totalBytes.toDouble()).toFloat().coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                    else LinearProgressIndicator(Modifier.fillMaxWidth())
                }
                Text(fileSize(job.bytes) + " / " + fileSize(job.totalBytes), style = MaterialTheme.typography.labelSmall)
                job.error?.let { Notice(errorText(it), true) }
                job.retryAt?.let { Text(str(R.string.text_retry_after_ed1af9a) + messageTime(it), style = MaterialTheme.typography.labelSmall) }
                if (job.kind == "download" && job.stage != "done") TextButton(onClick = { client.fileWork.cancelDownload(job.id) }, enabled = !state.working("files:cancel:${job.id}")) { Text(str(R.string.text_cancel_this_restore_8957d7e)) }
                HorizontalDivider()
            }
        }
        if (tasks.isNotEmpty()) item { SectionTitle(str(R.string.text_local_operations_in_this_session_5b92d3b)) }
        items(tasks.values.toList().reversed(), key = { it.id }) { task ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(task.name.ifEmpty { str(R.string.text_importing_a_file_51e2c5d) }, style = MaterialTheme.typography.titleMedium)
                Text(when (task.phase) { "working" -> str(R.string.text_working_ef6a315); "done" -> str(R.string.text_operation_finished_check_copy_status_4c225e7); "failed" -> str(R.string.text_needs_attention_f50f34d); else -> str(R.string.text_interrupted_2974557) })
                if (task.phase == "working") {
                    if (task.total > 0u) { LinearProgressIndicator(progress = { (task.completed.toDouble() / task.total.toDouble()).toFloat().coerceIn(0f, 1f) }, Modifier.fillMaxWidth()); Text(fileSize(task.completed) + " / " + fileSize(task.total), style = MaterialTheme.typography.labelSmall) }
                    else LinearProgressIndicator(Modifier.fillMaxWidth())
                    TextButton(onClick = { client.transfers.cancel(task.id) }) { Text(str(R.string.text_stop_waiting_be4e25a)) }
                }
                task.error?.let { Notice(errorText(it), true) }
                HorizontalDivider()
            }
        }
        if (tasks.isEmpty() && attachments.values.none { it.saving || it.error != null } && durable is ReadState.Empty && background is ReadState.Empty) item { EmptyPanel(str(R.string.text_no_pending_transfers_e636c2e)) }
    }
}

internal fun transferStage(stage: String) = when (stage) {
    "queued" -> str(R.string.text_queued_2afa45a)
    "uploading" -> str(R.string.text_uploading_encrypted_chunks_8ee869d)
    "downloading", "fetching" -> str(R.string.text_downloading_encrypted_chunks_1284c0a)
    "committing" -> str(R.string.text_confirming_cloud_storage_a409f32)
    "verifying" -> str(R.string.text_verifying_all_contents_32fa18c)
    "done" -> str(R.string.text_verified_and_complete_494fee7)
    "failed" -> str(R.string.text_needs_attention_f50f34d)
    else -> str(R.string.text_in_progress_b2d2451) + stage
}
