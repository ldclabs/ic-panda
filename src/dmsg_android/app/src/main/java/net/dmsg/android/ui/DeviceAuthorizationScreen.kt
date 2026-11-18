package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonPrimitive
import net.dmsg.android.core.*
import java.security.MessageDigest

@Composable fun JoinAccountScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    var account by remember { mutableStateOf("") }
    var token by remember { mutableStateOf<String?>(null) }
    var submitted by remember { mutableStateOf<String?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(str(R.string.text_join_an_existing_account_cc7175b), style = MaterialTheme.typography.headlineSmall)
        Notice(str(R.string.text_ask_an_authorized_administrator_to_approve_this_login_identity_ap_76d01ae))
        OutlinedTextField(account, { account = it.trim(); token = null; submitted = null; failure = null }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(str(R.string.text_account_id_43f0a66)) })
        Button(onClick = {
            val requested = account
            client.perform({ core -> (core.createBindingRequest(requested).decode() as? JsonPrimitive)?.takeIf { it.isString }?.content ?: error("INTEGRITY_FAILED") }, key = "access:binding", onError = { failure = it }) {
                if (account == requested) { token = it; submitted = requested; failure = null }
            }
        }, enabled = validAccount(account) && !state.working("access:binding")) { Text(str(R.string.text_create_binding_request_598bc7a)) }
        token?.let { request ->
            QrCode(request)
            TextButton(onClick = { client.clipboard.copy(request) }) { Text(str(R.string.text_copy_request_b707c92)) }
            SelectionContainer { Text(request, style = MaterialTheme.typography.bodySmall, maxLines = 5) }
            Button(onClick = { submitted?.let { target -> client.perform({ it.acceptBinding(target) }, transition = true, onError = { failure = it }) { client.showAccess(AccessPage.WELCOME) } } }, enabled = !state.busy) { Text(str(R.string.text_continue_after_approval_0bcc500)) }
        }
        failure?.let { Notice(errorText(it), true) }
    }
}

@Composable fun DevicePairingScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    var token by remember { mutableStateOf<String?>(null) }
    var refreshAt by remember { mutableLongStateOf(0) }
    var invitation by remember { mutableStateOf("") }
    var failure by remember { mutableStateOf<String?>(null) }
    var delivered by remember { mutableStateOf<Pair<String, ULong>?>(null) }
    var needsLogin by remember { mutableStateOf(false) }
    var polling by remember { mutableStateOf(true) }
    LaunchedEffect(qr.result) { client.qr.consume("pair-invitation")?.let { invitation = it; delivered = null } }
    fun request() = client.perform({ (it.pairRequest().decode() as? JsonPrimitive)?.takeIf { value -> value.isString }?.content ?: error("INTEGRITY_FAILED") }, key = "access:pair-request", onError = { failure = it }) {
        token = it; refreshAt = android.os.SystemClock.elapsedRealtime() + 270_000; failure = null
    }
    fun check(manual: Boolean = false) {
        if (state.busy || needsLogin || client.qr.state.value is QrRequest || (!manual && !polling)) return
        if (manual) polling = true
        client.perform({ it.refreshAccount() }, transition = true, onError = { code ->
            failure = code; needsLogin = code in setOf("AUTH_REQUIRED", "LOGIN_EXPIRED")
            if (needsLogin || code == "CANCELLED" || code.contains("HARDWARE") || code.startsWith("BIOMETRIC") || code.startsWith("DEVICE_CREDENTIAL") || code == "AUTHENTICATION_TIMEOUT") polling = false
        }) { failure = null }
    }
    LaunchedEffect(Unit) {
        request()
        while (true) {
            delay(5000)
            if (token != null && android.os.SystemClock.elapsedRealtime() >= refreshAt) request()
            if (token != null || delivered != null) check()
        }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(str(R.string.text_authorize_this_android_device_051b886), style = MaterialTheme.typography.headlineSmall)
        Notice(str(R.string.text_your_existing_device_makes_the_final_decision_compare_the_complet_0fdf511))
        token?.let { encoded ->
            QrCode(encoded)
            SelectionContainer { Text(state.session?.deviceId.orEmpty(), style = MaterialTheme.typography.bodySmall) }
            TextButton(onClick = { client.clipboard.copy(encoded) }) { Text(str(R.string.text_copy_device_request_7bde6dd)) }
            TextButton(onClick = ::request, enabled = !state.working("access:pair-request")) { Text(str(R.string.text_refresh_request_b86a6e2)) }
        } ?: if (state.working("access:pair-request")) LinearProgressIndicator(Modifier.fillMaxWidth()) else TextButton(onClick = ::request) { Text(str(R.string.text_show_device_request_8145c2f)) }
        SectionTitle(str(R.string.text_join_with_an_invitation_d50f262))
        OutlinedTextField(invitation, { invitation = it; delivered = null }, Modifier.fillMaxWidth(), minLines = 2, label = { Text(str(R.string.text_paste_desktop_invitation_ebe35cc)) })
        OutlinedButton(onClick = { client.qr.open("pair-invitation") }) { Text(str(R.string.text_scan_invitation_27f0014)) }
        Button(onClick = {
            val shown = invitation.trim()
            client.perform({ core -> core.joinDeviceInvitation(shown).decode().objectValue().let { it.required("device_id") to it.requiredUnsigned("expires_at") } }, key = "access:invitation", onError = { failure = it }) {
                if (invitation.trim() == shown) { delivered = it; failure = null }
            }
        }, enabled = invitation.isNotBlank() && delivered == null && !state.working("access:invitation")) { Text(str(R.string.text_send_device_request_c245e0d)) }
        delivered?.let { (device, expires) ->
            Notice(str(R.string.text_delivered_for_review_the_device_is_not_authorized_yet_5bae486))
            SelectionContainer { Text(device) }; Text(str(R.string.text_expires_d2c7887) + displayTime(expires))
        }
        failure?.let { Notice(errorText(it), true) }
        if (needsLogin) TextButton(onClick = { client.showAccess(AccessPage.WELCOME) }) { Text(str(R.string.text_return_to_sign_in_b22d5c4)) }
        else OutlinedButton(onClick = { check(manual = true) }, enabled = !state.busy) { Text(str(R.string.text_check_authorization_039f8a2)) }
        OutlinedButton(onClick = { client.showAccess(AccessPage.HANDOVER) }) { Text(str(R.string.text_replace_an_old_device_186797a)) }
        TextButton(onClick = { client.showAccess(AccessPage.RECOVERY) }) { Text(str(R.string.text_all_devices_lost_c768f48)) }
    }
}

@Composable fun PairApprovalScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    var token by remember { mutableStateOf("") }
    LaunchedEffect(qr.result) { client.qr.consume("pair-approval")?.let { token = it } }
    val requestKey = remember(token) { "pair:" + MessageDigest.getInstance("SHA-256").digest(token.trim().toByteArray()).joinToString("") { "%02x".format(it) } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(str(R.string.text_authorize_another_device_d67d951), style = MaterialTheme.typography.headlineSmall)
        OutlinedTextField(token, { token = it }, Modifier.fillMaxWidth(), minLines = 3, maxLines = 7, label = { Text(str(R.string.text_device_request_a29f5e8)) })
        OutlinedButton(onClick = { client.qr.open("pair-approval") }) { Text(str(R.string.text_scan_device_request_c50056e)) }
        Button(onClick = {
            val reviewed = token.trim()
            client.approvals.request(requestKey) { core ->
                val device = core.inspectPairingRequest(reviewed).decode().objectValue().required("device_id")
                core.approvePairingRequest(reviewed, device)
            }
        }, enabled = token.isNotBlank() && !state.working("approval:request:" + requestKey)) { Text(str(R.string.text_review_request_cb6fb73)) }
        ApprovalReceipt(client, requestKey)
        Notice(str(R.string.text_this_creates_a_pending_approval_scanning_or_reviewing_does_not_au_dc3cd70))
    }
}

@Composable fun MovedWorkspaceScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    var confirming by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
        Text(str(R.string.text_this_workspace_came_from_another_device_d9830c7), style = MaterialTheme.typography.headlineSmall)
        Notice(str(R.string.text_its_device_bound_keys_cannot_be_replaced_automatically_keep_the_c_c11e52c))
        Button(onClick = { confirming = true }, enabled = !state.busy) { Text(str(R.string.text_set_aside_and_start_a_new_device_1e89498)) }
    }
    if (confirming) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_keep_this_copy_and_continue_4a04eef)) }, text = { Text(str(R.string.text_the_copied_workspace_stays_on_this_device_for_later_import_or_rev_2d360d1)) }, confirmButton = { TextButton(onClick = { confirming = false; client.perform({ it.setAsideMoved() }, transition = true) { client.showAccess(AccessPage.WELCOME) } }) { Text(str(R.string.text_continue_fd19155)) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
