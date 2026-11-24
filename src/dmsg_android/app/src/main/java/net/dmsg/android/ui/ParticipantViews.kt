package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.People
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun ParticipantIdentity(client: ClientController, account: String, showId: Boolean = false) {
    val people by client.participants.people.collectAsStateWithLifecycle()
    val person = people[account]
    var inspecting by remember(account) { mutableStateOf(false) }
    DisposableEffect(client, account) { client.participants.observe(account); onDispose { client.participants.release(account) } }
    Row(Modifier.clickable { inspecting = true }.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        val bitmap = person?.avatar
        val image = remember(bitmap) { bitmap?.asImageBitmap() }
        Avatar(person?.name ?: account, account, if (image != null) Modifier.testTag("participant-avatar") else Modifier, size = 32.dp, image = image)
        Column {
            val hasName = person?.name != null
            Text(person?.name ?: account, style = if (hasName) MaterialTheme.typography.titleSmall else MaterialTheme.typography.bodyMedium.code(),
                maxLines = if (showId) 2 else 1, overflow = TextOverflow.Ellipsis)
            if (showId && hasName) Text(account, style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (person?.stale == true) Text(str(R.string.text_last_verified_name_921b2a4), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
    if (inspecting) AlertDialog(onDismissRequest = { inspecting = false }, title = { Text(person?.name ?: str(R.string.text_account_identity_0d43c22)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            SelectionContainer { Text(account, style = MaterialTheme.typography.bodyMedium.code()) }
            Text(str(R.string.text_a_name_or_avatar_is_not_a_unique_identity_compare_the_complete_ac_9255f9b), style = MaterialTheme.typography.bodyMedium)
            person?.takeIf { it.observedAt > 0u }?.let { Text(str(R.string.text_last_verified_92b1841) + displayTime(it.observedAt), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }, confirmButton = { CopyButton(str(R.string.text_copy_account_id_7bcff24), icon = false, key = account) { client.clipboard.copy(account) } }, dismissButton = { TextButton(onClick = { inspecting = false }) { Text(str(R.string.text_close_6cf4a77)) } })
}

@Composable fun RecipientField(client: ClientController, account: String, change: (String) -> Unit, enabled: Boolean = true, label: String = str(R.string.text_account_id_e4ff381)) {
    var choosing by remember { mutableStateOf(false) }
    val known by client.participants.accounts.state.collectAsStateWithLifecycle()
    OutlinedTextField(account, change, Modifier.fillMaxWidth().revealFocusedInput(), enabled = enabled, singleLine = true, label = { Text(label) },
        trailingIcon = { IconButton(onClick = { client.participants.browse(); choosing = true }, enabled = enabled) { Icon(Icons.Outlined.People, str(R.string.text_choose_a_known_account_1bb184d)) } })
    if (validAccount(account)) ParticipantIdentity(client, account, showId = true)
    if (choosing) AlertDialog(onDismissRequest = { choosing = false }, title = { Text(str(R.string.text_known_accounts_f4d4ed2)) }, text = {
        Column {
            Text(str(R.string.text_from_your_conversations_and_letters_on_this_device_211a5ed), style = MaterialTheme.typography.bodySmall)
            ReadFeedback(known, client.participants::browse)
            LazyColumn(Modifier.heightIn(max = 420.dp)) { items(known.valueOrNull().orEmpty(), key = { it }) { id ->
                Row(verticalAlignment = Alignment.CenterVertically) { Box(Modifier.weight(1f)) { ParticipantIdentity(client, id, true) }; TextButton(onClick = { change(id); choosing = false }) { Text(str(R.string.text_choose_9c29923)) } }
            } }
            if (known is ReadState.Empty) Text(str(R.string.text_no_known_accounts_yet_paste_the_full_id_instead_454f324))
        }
    }, confirmButton = { TextButton(onClick = { choosing = false }) { Text(str(R.string.text_close_6cf4a77)) } })
}
