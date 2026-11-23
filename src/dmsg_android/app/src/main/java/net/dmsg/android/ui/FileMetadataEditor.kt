package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun FileMetadataEditor(client: ClientController, current: LibraryFile, close: () -> Unit) {
    val state by client.state.collectAsStateWithLifecycle()
    val draft by client.library.editor.collectAsStateWithLifecycle()
    val failure by client.library.editorError.collectAsStateWithLifecycle()
    val value = draft?.takeIf { it.file.id == current.id } ?: return
    val working = state.working("files:metadata:${current.id}")
    AlertDialog(onDismissRequest = close, title = { Text(str(R.string.text_file_notes_and_tags_d701552)) }, text = {
        Column(Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(current.name, style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(value.tags, { client.library.editMetadata(value.copy(tags = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_tags_separated_by_commas_10131bb)) }, enabled = !working)
            OutlinedTextField(value.note, { client.library.editMetadata(value.copy(note = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_note_a907cbf)) }, minLines = 3, maxLines = 8, enabled = !working)
            Section {
                SettingToggle(
                    title = str(R.string.text_favorite_898f4c6),
                    checked = value.starred,
                    change = { client.library.editMetadata(value.copy(starred = it)) },
                    modifier = Modifier.semantics { contentDescription = str(R.string.text_favorite_898f4c6) },
                    enabled = !working,
                )
            }
            Text(str(R.string.text_unsaved_changes_stay_in_this_unlocked_session_locking_clears_this_4b22421),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            failure?.let { Notice(errorText(it), true) }
            if (failure == "VERSION_CONFLICT") {
                DetailCard {
                    Text(str(R.string.text_current_stored_metadata_cf91691), style = MaterialTheme.typography.titleSmall)
                    if (current.tags.isNotEmpty()) Text(current.tags.joinToString(", "), style = MaterialTheme.typography.bodySmall)
                    if (current.note.isNotEmpty()) Text(current.note, style = MaterialTheme.typography.bodySmall)
                    Text(if (current.starred) str(R.string.text_currently_a_favorite_d790164) else str(R.string.text_currently_not_a_favorite_24989c8),
                        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton(onClick = { client.library.reviewNewMetadata(current) }) { Text(str(R.string.text_use_my_edits_after_reviewing_this_update_6b8c851)) }
                }
            }
            TextButton(onClick = { client.library.discardMetadata(); close() }, enabled = !working) {
                Text(str(R.string.text_discard_form_changes_04819d8), color = MaterialTheme.colorScheme.error)
            }
        }
    }, confirmButton = { TextButton(onClick = { client.library.saveMetadata(close) }, enabled = !working && failure != "VERSION_CONFLICT") { Text(str(R.string.text_save_ec8e6d5)) } },
        dismissButton = { TextButton(onClick = close) { Text(str(R.string.text_keep_draft_89639c8)) } })
}
