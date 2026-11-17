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

@Composable fun NoticeDestinationDialog(client: ClientController) {
    val destination by client.noticeNavigation.destination.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val names by client.accountNames.collectAsStateWithLifecycle()
    val value = destination.valueOrNull()
    if (destination is ReadState.Idle || state.concealed || state.starting) return
    val reason = (destination as? ReadState.Failed)?.code
    val ended = value?.ended == true || reason in setOf("NOT_FOUND", "FORBIDDEN", "AUTH_REQUIRED", "DEVICE_NOT_APPROVED", "INVALID_INPUT")
    val other = value != null && value.account != state.session?.account
    val title = when { ended -> str(R.string.text_reminder_unavailable_81a7dfd); reason != null -> str(R.string.text_unable_to_open_reminder_db3b926); other -> str(R.string.text_this_reminder_belongs_to_another_account_fc6ab8f); value != null && !state.unlocked -> str(R.string.text_unlock_to_view_reminder_2c82137); else -> str(R.string.text_opening_reminder_f921daf) }
    AlertDialog(onDismissRequest = client.noticeNavigation::dismiss, title = { Text(title) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            when {
                ended -> Text(str(R.string.text_the_original_item_can_no_longer_be_opened_no_action_has_been_take_e5dcc22))
                reason != null -> Text(errorText(reason))
                other -> {
                    Text(str(R.string.text_choose_its_account_to_continue_your_current_account_stays_selecte_7c080c4))
                    Text(names[value!!.account] ?: value.account)
                    if (names[value.account] != null) Text(value.account, style = MaterialTheme.typography.bodySmall)
                }
                value != null && !state.unlocked -> Text(str(R.string.text_this_reminder_is_protected_unlock_this_workspace_to_continue_214b997))
                else -> LinearProgressIndicator(Modifier.fillMaxWidth())
            }
        }
    }, confirmButton = {
        when {
            ended -> TextButton(onClick = client.noticeNavigation::dismiss) { Text(str(R.string.text_close_6cf4a77)) }
            reason != null -> TextButton(onClick = client.noticeNavigation::retry) { Text(str(R.string.text_retry_563171c)) }
            other -> TextButton(onClick = client.noticeNavigation::switchAccount, enabled = !state.busy) { Text(str(R.string.text_switch_account_52f71df)) }
            value != null && !state.unlocked -> TextButton(onClick = {
                if (state.session?.fastUnlock == true) client.unlock()
                else { client.noticeNavigation.clear(); client.showAccess(AccessPage.WELCOME) }
            }, enabled = !state.busy) { Text(str(R.string.text_unlock_workspace_ec9cb2a)) }
        }
    }, dismissButton = { if (!ended) TextButton(onClick = client.noticeNavigation::dismiss) { Text(str(R.string.text_cancel_998b9c4)) } })
}
