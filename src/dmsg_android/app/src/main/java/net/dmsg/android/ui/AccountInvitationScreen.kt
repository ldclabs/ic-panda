package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.str
import net.dmsg.core.PasskeyOperation

private val transientPollCodes = setOf("UNAVAILABLE", "NETWORK_UNAVAILABLE", "EXECUTION_UNKNOWN", "SYNC_BUSY", "POLICY_STALE")

@OptIn(ExperimentalMaterial3Api::class)
@Composable internal fun AccountJoinScreen(client: ClientController, openLogin: (String) -> Unit) {
    val state by client.state.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    var token by remember { mutableStateOf("") }
    var view by remember { mutableStateOf<JsonObject?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    var manual by remember { mutableStateOf(false) }
    var paused by remember { mutableStateOf(false) }
    var retryAdvance by remember { mutableStateOf(false) }
    var visible by remember { mutableStateOf(true) }
    val stage = view?.text("stage")
    val key = "access:join-invitation"
    fun edit(input: String) { token = input; view = null; failure = null; paused = false; retryAdvance = false }
    fun close() { if (visible) client.showAccountInvitation(false) }
    fun failed(code: String) {
        failure = code
        val current = view
        if (code == "EXPIRED") {
            view = null
            paused = true
        } else if (code in setOf("AUTH_REQUIRED", "LOGIN_EXPIRED") && current != null) {
            view = JsonObject(current.filterKeys { it == "account" || it == "expires_at" } + ("stage" to JsonPrimitive("sign_in")))
            paused = true
        } else {
            paused = code !in transientPollCodes
        }
    }
    fun check(advance: Boolean = false) {
        val input = token.trim()
        if (input.isEmpty() || state.busy || client.isWorking(key) || state.concealed || client.qr.state.value is QrRequest) return
        retryAdvance = advance
        fun currentInput() = visible && token.trim() == input
        fun onError(code: String) { if (currentInput()) failed(code) }
        val current = view
        val phase = current?.text("stage")
        if (advance && phase in setOf("waiting_device", "authorized") && state.session?.principal == current?.text("principal")) {
            client.perform({ it.refreshAccount() }, transition = true, key = key, quiet = true, userInitiated = false, onError = ::onError) { result ->
                if (currentInput()) {
                    failure = null
                    if (result.state == "unlocked" && result.account == current?.text("account")) close()
                    else if (result.state == "pairing_required" && (current?.unsigned("expires_at") ?: 0uL) <= System.currentTimeMillis().toULong()) failed("EXPIRED")
                }
            }
        } else {
            client.perform({ it.accountInvitation(input, if (advance) current?.text("principal") else null, advance && phase == "bind_login").decode().objectValue() },
                key = key, quiet = true, onError = ::onError) { next ->
                if (currentInput()) {
                    view = next
                    failure = null
                    if (next.text("stage") == "authorized" && state.unlocked && state.session?.account == next.text("account")) close()
                }
            }
        }
    }
    BackHandler { if (manual) manual = false else close() }
    DisposableEffect(Unit) { onDispose { visible = false } }
    LaunchedEffect(Unit) {
        client.perform({ it.savedAccountInvitation() }, quiet = true) { saved ->
            if (visible && saved != null && token.isBlank()) { edit(saved); check() }
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
        while (!paused && !state.concealed && stage in setOf("waiting_login", "waiting_device", "authorized")) {
            delay(4000)
            check(advance = true)
        }
    }
    if (state.concealed) return
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.page) {
        if (manual) {
            Column(Modifier.fillMaxSize()) {
                TopAppBar(
                    title = { Text(str(R.string.text_join_an_existing_account_cc7175b)) },
                    navigationIcon = { IconButton(onClick = { manual = false }) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, str(R.string.text_back_5db5cac)) } },
                    colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.page),
                )
                JoinAccountScreen(client)
            }
            LaunchedEffect(state.session?.state) { if (state.session?.state != "no_account") { manual = false; close() } }
            return@Surface
        }
        Column(Modifier.fillMaxSize()) {
            TopAppBar(
                title = { Text(str(R.string.access_existing_account)) },
                navigationIcon = { IconButton(onClick = ::close) { Icon(Icons.Outlined.Close, str(R.string.text_close_6cf4a77)) } },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.page),
            )
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).imePadding().navigationBarsPadding().padding(pagePadding()),
                verticalArrangement = Arrangement.spacedBy(16.dp)) {
                failure?.let {
                    Notice(if (it == "EXPIRED") str(R.string.text_this_invitation_has_expired_ask_for_a_new_invitation_d58d62e) else errorText(it), true)
                    if (it != "EXPIRED") OutlinedButton(onClick = { paused = false; check(retryAdvance) }, enabled = !state.busy && !state.working(key), modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_retry_563171c))
                    }
                }
                val current = view
                if (current == null) {
                    DetailCard {
                        Text(str(R.string.join_scan_hint), style = MaterialTheme.typography.bodyMedium)
                        Button(onClick = { client.qr.open("join-account") }, modifier = Modifier.fillMaxWidth()) {
                            Icon(Icons.Outlined.QrCodeScanner, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_scan_invitation_27f0014))
                        }
                        OutlinedTextField(token, ::edit, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_paste_invitation_0989300)) })
                        OutlinedButton(onClick = { check() }, enabled = token.isNotBlank() && !state.busy && !state.working(key), modifier = Modifier.fillMaxWidth()) {
                            Text(str(R.string.text_review_9a1a906))
                        }
                    }
                    if (state.session?.state == "no_account") Section {
                        ListRow(str(R.string.text_create_binding_request_598bc7a), icon = Icons.Outlined.Key) { manual = true }
                    }
                } else {
                    DetailCard {
                        LabelValue(str(R.string.text_account_id_43f0a66), current.required("account"), code = true)
                        if (stage == "bind_login") current.text("principal")?.let { LabelValue(str(R.string.join_identity), it, code = true) }
                        current.text("code")?.let {
                            Text(str(R.string.join_code), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            SelectionContainer { Text(it, style = MaterialTheme.typography.headlineSmall.code().tabular(), color = MaterialTheme.colorScheme.primary) }
                            Text(str(R.string.join_compare_hint), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        current.text("device_id")?.let { TransferIdentity(str(R.string.text_device_id_b31993b), it) }
                    }
                    when (stage) {
                        "sign_in", "other_account", "linked_login_required" -> {
                            if (stage == "other_account") {
                                Notice(str(R.string.join_other))
                                current.text("current_account")?.let { Section { CodeRow(str(R.string.text_current_account_59c0362), it) } }
                            } else if (stage == "linked_login_required") {
                                Notice(str(R.string.access_linked_login))
                            }
                            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                if (state.session?.passkeyAvailable == true) {
                                    Button(onClick = { client.passkey() }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.access_existing_passkey)) }
                                    if (stage != "linked_login_required") OutlinedButton(onClick = { client.passkey(PasskeyOperation.REGISTER) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                                        Text(str(R.string.text_create_passkey_fb912be))
                                    }
                                }
                                OutlinedButton(onClick = { client.login(open = openLogin) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                                    Text(str(R.string.text_sign_in_with_internet_identity_4d3941e))
                                }
                            }
                        }
                        "bind_login", "device" -> Button(onClick = { check(true) }, enabled = !state.busy && !state.working(key), modifier = Modifier.fillMaxWidth()) {
                            Text(str(if (stage == "bind_login") R.string.join_confirm else R.string.text_send_device_request_c245e0d))
                        }
                        else -> {
                            if (stage == "waiting_device") Section { ListRow(str(R.string.join_identity), value = str(R.string.text_completed_6ff49bc)) }
                            Notice(str(if (state.session?.state == "root_required") R.string.pair_preparing_vault else if (stage == "waiting_login") R.string.join_wait_login else R.string.join_wait_device))
                        }
                    }
                    OutlinedButton(onClick = { edit(""); client.qr.open("join-account") }, enabled = !state.working(key), modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Outlined.QrCodeScanner, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_scan_invitation_27f0014))
                    }
                }
                if (state.working(key)) LinearProgressIndicator(Modifier.fillMaxWidth())
            }
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
    var pollWarning by remember { mutableStateOf<String?>(null) }
    var turn by remember { mutableIntStateOf(0) }
    var alive by remember { mutableStateOf(true) }
    val key = "devices:account-invite"
    fun cancelOnRelay(id: String?) {
        id?.let { client.perform({ core -> core.cancelAccountInvitation(it) }, quiet = true) }
    }
    fun pollError(id: String, code: String) {
        if (code in transientPollCodes) {
            pollWarning = code
        } else {
            pollWarning = null
            receipt = null
            candidate = null
            error = code
            cancelOnRelay(id)
        }
    }
    DisposableEffect(Unit) {
        alive = true
        onDispose {
            alive = false
            cancelOnRelay(invite?.text("id"))
        }
    }
    LaunchedEffect(turn) {
        invite = null; candidate = null; receipt = null; finished = false; loginDone = false; error = null; pollWarning = null
        // Core's InvitationHost cancels any previous invitation before creating the next one.
        client.perform({ it.startAccountInvitation().decode().objectValue() },
            key = key, onError = { error = it }) { value ->
            if (alive) invite = value
            else cancelOnRelay(value.text("id"))
        }
    }
    LaunchedEffect(invite, finished, error, state.concealed) {
        val id = invite?.text("id") ?: return@LaunchedEffect
        while (!finished && error == null && !state.concealed) {
            delay(2000)
            if (client.isWorking(key)) continue
            val pending = receipt
            if (pending != null) {
                client.perform({ it.approval(pending).decode().objectValue().required("state") }, key = key, quiet = true, onError = { pollError(id, it) }) { result ->
                    pollWarning = null
                    if (result == "done") {
                        if (candidate?.text("kind") == "login_binding") loginDone = true
                        else { finished = true; cancelOnRelay(id) }
                        receipt = null; candidate = null
                    } else if (result in setOf("failed", "rejected", "expired", "cancelled")) {
                        receipt = null; candidate = null; error = "PAIR_REFRESH"
                        cancelOnRelay(id)
                    }
                }
            } else if (candidate == null) {
                client.perform({ it.accountInvitationStatus(id).decode().objectValue()["candidate"] as? JsonObject },
                    key = key, quiet = true, onError = { pollError(id, it) }) {
                    pollWarning = null
                    if (!loginDone || it?.text("kind") != "login_binding") candidate = it
                }
            }
        }
    }
    if (state.concealed) return
    val pending = receipt
    if (pending != null) {
        ApprovalsScreen(client, state, pending) {}
        return
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        pollWarning?.let { Notice(errorText(it), true) }
        val shown = candidate
        when {
            error != null -> {
                Notice(if (error in setOf("PAIR_REFRESH", "EXPIRED", "VERSION_CONFLICT")) str(R.string.pair_refresh_request) else errorText(error!!), true)
                Button(onClick = { turn++ }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_retry_563171c)) }
            }
            invite?.text("route") == "handoff" -> {
                val admins = invite?.strings("approvers").orEmpty()
                Notice(str(if (admins.isEmpty()) R.string.pair_no_approver else R.string.pair_continue_elsewhere))
                if (admins.isNotEmpty()) DetailCard {
                    admins.forEach { SelectionContainer { Text(it, style = MaterialTheme.typography.bodySmall.code()) } }
                }
            }
            finished -> EmptyState(str(R.string.text_completed_6ff49bc), icon = Icons.Outlined.CheckCircleOutline)
            shown != null -> {
                DetailCard {
                    shown.text("code")?.let {
                        Text(str(R.string.join_code), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        SelectionContainer { Text(it, style = MaterialTheme.typography.headlineSmall.code().tabular(), color = MaterialTheme.colorScheme.primary) }
                        Text(str(R.string.join_compare_hint), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    TransferIdentity(str(R.string.text_device_id_b31993b), shown.required("device_id"))
                    shown.text("principal")?.let { LabelValue(str(R.string.join_identity), it, code = true) }
                    shown.text("role")?.let { LabelValue(str(R.string.text_role_b9e0249), deviceRoleText(it)) }
                    if (shown.containsKey("capabilities")) LabelValue(str(R.string.text_permissions_be9338a), shown.strings("capabilities").joinToString(", ", transform = ::deviceCapabilityText))
                }
                Button(
                    onClick = {
                        client.perform({ core ->
                            val token = shown.required("token")
                            (if (shown.text("kind") == "login_binding") core.reviewBinding(token)
                            else core.approvePairingRequest(token, shown.required("device_id"))).decode().objectValue().required("id")
                        }, key = key, onError = { error = if (it in setOf("EXPIRED", "VERSION_CONFLICT")) "PAIR_REFRESH" else it }) { receipt = it }
                    },
                    enabled = !state.working(key),
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(str(R.string.text_review_9a1a906)) }
            }
            loginDone -> Notice(str(R.string.join_wait_device))
            invite?.text("token") != null -> {
                val token = invite!!.required("token")
                QrCode(token)
                DetailCard {
                    state.session?.account?.let { LabelValue(str(R.string.text_account_id_43f0a66), it, code = true) }
                    Text(str(R.string.join_show_hint), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    OutlinedButton(onClick = { client.clipboard.copy(token) }, modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Outlined.ContentCopy, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_copy_request_b707c92))
                    }
                }
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
