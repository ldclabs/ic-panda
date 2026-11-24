package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import android.content.Intent
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun FilePreviewScreen(client: ClientController, id: String, version: String) {
    val state by client.state.collectAsStateWithLifecycle()
    val preview by client.filePreview.preview.state.collectAsStateWithLifecycle()
    val content = preview.valueOrNull()?.takeIf { it.file.id == id && it.file.version == version }
    val context = LocalContext.current
    var confirmation by remember(id, version) { mutableStateOf<String?>(null) }
    val pickers = LocalFilePickers.current
    LaunchedEffect(id, version, state.epoch) { client.filePreview.open(id, version) }
    DisposableEffect(id, version) { onDispose { client.filePreview.clear() } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(preview, { client.filePreview.open(id, version) })
        content?.let { value ->
            DetailCard {
                Text(value.file.name, style = MaterialTheme.typography.titleMedium)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    RowIcon(Icons.Outlined.VerifiedUser)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(str(R.string.text_complete_file_verified_63c564b), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
                        Text(fileSize(value.file.size) + " · " + value.file.mime, style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
            when (value.kind) {
                "text" -> {
                    if (value.truncated) Notice(str(R.string.text_showing_the_first_128_kib_the_complete_file_was_verified_export_t_fec615a))
                    DetailCard {
                        // Copy decrypted contents through the private clipboard action below.
                        Text(value.text.orEmpty(), modifier = Modifier.fillMaxWidth(), style = MaterialTheme.typography.bodyLarge)
                    }
                    CopyButton(str(R.string.text_copy_displayed_text_1254da5), outlined = true, enabled = !value.text.isNullOrEmpty()) { client.clipboard.copy(value.text.orEmpty()) }
                }
                "image" -> value.bitmap?.let { bitmap ->
                    DetailCard {
                        Image(bitmap.asImageBitmap(), contentDescription = value.file.name, modifier = Modifier.fillMaxWidth().heightIn(max = 640.dp), contentScale = ContentScale.Fit)
                    }
                }
                else -> Notice(str(R.string.text_use_a_compatible_app_to_view_this_file_only_a_verified_copy_will_e253728))
            }
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Button(onClick = { confirmation = "export" }, enabled = !state.working("file:export:$id"), modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.SaveAlt, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_save_a_copy_outside_dmsg_244f27e))
                }
                OutlinedButton(onClick = { confirmation = "open" }, enabled = !state.working("file:open:$id"), modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.AutoMirrored.Outlined.OpenInNew, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_open_in_another_app_d703f04))
                }
            }
        }
    }
    if (confirmation != null && content != null) AlertDialog(onDismissRequest = { confirmation = null }, title = { Text(str(R.string.text_share_a_verified_copy_e22c532)) },
        text = { Text(str(R.string.text_the_destination_or_app_you_choose_can_keep_this_copy_dmsg_cannot_dfc6feb)) }, confirmButton = {
            TextButton(onClick = {
                val action = confirmation; confirmation = null
                if (action == "export") {
                    if (client.transfers.beginExport(content.file)) pickers.export(content.file)
                } else client.transfers.share(content.file) { file, mime ->
                    client.beginExternalInteraction()
                    val uri = FileProvider.getUriForFile(context, context.packageName + ".files", file)
                    runCatching { context.startActivity(Intent.createChooser(Intent(Intent.ACTION_VIEW).setDataAndType(uri, mime).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION), null)) }.onFailure { client.showError("FILE_UNAVAILABLE") }
                }
            }) { Text(str(R.string.text_continue_fd19155)) }
        }, dismissButton = { TextButton(onClick = { confirmation = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
