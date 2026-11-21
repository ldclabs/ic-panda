package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.serialization.json.Json
import net.dmsg.android.BuildConfig
import net.dmsg.android.core.*

@Composable fun BrowserConnectionsScreen(client: ClientController, browser: (String) -> Unit) {
    val invitations by client.browserConnections.invitations.state.collectAsStateWithLifecycle()
    val pairings by client.browserConnections.pairings.state.collectAsStateWithLifecycle()
    val candidates by client.browserConnections.candidates.collectAsStateWithLifecycle()
    val errors by client.browserConnections.errors.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var grant by remember { mutableStateOf<Pair<BrowserInvitation, BrowserCandidate>?>(null) }
    var revoke by remember { mutableStateOf<BrowserPairing?>(null) }
    var now by remember { mutableStateOf(System.currentTimeMillis().toULong()) }
    val origin = remember { if (BuildConfig.IS_STAGING) runCatching { Json.parseToJsonElement(BuildConfig.DEPLOYMENT).objectValue().required("connect_url") }.getOrDefault("") else "https://connect.dmsg.net" }
    fun open(invite: BrowserInvitation) {
        if (!trustedBrowserInvitation(invite.url, origin) || invite.expiresAt <= System.currentTimeMillis().toULong()) { client.showError("INVALID_INPUT"); return }
        client.beginExternalInteraction(); browser(invite.url)
    }
    LaunchedEffect(state.epoch) { client.browserConnections.load() }
    LaunchedEffect(state.epoch) {
        while (true) {
            kotlinx.coroutines.delay(5_000); now = System.currentTimeMillis().toULong()
            client.browserConnections.invitations.value().orEmpty().forEach(client.browserConnections::candidates)
        }
    }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            Notice(str(R.string.text_connect_only_a_browser_you_control_a_connection_can_submit_reques_4b16bf2))
            Button(onClick = { client.browserConnections.create(::open) }, enabled = !state.working("browser:create")) { Text(str(R.string.text_connect_a_browser_772f8bd)) }
            ReadFeedback(invitations, client.browserConnections::load)
        }
        items(invitations.valueOrNull().orEmpty(), key = { it.id }) { invite ->
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(str(R.string.text_waiting_for_browser_confirmation_a77bc19), style = MaterialTheme.typography.titleMedium)
                    LabelValue(str(R.string.text_expires_12414d7), displayTime(invite.expiresAt))
                    OutlinedButton(onClick = { open(invite) }, enabled = now < invite.expiresAt) { Text(str(R.string.text_open_invitation_in_browser_ca2e70a)) }
                    TextButton(onClick = { client.browserConnections.close(invite) }, enabled = !state.working("browser:close:" + invite.id)) { Text(str(R.string.text_close_invitation_0a1976d)) }
                    errors[invite.id]?.let { Notice(errorText(it)); TextButton(onClick = { client.browserConnections.candidates(invite) }) { Text(str(R.string.text_check_candidates_again_b760fe5)) } }
                    candidates[invite.id].orEmpty().forEach { candidate ->
                        val connected = pairings.valueOrNull()?.any { it.id == candidate.id && it.state == "active" } == true
                        SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { Text(candidate.label); Text(candidate.id, style = MaterialTheme.typography.bodySmall)  } }
                        if (connected) Text(str(R.string.text_connected_77956f6)) else OutlinedButton(onClick = { grant = invite to candidate }, enabled = now < invite.expiresAt && !state.working("browser:grant:" + candidate.id)) { Text(str(R.string.text_review_this_browser_8d99357)) }
                    }
                }
            }
        }
        item { SectionTitle(str(R.string.text_connected_browsers_e404840)); ReadFeedback(pairings, client.browserConnections::load) }
        items(pairings.valueOrNull().orEmpty(), key = { it.id }) { pairing ->
            Card {
                Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(pairing.label ?: str(R.string.text_browser_0d2c6bc), style = MaterialTheme.typography.titleMedium)
                    SelectionContainer { Text(pairing.id, style = MaterialTheme.typography.bodySmall) }
                    Text(when (pairing.state) { "active" -> str(R.string.text_active_45e5d33); "revoked" -> str(R.string.text_revoked_c4fbad3); "expired" -> str(R.string.text_expired_70446d7); else -> str(R.string.text_status_needs_checking_23c90f0) })
                    pairing.expiresAt?.let { LabelValue(str(R.string.text_expires_12414d7), displayTime(it)) }
                    if (pairing.state == "active") TextButton(onClick = { revoke = pairing }, enabled = !state.working("browser:revoke:" + pairing.id)) { Text(str(R.string.text_revoke_browser_1c594a3), color = MaterialTheme.colorScheme.error) }
                }
            }
        }
        if (pairings is ReadState.Empty) item { EmptyPanel(str(R.string.text_no_connected_browsers_9df2681)) }
    }
    grant?.let { choice ->
        AlertDialog(onDismissRequest = { grant = null }, title = { Text(str(R.string.text_connect_this_browser_8b0cd9a)) }, text = {
            Column { Text(choice.second.label); SelectionContainer { Text(choice.second.id) }; Text(str(R.string.text_confirm_this_candidate_in_the_browser_you_opened_its_label_is_not_26fee81)) }
        }, confirmButton = { TextButton(onClick = { grant = null; client.browserConnections.grant(choice.first, choice.second) }) { Text(str(R.string.text_connect_b3b35de)) } }, dismissButton = { TextButton(onClick = { grant = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
    }
    revoke?.let { pairing ->
        AlertDialog(onDismissRequest = { revoke = null }, title = { Text(str(R.string.text_revoke_this_browser_04ff224)) }, text = { SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { Text(pairing.label.orEmpty()); Text(pairing.id)  } } }, confirmButton = { TextButton(onClick = { revoke = null; client.browserConnections.revoke(pairing) }) { Text(str(R.string.text_revoke_411a6c9)) } }, dismissButton = { TextButton(onClick = { revoke = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
    }
}
@Composable fun BrowserRequestScreen(client: ClientController, link: ConnectLink, browser: (String) -> Unit) {
    val state by client.state.collectAsStateWithLifecycle()
    val request by client.browserConnections.request.state.collectAsStateWithLifecycle()
    if (link.account != null && link.account != state.session?.account) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Notice(str(R.string.text_this_request_belongs_to_another_account_choose_its_account_before_0b05dcb))
            SelectionContainer { Text(link.account) }
            Button(onClick = { client.navigation.open("accounts") }) { Text(str(R.string.text_choose_account_58cfffa)) }
            TextButton(onClick = { client.dismissConnectLink(); client.navigation.back() }) { Text(str(R.string.text_dismiss_request_87156ba)) }
        }
    } else if (link.destination == "pair") BrowserConnectionsScreen(client, browser)
    else {
        LaunchedEffect(link, state.epoch) { client.browserConnections.locate(link) }
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Notice(str(R.string.text_the_app_retrieves_and_verifies_the_original_browser_request_openi_aea3c2b))
            ReadFeedback(request, { client.browserConnections.locate(link) })
            if (request is ReadState.Empty) Text(str(R.string.text_the_request_is_unavailable_to_this_device_it_may_have_expired_or_763c484))
            request.valueOrNull()?.let { id ->
                Button(onClick = { client.dismissConnectLink(); client.approvals.open(id) }) { Text(str(R.string.text_review_verified_request_ecf7242)) }
            }
            TextButton(onClick = { client.navigation.open("approvals") }) { Text(str(R.string.text_open_approvals_639b4b5)) }
            TextButton(onClick = { client.dismissConnectLink(); client.navigation.back() }) { Text(str(R.string.text_dismiss_request_87156ba)) }
        }
    }
}
