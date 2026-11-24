package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import android.content.Intent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import net.dmsg.android.core.*
import java.security.MessageDigest

@Composable fun JoinAccountScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    var account by remember { mutableStateOf("") }
    var token by remember { mutableStateOf<String?>(null) }
    var submitted by remember { mutableStateOf<String?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        failure?.let { Notice(errorText(it), true) }
            ?: Notice(str(R.string.text_ask_an_authorized_administrator_to_approve_this_login_identity_ap_76d01ae))
        DetailCard {
            Text(str(R.string.text_join_an_existing_account_cc7175b), style = MaterialTheme.typography.titleLarge)
            OutlinedTextField(account, { account = it.trim(); token = null; submitted = null; failure = null }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(str(R.string.text_account_id_43f0a66)) })
            Button(
                onClick = {
                    val requested = account
                    client.perform({ core -> (core.createBindingRequest(requested).decode() as? JsonPrimitive)?.takeIf { it.isString }?.content ?: error("INTEGRITY_FAILED") }, key = "access:binding", onError = { failure = it }) {
                        if (account == requested) { token = it; submitted = requested; failure = null }
                    }
                },
                enabled = validAccount(account) && !state.working("access:binding"),
                modifier = Modifier.fillMaxWidth(),
            ) { Text(str(R.string.text_create_binding_request_598bc7a)) }
        }
        token?.let { request ->
            QrCode(request)
            DetailCard {
                state.session?.principal?.let { LabelValue(str(R.string.join_identity), it, code = true) }
                SelectionContainer { Text(request, style = MaterialTheme.typography.bodySmall.code(), maxLines = 5) }
                CopyButton(str(R.string.text_copy_request_b707c92), outlined = true, key = request) { client.clipboard.copy(request) }
                Button(
                    onClick = { submitted?.let { target -> client.perform({ it.acceptBinding(target) }, transition = true, onError = { failure = it }) { client.showAccess(AccessPage.WELCOME) } } },
                    enabled = !state.busy,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(str(R.string.text_continue_after_approval_0bcc500)) }
            }
        }
    }
}

@Composable fun DevicePairingScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    var token by remember { mutableStateOf<String?>(null) }
    var refreshAt by remember { mutableLongStateOf(0) }
    var failure by remember { mutableStateOf<String?>(null) }
    var needsLogin by remember { mutableStateOf(false) }
    var polling by rememberSaveable(state.session?.deviceId) { mutableStateOf(true) }
    val preparing = state.session?.state == "root_required"
    fun request() = client.perform({ (it.pairRequest().decode() as? JsonPrimitive)?.takeIf { value -> value.isString }?.content ?: error("INTEGRITY_FAILED") }, key = "access:pair-request", onError = { failure = it }) {
        token = it; refreshAt = android.os.SystemClock.elapsedRealtime() + 270_000; failure = null
    }
    fun check(manual: Boolean = false) {
        if (state.busy || state.concealed || needsLogin || client.qr.state.value is QrRequest) return
        // The refresh transition cancels other operations in progress.
        if (client.isWorking("access:pair-request")) return
        if (!manual && (!polling || state.session?.state !in setOf("pairing_required", "root_required"))) return
        if (manual) polling = true
        client.perform({ it.refreshAccount() }, transition = true, userInitiated = manual, onError = { code ->
            failure = code; needsLogin = code in setOf("AUTH_REQUIRED", "LOGIN_EXPIRED")
            if (needsLogin || code in setOf("CANCELLED", "LOCKED", "FORBIDDEN", "INTEGRITY_FAILED", "AUTHENTICATION_TIMEOUT") || code.contains("HARDWARE") || code.startsWith("BIOMETRIC") || code.startsWith("DEVICE_CREDENTIAL")) polling = false
        }) { refreshed ->
            failure = null
            if (refreshed.state == "pairing_required" && token != null && android.os.SystemClock.elapsedRealtime() >= refreshAt) request()
        }
    }
    LaunchedEffect(Unit) {
        if (state.session?.state == "pairing_required") request()
        while (true) {
            delay(5000)
            check()
        }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        val currentFailure = failure
        when {
            currentFailure != null -> Notice(errorText(currentFailure), true)
            preparing -> Notice(str(R.string.pair_preparing_vault))
            state.session?.state == "pairing_required" -> Notice(str(R.string.text_your_existing_device_makes_the_final_decision_compare_the_complet_0fdf511))
        }
        if (state.session?.state == "pairing_required") {
            token?.let { encoded ->
                QrCode(encoded)
                DetailCard {
                    TransferIdentity(str(R.string.text_device_id_b31993b), state.session?.deviceId.orEmpty())
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        CopyButton(str(R.string.text_copy_device_request_7bde6dd), key = encoded) { client.clipboard.copy(encoded) }
                        TextButton(onClick = ::request, enabled = !state.working("access:pair-request")) {
                            Icon(Icons.Outlined.Refresh, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_refresh_request_b86a6e2))
                        }
                    }
                }
            } ?: DetailCard {
                if (state.working("access:pair-request")) LinearProgressIndicator(Modifier.fillMaxWidth())
                else OutlinedButton(onClick = ::request, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_show_device_request_8145c2f)) }
            }
            OutlinedButton(onClick = { client.showAccountInvitation(true) }, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Outlined.QrCodeScanner, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_scan_invitation_27f0014))
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            if (needsLogin) Button(onClick = { client.showAccess(AccessPage.WELCOME) }, modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_return_to_sign_in_b22d5c4))
            } else Button(
                onClick = { check(manual = true) },
                enabled = !state.busy && !state.working("access:pair-request"),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(str(if (!polling || state.session?.state == "locked") R.string.text_unlock_workspace_ec9cb2a else R.string.text_check_authorization_039f8a2))
            }
            OutlinedButton(onClick = { client.showAccess(AccessPage.HANDOVER) }, modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_replace_an_old_device_186797a))
            }
        }
        Section {
            ListRow(str(R.string.text_all_devices_lost_c768f48), glyph = R.drawable.glyph_recovery) { client.showAccess(AccessPage.RECOVERY) }
        }
    }
}

@Composable private fun DeviceFingerprint(value: String) {
    SelectionContainer {
        Text(value.chunked(4).joinToString(" "), style = MaterialTheme.typography.bodySmall.code())
    }
}

@Composable internal fun DeviceRequestCode(client: ClientController, token: String, device: String?, key: String? = null) {
    val context = LocalContext.current
    QrCode(token)
    DetailCard {
        if (device != null) {
            Text(str(R.string.text_device_id_b31993b), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            DeviceFingerprint(device)
        }
        if (key != null) {
            Text(str(R.string.pair_key_fingerprint), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            DeviceFingerprint(key)
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = {
                client.beginExternalInteraction()
                val intent = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, token)
                context.startActivity(Intent.createChooser(intent, null))
            }) { Icon(Icons.Outlined.Share, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.pair_share_request)) }
            CopyButton(str(R.string.text_copy_request_b707c92), key = token) { client.clipboard.copy(token) }
        }
    }
}

private fun pairingReceipt(token: String): String =
    "pair:" + MessageDigest.getInstance("SHA-256").digest(token.trim().toByteArray()).joinToString("") { "%02x".format(it) }

@Composable fun PairApprovalScreen(client: ClientController, enrollmentOnly: Boolean = false) {
    val state by client.state.collectAsStateWithLifecycle()
    var token by remember { mutableStateOf("") }
    var reviewError by remember { mutableStateOf<String?>(null) }
    var preview by remember { mutableStateOf<JsonObject?>(null) }
    var enrollment by remember { mutableStateOf<JsonObject?>(null) }
    var visible by remember { mutableStateOf(true) }
    DisposableEffect(Unit) { visible = true; onDispose { visible = false } }
    fun edit(value: String) { token = value; preview = null; enrollment = null; reviewError = null }
    val requestKey = remember(token) { pairingReceipt(token) }
    fun review() {
        val input = token.trim()
        reviewError = null
        client.perform({ it.inspectPairingRequest(input).decode().objectValue() }, key = "pair:inspect", replace = true,
            quiet = true, onError = { code -> if (visible && input == token.trim()) reviewError = code }) { inspected ->
            if (!visible || input != token.trim()) return@perform
            val key = inspected.required("kind") == "approval_key"
            if (enrollmentOnly && !key) { reviewError = "INVALID_INPUT"; return@perform }
            val firstLoginPreview = inspected.text("kind") == "login_binding" && preview?.text("principal") != inspected.text("principal")
            preview = inspected
            if (!firstLoginPreview && inspected.required("route") == "approve") client.approvals.request(
                pairingReceipt(input),
                active = { visible && token.trim() == input },
                onError = { code -> if (visible && input == token.trim()) reviewError = code },
            ) { core ->
                if (inspected.text("kind") == "login_binding") return@request core.reviewBinding(input)
                val device = inspected.required("device_id")
                if (key) core.approveApprovalKey(input, device) else core.approvePairingRequest(input, device)
            }
        }
    }
    val qr by client.qr.state.collectAsStateWithLifecycle()
    LaunchedEffect(qr.result) { client.qr.consume("pair-approval")?.let { edit(it); review() } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        reviewError?.let { code ->
            Notice(if (code == "EXPIRED" || code == "VERSION_CONFLICT") str(R.string.pair_refresh_request) else errorText(code), true)
        }
        if (preview?.text("route") != "handoff") {
            DetailCard {
                Text(str(if (enrollmentOnly) R.string.text_review_an_enrollment_request_65ae501 else R.string.text_authorize_another_device_d67d951), style = MaterialTheme.typography.titleMedium)
                OutlinedTextField(token, ::edit, Modifier.fillMaxWidth(), minLines = 2, maxLines = 5, label = { Text(str(R.string.text_device_request_a29f5e8)) })
                OutlinedButton(onClick = { client.qr.open("pair-approval") }, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.QrCodeScanner, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_scan_device_request_c50056e))
                }
                Button(onClick = ::review, enabled = token.isNotBlank() && !state.working("pair:inspect"), modifier = Modifier.fillMaxWidth()) {
                    Text(str(R.string.text_review_request_cb6fb73))
                }
            }
            ApprovalReceipt(client, requestKey)
        }
        preview?.let { inspected ->
            inspected.text("principal")?.let {
                Section { CodeRow(str(R.string.join_identity), it) }
            }
            when (inspected.required("route")) {
                "done" -> Notice(str(R.string.text_completed_6ff49bc))
                "refresh" -> Notice(str(R.string.pair_refresh_request))
                "handoff" -> {
                    DetailCard {
                        Text(str(if (enrollment == null) R.string.pair_continue_elsewhere else R.string.pair_enable_approvals), style = MaterialTheme.typography.titleMedium)
                        Text(str(R.string.pair_scan_with_admin), style = MaterialTheme.typography.bodyMedium)
                    }
                    if (enrollment == null) {
                        DeviceRequestCode(client, token.trim(), inspected.text("device_id"), inspected.text("key_id"))
                        val devices = inspected.strings("approvers")
                        if (devices.isEmpty()) {
                            Notice(str(R.string.pair_no_approver))
                            Section { ListRow(str(R.string.text_account_recovery_04aaf7e), glyph = R.drawable.glyph_recovery) { client.navigation.open("recovery") } }
                        } else DetailCard {
                            devices.forEach { id -> DeviceFingerprint(id) }
                        }
                    }
                    if (enrollment == null && inspected.requiredFlag("can_enroll")) {
                        OutlinedButton(
                            onClick = {
                                val input = token
                                client.perform({ it.approvalKeyRequest(false).decode().objectValue() }, key = "pair:enrollment") {
                                    if (visible && token == input) enrollment = it
                                }
                            },
                            enabled = !state.working("pair:enrollment"),
                            modifier = Modifier.fillMaxWidth(),
                        ) { Text(str(R.string.pair_enable_approvals)) }
                    }
                    enrollment?.let { key ->
                        if (inspected.required("kind") == "device_pair") Notice(str(R.string.pair_after_registration))
                        DeviceRequestCode(client, key.required("token"), key.required("device_id"), key.required("key_id"))
                    }
                    OutlinedButton(onClick = { edit(""); client.qr.open("pair-approval") }, modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Outlined.QrCodeScanner, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_scan_device_request_c50056e))
                    }
                }
                else -> Unit
            }
        }
    }
}

@Composable fun MovedWorkspaceScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    var confirming by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Notice(str(R.string.text_its_device_bound_keys_cannot_be_replaced_automatically_keep_the_c_c11e52c))
        DetailCard {
            Text(str(R.string.text_this_workspace_came_from_another_device_d9830c7), style = MaterialTheme.typography.titleLarge)
            Button(onClick = { confirming = true }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_set_aside_and_start_a_new_device_1e89498))
            }
        }
    }
    if (confirming) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_keep_this_copy_and_continue_4a04eef)) }, text = { Text(str(R.string.text_the_copied_workspace_stays_on_this_device_for_later_import_or_rev_2d360d1)) }, confirmButton = { TextButton(onClick = { confirming = false; client.perform({ it.setAsideMoved() }, transition = true) { client.showAccess(AccessPage.WELCOME) } }) { Text(str(R.string.text_continue_fd19155)) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
