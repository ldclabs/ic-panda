package net.dmsg.android.ui

import androidx.activity.compose.BackHandler
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
import kotlinx.serialization.json.JsonObject
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.str
import net.dmsg.core.PasskeyOperation

@Composable internal fun AccountJoinScreen(client: ClientController, openLogin: (String) -> Unit) {
    val state by client.state.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    var token by remember { mutableStateOf("") }
    var view by remember { mutableStateOf<JsonObject?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    var manual by remember { mutableStateOf(false) }
    var paused by remember { mutableStateOf(false) }
    val stage = view?.text("stage")
    val key = "access:join-invitation"
    fun edit(input: String) { token = input; view = null; failure = null; paused = false }
    fun close() { client.showAccountInvitation(false) }
    fun failed(code: String) {
        failure = code
        paused = code !in setOf("UNAVAILABLE", "EXECUTION_UNKNOWN", "SYNC_BUSY", "POLICY_STALE")
    }
    fun check(advance: Boolean = false) {
        val input = token.trim()
        if (input.isEmpty() || state.busy || client.isWorking(key) || state.concealed || client.qr.state.value is QrRequest) return
        val current = view
        val phase = current?.text("stage")
        if (advance && phase in setOf("waiting_device", "authorized") && state.session?.principal == current?.text("principal")) {
            client.perform({ it.refreshAccount() }, transition = true, key = key, quiet = true, userInitiated = false, onError = ::failed) { result ->
                if (token.trim() == input) {
                    failure = null
                    if (result.state == "unlocked" && result.account == current?.text("account")) close()
                    else if (result.state == "pairing_required" && (current?.unsigned("expires_at") ?: 0uL) <= System.currentTimeMillis().toULong()) failed("EXPIRED")
                }
            }
        } else {
            client.perform({ it.accountInvitation(input, if (advance) current?.text("principal") else null, advance && phase == "bind_login").decode().objectValue() },
                key = key, quiet = true, onError = ::failed) { next ->
                if (token.trim() == input) { view = next; failure = null }
            }
        }
    }
    BackHandler { if (manual) manual = false else close() }
    LaunchedEffect(Unit) {
        client.perform({ it.savedAccountInvitation() }, quiet = true) { saved ->
            if (saved != null && token.isBlank()) { edit(saved); check() }
        }
    }
    LaunchedEffect(qr.result) {
        client.qr.consume("join-account")?.let { edit(it); check() }
    }
    LaunchedEffect(state.session?.principal, state.busy, state.concealed) {
        val signedIn = state.session?.principal
        if (!state.busy && !state.concealed && view != null && view?.text("principal") != signedIn
            && token.isNotBlank()) {
            paused = false
            check()
        }
    }
    LaunchedEffect(stage, paused, state.concealed, state.session?.principal) {
        while (!paused && !state.concealed && stage in setOf("waiting_login","waiting_device","authorized")) {
            delay(4000)
            check(advance = true)
        }
    }
    if (state.concealed) return
    if (manual) {
        Column(Modifier.fillMaxSize().systemBarsPadding()) {
            TextButton(onClick = { manual = false }) { Text(str(R.string.text_back_5db5cac)) }
            JoinAccountScreen(client)
        }
        LaunchedEffect(state.session?.state) { if (state.session?.state != "no_account") { manual = false; close() } }
        return
    }
    Column(Modifier.fillMaxSize().systemBarsPadding().verticalScroll(rememberScrollState()).imePadding().padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        TextButton(onClick = ::close) { Text(str(R.string.text_close_6cf4a77)) }
        Text(str(R.string.access_existing_account), style = MaterialTheme.typography.headlineSmall)
        val current = view
        if (current == null) {
            Notice(str(R.string.join_scan_hint))
            Button(onClick = { client.qr.open("join-account") }) { Text(str(R.string.text_scan_invitation_27f0014)) }
            OutlinedTextField(token, ::edit, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_paste_invitation_0989300)) })
            OutlinedButton(onClick = { check() }, enabled = token.isNotBlank() && !state.working(key)) { Text(str(R.string.text_review_9a1a906)) }
            if (state.session?.state == "no_account") TextButton(onClick = { manual = true }) { Text(str(R.string.text_create_binding_request_598bc7a)) }
        } else {
            LabelValue(str(R.string.text_account_id_43f0a66), current.required("account"))
            current.text("code")?.let { LabelValue(str(R.string.join_code), it); Text(str(R.string.join_compare_hint)) }
            current.text("device_id")?.let { SelectionContainer { Text(it, style = MaterialTheme.typography.bodySmall) } }
            when (stage) {
                "sign_in", "other_account", "linked_login_required" -> {
                    if (stage == "other_account") {
                        Notice(str(R.string.join_other)); current.text("current_account")?.let { SelectionContainer { Text(it) } }
                    }
                    if (state.session?.passkeyAvailable == true) {
                        Button(onClick = { client.passkey() }, enabled = !state.busy) { Text(str(R.string.access_existing_passkey)) }
                        if (stage != "linked_login_required") OutlinedButton(onClick = { client.passkey(PasskeyOperation.REGISTER) }, enabled = !state.busy) { Text(str(R.string.text_create_passkey_fb912be)) }
                    }
                    OutlinedButton(onClick = { client.login(open = openLogin) }, enabled = !state.busy) { Text(str(R.string.text_sign_in_with_internet_identity_4d3941e)) }
                }
                "bind_login", "device" -> Button(onClick = { check(true) }, enabled = !state.working(key)) {
                    Text(str(if (stage == "bind_login") R.string.join_confirm else R.string.text_send_device_request_c245e0d))
                }
                else -> {
                    if (stage == "waiting_device") LabelValue(str(R.string.join_identity), str(R.string.text_completed_6ff49bc))
                    Notice(str(if (state.session?.state == "root_required") R.string.pair_preparing_vault else if (stage == "waiting_login") R.string.join_wait_login else R.string.join_wait_device))
                }
            }
            TextButton(onClick = { edit(""); client.qr.open("join-account") }, enabled = !state.working(key)) { Text(str(R.string.text_scan_invitation_27f0014)) }
        }
        if (state.working(key)) LinearProgressIndicator(Modifier.fillMaxWidth())
        failure?.let {
            Notice(errorText(it), true)
            OutlinedButton(onClick = { paused = false; check(stage in setOf("waiting_login","waiting_device","authorized")) }) { Text(str(R.string.text_retry_563171c)) }
        }
    }
}

@Composable internal fun AccountInviteHostScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    var invite by remember { mutableStateOf<JsonObject?>(null) }
    var candidate by remember { mutableStateOf<JsonObject?>(null) }
    var receipt by remember { mutableStateOf<String?>(null) }
    var loginDone by remember { mutableStateOf(false) }
    var finished by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var turn by remember { mutableIntStateOf(0) }
    var alive by remember { mutableStateOf(true) }
    val key = "devices:account-invite"
    DisposableEffect(Unit) {
        alive = true
        onDispose {
            alive = false
            invite?.text("id")?.let { id -> client.perform({ it.cancelAccountInvitation(id) }, quiet = true) }
        }
    }
    LaunchedEffect(turn) {
        val previous = invite?.text("id")
        invite = null; candidate = null; receipt = null; finished = false; loginDone = false; error = null
        client.perform({ if (previous != null) it.cancelAccountInvitation(previous); it.startAccountInvitation().decode().objectValue() },
            key = key, onError = { error = it }) { value ->
            if (alive) invite = value
            else value.text("id")?.let { id -> client.perform({ it.cancelAccountInvitation(id) }, quiet = true) }
        }
    }
    LaunchedEffect(invite, finished, error, state.concealed) {
        val id = invite?.text("id") ?: return@LaunchedEffect
        while (!finished && error == null && !state.concealed) {
            delay(2000)
            val pending = receipt
            if (pending != null) {
                client.perform({ it.approval(pending).decode().objectValue().required("state") }, key = key, quiet = true, onError = { error = it }) { result ->
                    if (result == "done") {
                        if (candidate?.text("kind") == "login_binding") loginDone = true else finished = true
                        receipt = null; candidate = null
                    } else if (result in setOf("failed","rejected","expired","cancelled")) { error = result.uppercase(); receipt = null }
                }
            } else if (candidate == null) {
                client.perform({ it.accountInvitationStatus(id).decode().objectValue()["candidate"] as? JsonObject },
                    key = key, quiet = true, onError = { error = it }) { if (!loginDone || it?.text("kind") != "login_binding") candidate = it }
            }
        }
    }
    if (state.concealed) return
    val pending = receipt
    if (pending != null) {
        ApprovalsScreen(client, state, pending) {}
        return
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        val shown = candidate
        when {
            error != null -> { Notice(errorText(error!!), true); Button(onClick = { turn++ }) { Text(str(R.string.text_retry_563171c)) } }
            invite?.text("route") == "handoff" -> {
                val admins = invite?.strings("approvers").orEmpty()
                Notice(str(if (admins.isEmpty()) R.string.pair_no_approver else R.string.pair_continue_elsewhere))
                admins.forEach { SelectionContainer { Text(it) } }
            }
            finished -> Text(str(R.string.text_completed_6ff49bc))
            shown != null -> {
                shown.text("code")?.let { LabelValue(str(R.string.join_code), it); Text(str(R.string.join_compare_hint)) }
                SelectionContainer { Text(shown.required("device_id")) }
                shown.text("principal")?.let { LabelValue(str(R.string.join_identity), it) }
                shown.text("role")?.let { LabelValue(str(R.string.text_role_b9e0249), deviceRoleText(it)) }
                if (shown.containsKey("capabilities")) LabelValue(str(R.string.text_permissions_be9338a), shown.strings("capabilities").joinToString(", ", transform = ::deviceCapabilityText))
                Button(onClick = {
                    client.perform({ core ->
                        val token = shown.required("token")
                        (if (shown.text("kind") == "login_binding") core.reviewBinding(token)
                         else core.approvePairingRequest(token, shown.required("device_id"))).decode().objectValue().required("id")
                    }, key = key, onError = { error = it }) { receipt = it }
                }, enabled = !state.working(key)) { Text(str(R.string.text_review_9a1a906)) }
            }
            loginDone -> Notice(str(R.string.join_wait_device))
            invite?.text("token") != null -> {
                state.session?.account?.let { LabelValue(str(R.string.text_account_id_43f0a66), it) }
                val token = invite!!.required("token")
                QrCode(token)
                TextButton(onClick = { client.clipboard.copy(token) }) { Text(str(R.string.text_copy_request_b707c92)) }
                Notice(str(R.string.join_show_hint))
            }
            else -> LinearProgressIndicator(Modifier.fillMaxWidth())
        }
    }
}

internal fun deviceRoleText(role: String): String = when (role) {
    "Administrator" -> str(R.string.text_administrator_f8e6ab3)
    "Member" -> str(R.string.text_member_9b8c32d)
    else -> role
}

internal fun deviceCapabilityText(capability: String): String = when (capability) {
    "ContentSign" -> str(R.string.device_content_signing)
    "VaultUnlock" -> str(R.string.text_unlock_workspace_ec9cb2a)
    "RootManage" -> str(R.string.text_account_security_41c2886)
    "FormalApprove" -> str(R.string.text_approvals_1b5e94c)
    "PaymentOffer" -> str(R.string.text_payments_01597be)
    "WalletApprove" -> str(R.string.text_multisig_wallets_451404c)
    else -> capability
}
