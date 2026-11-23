package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.Image
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import net.dmsg.android.platform.IncomingShare

@Composable fun IncomingSharesScreen(client: ClientController) {
    val entries by client.incoming.entries.state.collectAsStateWithLifecycle()
    val intake by client.incoming.intake.collectAsStateWithLifecycle()
    val previews by client.incoming.previews.collectAsStateWithLifecycle()
    val images by client.incoming.images.collectAsStateWithLifecycle()
    val tree by client.library.tree.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var destination by remember { mutableStateOf(emptyList<LibraryFolder>()) }
    val folder = destination.folderId
    fun displayFolder(id: String?): String = when (id) {
        null, folder -> destination.folderLabel
        "root" -> str(R.string.text_files_c79ddc2)
        else -> tree.valueOrNull()?.find { it.id == id }?.let { (it.path + it.name).joinToString(" / ") } ?: str(R.string.file_original_folder)
    }
    LaunchedEffect(Unit) { if (client.library.tree.value() == null) client.library.loadTree() }
    var choosing by remember { mutableStateOf(false) }
    var discard by remember { mutableStateOf<IncomingShare?>(null) }
    var importing by remember { mutableStateOf<IncomingShare?>(null) }
    LaunchedEffect(state.epoch) { client.incoming.load(); client.incoming.acknowledge() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            Notice(str(R.string.text_shared_items_wait_in_an_independently_encrypted_copy_on_this_devi_ac0b59d))
            state.session?.account?.let { TransferIdentity(str(R.string.text_import_account_2335d8d), it) }
            AccountRow(str(R.string.text_choose_another_account_bd5c007)) { client.navigation.push(Route("accounts")) }
            TextButton(onClick = { choosing = true }) { Text(str(R.string.text_destination_646099e) + destination.folderLabel) }
            if (intake.working) { LinearProgressIndicator(Modifier.fillMaxWidth()); Text(str(R.string.text_encrypting_the_shared_items_dcc2984)); TextButton(onClick = client.incoming::stop) { Text(str(R.string.text_stop_receiving_d2c4d33)) } }
            intake.error?.let { Notice(errorText(it), true) }
            if (intake.duplicate) Notice(str(R.string.text_this_shared_copy_was_already_received_it_will_not_be_imported_twi_64ed5e3))
            ReadFeedback(entries, client.incoming::load)
            if (entries is ReadState.Empty && !intake.working) EmptyPanel(str(R.string.text_no_shared_items_waiting_0f756a2))
        }
        entries.valueOrNull().orEmpty().forEach { entry ->
            item(key = entry.id) {
                Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(entry.name, style = MaterialTheme.typography.titleMedium)
                    Text(fileSize(entry.size.toULong()), style = MaterialTheme.typography.labelMedium)
                    val busy = state.working("incoming:import:" + entry.id) || state.working("incoming:discard:" + entry.id)
                    if (state.working("incoming:import:" + entry.id)) {
                        LinearProgressIndicator(Modifier.fillMaxWidth())
                        Text(str(R.string.text_verifying_and_importing_61b4964))
                        TextButton(onClick = { client.transfers.cancel("incoming:import:" + entry.id); client.incoming.load() }) { Text(str(R.string.text_stop_importing_f7fe029)) }
                    }
                    if (!entry.complete) Notice(str(R.string.text_the_source_copy_was_interrupted_discard_it_and_share_the_source_a_12f4b77))
                    else if (entry.account == null || entry.account == state.session?.account) {
                        if (entry.mime.startsWith("image/") && entry.size <= 8 * 1024 * 1024) TextButton(onClick = { client.incoming.previewImage(entry) }) { Text(str(R.string.text_preview_shared_image_c1b8c69)) }
                        images[entry.id]?.let { Image(it.asImageBitmap(), str(R.string.text_shared_image_preview_a2021ea), Modifier.fillMaxWidth().heightIn(max = 220.dp), contentScale = ContentScale.Fit) }
                        if (entry.mime.startsWith("text/")) TextButton(onClick = { client.incoming.preview(entry) }) { Text(str(R.string.text_preview_shared_text_e851a01)) }
                        previews[entry.id]?.let { preview ->
                            Text(str(R.string.text_preview_up_to_128_kib_87caec2), style = MaterialTheme.typography.labelSmall)
                            SelectionContainer { Text(preview, modifier = Modifier.heightIn(max = 240.dp).verticalScroll(rememberScrollState())) }
                        }
                        if (entry.account != null) Text(str(R.string.text_continue_the_original_import_destination_d8d7ad5) + displayFolder(entry.folder))
                        Button(onClick = { importing = entry }, enabled = !busy) { Text(str(R.string.text_import_into_this_account_098cc0d)) }
                    } else {
                        Notice(str(R.string.text_continue_this_import_in_the_account_originally_selected_92d1041))
                        TextButton(onClick = { client.navigation.push(Route("accounts")) }) { Text(str(R.string.text_switch_account_52f71df)) }
                    }
                    TextButton(onClick = { discard = entry }, enabled = !busy) { Text(str(R.string.text_discard_staged_copy_5a040f7), color = MaterialTheme.colorScheme.error) }
                } }
            }
        }
    }
    importing?.let { item -> AlertDialog(onDismissRequest = { importing = null }, title = { Text(str(R.string.text_import_this_shared_copy_38939c9)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(item.name)
            TransferIdentity(str(R.string.text_account_4ab56e9), state.session?.account.orEmpty())
            LabelValue(str(R.string.text_destination_63c1289), displayFolder(item.folder))
            Text(str(R.string.text_the_file_is_encrypted_into_this_account_s_local_library_this_does_e6007cb))
        }
    }, confirmButton = { TextButton(onClick = { importing = null; client.incoming.import(item, folder) }) { Text(str(R.string.text_import_1a200b3)) } }, dismissButton = { TextButton(onClick = { importing = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
    discard?.let { item -> AlertDialog(onDismissRequest = { discard = null }, title = { Text(str(R.string.text_discard_this_staged_copy_db153d3)) }, text = { Text(str(R.string.text_only_the_pending_encrypted_copy_is_removed_its_source_and_any_fil_550e351)) }, confirmButton = {
        TextButton(onClick = { discard = null; client.incoming.discard(item) }) { Text(str(R.string.text_discard_475cd3e), color = MaterialTheme.colorScheme.error) }
    }, dismissButton = { TextButton(onClick = { discard = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
    if (choosing) FileFolderDialog(client, str(R.string.text_import_destination_4146867), { choosing = false }, initial = destination) {
        destination = it; choosing = false
    }
}
