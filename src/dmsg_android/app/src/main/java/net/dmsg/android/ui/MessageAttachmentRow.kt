package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import net.dmsg.core.MessageItem

@Composable fun MessageAttachmentRow(client: ClientController, channel: String, message: MessageItem) {
    val states by client.messageAttachments.states.collectAsStateWithLifecycle()
    val revision by client.messageAttachments.revision.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val current = states[client.messageAttachments.key(channel, message.sequence)]
    LaunchedEffect(channel, message.sequence, state.epoch, revision, message.attachmentDeleted) { client.messageAttachments.observe(channel, message) }
    DisposableEffect(channel, message.sequence) { onDispose { client.messageAttachments.release(channel, message.sequence) } }
    Column(Modifier.widthIn(max = 360.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (message.attachmentMime == "application/vnd.dmsg.secret-package") {
            TextButton(onClick = { client.navigation.push(Route("secret-import", channel = channel, sequence = message.sequence)) }, enabled = !message.attachmentDeleted) { Icon(Icons.Outlined.Shield, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_preview_secret_package_8cea531)) }
        } else {
            Text(message.attachmentName.orEmpty(), style = MaterialTheme.typography.titleSmall)
            message.attachmentSize?.let { Text(fileSize(it), style = MaterialTheme.typography.labelSmall) }
            when {
                message.attachmentDeleted -> Text(str(R.string.text_this_shared_attachment_was_removed_97f6cbf), style = MaterialTheme.typography.bodySmall)
                current?.saving == true -> {
                    if (current.total > 0u) LinearProgressIndicator(progress = { (current.completed.toDouble() / current.total.toDouble()).toFloat().coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth()) else LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(if (current.waiting) str(R.string.text_waiting_for_another_attachment_to_finish_2780f57) else str(R.string.text_downloading_verifying_and_saving_c1571eb), style = MaterialTheme.typography.bodySmall)
                    TextButton(onClick = { client.messageAttachments.cancel(channel, message) }) { Text(str(R.string.text_cancel_download_1a40278)) }
                }
                current?.file != null -> {
                    current.thumbnail?.let { Image(it.asImageBitmap(), message.attachmentName, Modifier.fillMaxWidth().heightIn(max = 180.dp), contentScale = ContentScale.Fit) }
                    Text(str(R.string.text_local_copy_saved_to_files_671755a), style = MaterialTheme.typography.labelSmall)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(onClick = { client.navigation.push(Route("file-preview", id = current.file.id, revision = current.file.version)) }) { Text(str(R.string.text_preview_68dd3b5)) }
                        TextButton(onClick = { client.navigation.push(Route("files", id = current.file.id, revision = current.file.version)) }) { Text(str(R.string.text_show_in_files_8b64e4b)) }
                    }
                }
                else -> {
                    current?.error?.let { Notice(errorText(it), true) }
                    TextButton(onClick = { client.messageAttachments.save(channel, message) }, enabled = current?.reading != true) { Icon(Icons.Outlined.Download, null); Spacer(Modifier.width(8.dp)); Text(if (current?.error != null) str(R.string.text_retry_original_attachment_752ccb1) else str(R.string.text_download_and_save_to_files_0a5a7b5)) }
                }
            }
        }
    }
}
