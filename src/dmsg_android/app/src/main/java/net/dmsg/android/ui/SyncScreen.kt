package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun SyncScreen(client: ClientController) {
    val local by client.accountWorkspace.local.state.collectAsStateWithLifecycle()
    val status by client.synchronization.status.collectAsStateWithLifecycle()
    val connected by client.synchronization.connected.collectAsStateWithLifecycle()
    val constrained by client.synchronization.constrained.collectAsStateWithLifecycle()
    val session by client.state.collectAsStateWithLifecycle()
    var automatic by remember { mutableStateOf(client.synchronization.automatic) }
    LaunchedEffect(session.epoch, status?.lastSuccessAtMs) { client.accountWorkspace.local() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        val current = status
        val syncError = current?.error
        when {
            syncError != null -> Notice(errorText(syncError), true)
            constrained -> Notice(str(R.string.text_automatic_work_is_paused_while_android_is_saving_power_or_the_dev_81dedff))
        }
        Section {
            SettingToggle(str(R.string.text_automatic_synchronization_dde4f07), automatic,
                { automatic = it; client.automaticSync(it) }, description = str(R.string.text_while_dmsg_is_open_and_unlocked_99d4594), icon = Icons.Outlined.Sync)
        }
        DetailCard {
            if (current == null) Text(str(R.string.text_synchronization_status_has_not_been_read_c16ea3e), style = MaterialTheme.typography.bodyMedium)
            else {
                Text(
                    if (!current.initialized) str(R.string.text_verifying_your_first_snapshot_0bfeb80)
                    else if (current.running) str(R.string.text_synchronizing_21e4747)
                    else str(R.string.text_account_synchronization_77ccb72),
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.openedReminderVisible(client, true) {
                        it.kind == "task" && it.id?.startsWith("sync_failed:") == true || it.kind == "service" && it.id?.startsWith("network:") == true
                    },
                )
                if (current.running) {
                    if (current.total > 0u) LinearProgressIndicator(progress = { (current.entries.toDouble() / current.total.toDouble()).coerceIn(0.0, 1.0).toFloat() }, modifier = Modifier.fillMaxWidth())
                    else LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(localizedNumber(current.entries) + " / " + localizedNumber(current.total), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                LabelValue(str(R.string.text_pending_upload_3dce896).trimEnd(' ', '·', ':'),
                    session.session?.pendingSync?.let(::localizedNumber) ?: str(R.string.text_unknown_d748bba))
                LabelValue(str(R.string.text_last_successful_sync_d3803f5).trimEnd(' ', '·', ':'),
                    current.lastSuccessAtMs?.let(::displayTime) ?: str(R.string.text_not_yet_confirmed_b746f78))
                current.retryAtMs?.let { LabelValue(str(R.string.text_next_retry_536436a).trimEnd(' ', '·', ':'), displayTime(it)) }
            }
            Text(
                if (connected) str(R.string.text_live_updates_connected_1679882)
                else str(R.string.text_live_updates_disconnected_opening_dmsg_checks_for_changes_a261ad7),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Button(onClick = client::sync, enabled = !session.working("sync:manual"), modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Outlined.Sync, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_sync_now_bf05b83))
            }
        }
        ReadFeedback(local, client.accountWorkspace::local)
        local.valueOrNull()?.let { LocalContentPanel(it) { client.navigation.push(Route("file-unbacked")) } }
        Section(footer = str(R.string.text_account_synchronization_does_not_automatically_back_up_local_file_422bc19)) {
            ListRow(str(R.string.text_cloud_backups_ab4daf1), icon = Icons.Outlined.CloudDone) { client.navigation.push(Route("cloud-files")) }
            ListRow(str(R.string.text_version_conflicts_0df1b3c), icon = Icons.Outlined.WarningAmber) { client.navigation.push(Route("conflicts")) }
        }
    }
}
