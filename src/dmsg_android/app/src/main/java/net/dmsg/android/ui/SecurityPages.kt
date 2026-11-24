package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import kotlinx.serialization.json.JsonObject
import net.dmsg.android.R
import net.dmsg.android.platform.quantity
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Logout
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import net.dmsg.android.core.*
import net.dmsg.core.PasskeyOperation

@Composable fun LoginIdentitiesScreen(client: ClientController, openLogin: (String) -> Unit) {
    val session by client.state.collectAsStateWithLifecycle()
    val security by client.account.security.state.collectAsStateWithLifecycle()
    val devices by client.account.devices.state.collectAsStateWithLifecycle()
    val authentication by client.authentication.state.collectAsStateWithLifecycle()
    var token by remember { mutableStateOf("") }
    var choose by remember { mutableStateOf(false) }
    var logoutAll by remember { mutableStateOf(false) }
    val value = security.valueOrNull()
    val manage = devices.valueOrNull()?.canManage == true && devices is ReadState.Ready
    LaunchedEffect(session.epoch) { client.account.security(); client.approvals.settle() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(security, devices, client.account::security, client.account::devices)
        when {
            !manage && devices is ReadState.Ready -> Notice(str(R.string.text_an_administrator_device_is_required_to_change_account_security_b4649ea))
            session.bindingRetry -> Notice(str(R.string.text_the_verified_login_is_waiting_for_binding_continue_the_original_a_74d014f))
            session.session?.passkeyAvailable == true && authentication.canContinue -> Notice(str(R.string.text_the_passkey_is_saved_continue_this_same_attempt_without_creating_5c00d64))
        }
        value?.logins?.forEach { login ->
            DetailCard {
                if (login.current) Text(str(R.string.text_current_login_3d84cda), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                SelectionContainer { Text(login.principal, style = MaterialTheme.typography.bodyMedium.code()) }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CopyButton(str(R.string.text_copy_identity_f95ea28)) { client.clipboard.copy(login.principal) }
                    TextButton(
                        onClick = { client.approvals.request("login:remove:" + login.principal) { it.removeLogin(login.principal) } },
                        enabled = manage && !login.current && value.logins.size > 1,
                    ) { Text(str(R.string.text_review_removing_this_login_7a4b004), color = if (manage && !login.current && value.logins.size > 1) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant) }
                }
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            if (session.session?.passkeyAvailable == true) {
                Button(onClick = { if (authentication.canContinue) client.continuePasskey() else choose = true }, enabled = manage && !session.busy && !authentication.working, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.Key, null); Spacer(Modifier.width(8.dp))
                    Text(if (authentication.canContinue) str(R.string.text_continue_adding_this_passkey_ed43cf3) else str(R.string.text_add_a_passkey_login_4978775))
                }
            }
            if (session.bindingRetry) {
                Button(onClick = client::continueBrowserBinding, enabled = !session.busy, modifier = Modifier.fillMaxWidth()) {
                    Text(str(R.string.text_continue_internet_identity_binding_459296a))
                }
            }
            OutlinedButton(onClick = { client.login(binding = true, open = openLogin) }, enabled = manage && !session.busy, modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_bind_another_internet_identity_92200a6))
            }
        }
        DetailCard {
            Text(str(R.string.text_approve_a_binding_request_91c89d0), style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(token, { token = it }, Modifier.fillMaxWidth(), minLines = 2, label = { Text(str(R.string.text_binding_request_8d16877)) })
            Button(onClick = { client.approvals.request("login:bind") { it.reviewBinding(token.trim()) } }, enabled = manage && token.isNotBlank(), modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_review_binding_ff96c1d))
            }
        }
        ApprovalReceipt(client, "login:bind")
        if (session.session?.loginRenewable == true) Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
            ListRow(str(R.string.text_sign_out_all_devices_ad29137), icon = Icons.AutoMirrored.Outlined.Logout, destructive = true, enabled = !session.busy) { logoutAll = true }
        }
    }
    if (choose) AlertDialog(onDismissRequest = { choose = false }, title = { Text(str(R.string.text_add_a_passkey_login_4978775)) }, text = { Text(str(R.string.text_use_a_saved_passkey_or_create_a_new_one_then_review_linking_that_0850b83)) },
        confirmButton = { TextButton(onClick = { choose = false; client.passkey(PasskeyOperation.BIND_EXISTING) }) { Text(str(R.string.text_use_saved_passkey_32922fb)) } },
        dismissButton = { TextButton(onClick = { choose = false; client.passkey(PasskeyOperation.BIND) }) { Text(str(R.string.text_create_new_passkey_9a542ac)) } })
    if (logoutAll) AlertDialog(onDismissRequest = { logoutAll = false }, title = { Text(str(R.string.text_sign_out_all_devices_df1c53c)) }, text = { Text(str(R.string.text_this_passkey_stops_renewing_its_devices_existing_authorizations_c_091e795)) }, confirmButton = { TextButton(onClick = { logoutAll = false; client.passkey(PasskeyOperation.LOGOUT_ALL) }) { Text(str(R.string.text_continue_fd19155)) } }, dismissButton = { TextButton(onClick = { logoutAll = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable fun RecoveryScreen(client: ClientController, expectedDigest: String? = null) {
    val session by client.state.collectAsStateWithLifecycle()
    val recovery by client.account.recovery.state.collectAsStateWithLifecycle()
    var now by remember { mutableStateOf(System.currentTimeMillis().toULong()) }
    var confirm by remember { mutableStateOf(false) }
    val value = recovery.valueOrNull()
    LaunchedEffect(session.epoch) { client.account.recovery() }
    LaunchedEffect(Unit) { while (true) { delay(1000); now = System.currentTimeMillis().toULong() } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(recovery, client.account::recovery)
        Notice(
            if (value != null && !value.enabled) str(R.string.text_keep_an_authorized_device_you_can_unlock_recovering_your_login_al_88630e3)
            else str(R.string.text_recovery_requires_a_bound_login_and_an_on_chain_policy_enabled_be_a56b875),
        )
        if (expectedDigest != null && recovery is ReadState.Ready && value?.pending?.digest != expectedDigest) {
            EmptyState(str(R.string.text_this_recovery_request_is_no_longer_pending_a8f8c13), glyph = R.drawable.glyph_recovery)
        }
        value?.takeIf { expectedDigest == null || it.pending?.digest == expectedDigest }?.let {
            DetailCard {
                Text(if (it.enabled) str(R.string.text_on_chain_recovery_enabled_63f689f) else str(R.string.text_on_chain_recovery_disabled_29431b5),
                    style = MaterialTheme.typography.titleMedium, color = if (it.enabled) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface)
                LabelValue(str(R.string.text_waiting_period_75c0508).trimEnd(' ', '·', ':'), quantity(R.plurals.days, it.delay / 86_400_000uL))
                if (it.member) Text(str(R.string.text_this_device_is_authorized_5186a9f), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            it.pending?.let { pending ->
                DetailCard {
                    Text(str(R.string.text_pending_recovery_ae47425),
                        modifier = if (recovery is ReadState.Ready && pending.digest != null) Modifier.reminderVisible(client, NoticeTarget("recovery", pending.digest), revision = pending) else Modifier,
                        style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                    SelectionContainer { Text(pending.id, style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    LabelValue(str(R.string.text_waiting_period_ends_07bad56).trimEnd(' ', '·', ':'), messageTime(pending.executeAfter))
                    LabelValue(str(R.string.text_expires_d2c7887).trimEnd(' ', '·', ':'), messageTime(pending.expiresAt))
                    if (now >= pending.expiresAt) Text(str(R.string.text_expired_3d7e71e), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.error)
                    if (session.unlocked && it.member) OutlinedButton(onClick = { client.approvals.request("recovery:cancel:" + pending.id) { core -> core.disputeRecovery(pending.id) } }, modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_review_cancelling_recovery_6a26716), color = MaterialTheme.colorScheme.error)
                    }
                }
                ApprovalReceipt(client, "recovery:cancel:" + pending.id)
            }
            if (it.canRequest(now)) Button(onClick = { confirm = true }, enabled = !session.working("recovery:request"), modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_request_account_recovery_c8c08bd))
            }
            if (it.canComplete(now) && (!it.member || session.session?.state == "root_required")) Button(onClick = { client.perform({ core -> core.completeRecovery() }, transition = true) { client.account.recovery() } }, enabled = !session.busy, modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_complete_recovery_7f77126))
            }
        }
        if (session.unlocked) Section {
            ListRow(str(R.string.text_on_chain_recovery_policy_2a130d0), glyph = R.drawable.glyph_recovery) { client.navigation.open("recovery-policy") }
        }
    }
    if (confirm) AlertDialog(onDismissRequest = { confirm = false }, title = { Text(str(R.string.text_start_account_recovery_0b7ddb1)) }, text = { Text(str(R.string.text_authorized_devices_can_cancel_the_request_during_the_waiting_peri_7e54d5f)) }, confirmButton = { TextButton(onClick = { confirm = false; client.perform({ it.requestRecovery() }, key = "recovery:request") { client.account.recovery() } }) { Text(str(R.string.text_request_recovery_9ec2322)) } }, dismissButton = { TextButton(onClick = { confirm = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable internal fun ApprovalReceipt(client: ClientController, key: String) {
    val receipts by client.approvals.receipts.collectAsStateWithLifecycle()
    receipts[key]?.let { id ->
        Section {
            ListRow(
                headline = str(R.string.text_view_original_approval_ecd3b71),
                supporting = str(R.string.text_waiting_for_approval_45d78c0),
                glyph = R.drawable.glyph_approvals,
            ) { client.approvals.open(id) }
        }
    }
}

@Composable fun HardwareScreen(client: ClientController) {
    val session by client.state.collectAsStateWithLifecycle()
    val status by client.account.hardware.state.collectAsStateWithLifecycle()
    val form by client.account.hardwareDraft.collectAsStateWithLifecycle()
    val value = status.valueOrNull()
    var binding by remember { mutableStateOf<Boolean?>(null) }
    var enrollment by remember { mutableStateOf<JsonObject?>(null) }
    val controls = setOf("account_control", "device_control", "principal_control")
    LaunchedEffect(session.epoch) { client.account.hardware(); client.approvals.settle() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        val banner = when {
            value?.boundAvailable == false -> str(R.string.text_the_binding_key_is_unavailable_keep_this_workspace_and_review_rec_9f02498) to true
            value?.key?.changed == true -> str(R.string.text_biometric_enrollment_changed_review_replacing_this_key_a3ffff1) to true
            value != null && !value.available -> str(R.string.text_hardware_approval_is_unavailable_on_this_device_be15b10) to false
            else -> str(R.string.text_quick_unlock_opens_local_content_device_binding_restricts_which_d_5f0009f) to false
        }
        Notice(banner.first, banner.second)
        ReadFeedback(status, client.account::hardware)
        if (value != null) {
            Section(title = str(R.string.text_this_device_ce825c6)) {
                InfoRow(str(R.string.text_hardware_security_04e42f4),
                    if (value.key?.here == true) str(R.string.text_approval_key_registered_here_08e9f7b) else str(R.string.text_no_local_registered_approval_key_2f482d1))
                value.key?.let { key -> CodeRow(str(R.string.pair_key_fingerprint), key.id) { client.clipboard.copy(key.id) } }
                SettingToggle(str(R.string.text_biometrics_only_2dc6cc9), form.biometryOnly, { client.account.editHardware(form.copy(biometryOnly = it)) }, enabled = value.biometry, description = str(R.string.text_no_device_pin_fallback_changing_enrolled_biometrics_may_invalidat_196305d), icon = Icons.Outlined.Fingerprint)
            }
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                if ("account_control" !in value.policy || value.key?.here == true && value.key.changed != true) {
                    Button(onClick = { client.approvals.request("hardware:key") { it.registerApprovalKey(form.biometryOnly) } }, enabled = value.available && !session.working("approval:request:hardware:key"), modifier = Modifier.fillMaxWidth()) {
                        Text(if (value.key?.here == true) str(R.string.text_review_replacement_key_e3e0fd1) else str(R.string.text_review_key_registration_0ac05aa))
                    }
                }
                OutlinedButton(onClick = { client.perform({ it.approvalKeyRequest(form.biometryOnly).decode().objectValue() }, key = "hardware:enrollment") { enrollment = it } }, enabled = value.available && !session.working("hardware:enrollment"), modifier = Modifier.fillMaxWidth()) {
                    Text(str(R.string.text_create_administrator_enrollment_request_42868af))
                }
            }
            ApprovalReceipt(client, "hardware:key")
            enrollment?.let { key -> DeviceRequestCode(client, key.required("token"), key.required("device_id"), key.required("key_id")) }
            if (value.key != null) Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                ListRow(str(R.string.text_review_retiring_this_key_88ea641), icon = Icons.Outlined.KeyOff, destructive = true) {
                    client.approvals.request("hardware:key") { it.retireApprovalKey(null) }
                }
            }
            Section(
                title = str(R.string.text_device_bound_workspace_170f7b1),
                footer = str(R.string.text_keep_another_authorized_device_or_a_recovery_path_before_changing_eef945b),
            ) {
                SettingToggle(
                    title = str(R.string.text_require_this_device_to_unlock_c7b4d5f),
                    checked = value.bound,
                    change = { binding = it },
                    enabled = value.available && !session.working("hardware:binding"),
                    description = if (value.bound) str(if (value.boundBiometryOnly == true) R.string.text_biometrics_only_2dc6cc9 else R.string.text_system_authentication_0802d32) else null,
                    icon = Icons.Outlined.Security,
                )
            }
            Section(title = str(R.string.text_require_hardware_approval_cdb4c98)) {
                SettingToggle(str(R.string.text_account_devices_and_identities_84f8f44), form.policy.any { it in controls }, { enabled -> client.account.editHardware(form.copy(policy = if (enabled) form.policy + controls else emptySet())) }, enabled = value.canManage)
                listOf("payment" to str(R.string.text_payments_01597be), "attestation" to str(R.string.text_attestations_9a07a08)).forEach { (purpose, title) ->
                    SettingToggle(title, purpose in form.policy, { enabled -> client.account.editHardware(form.copy(policy = if (enabled) form.policy + purpose else form.policy - purpose)) }, enabled = value.canManage && form.policy.any { it in controls })
                }
            }
            Button(onClick = { client.approvals.request("hardware:policy") { it.setHardwarePolicy(form.policy.sorted()) } }, enabled = value.canManage && form.policy != value.policy.toSet(), modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_review_policy_changes_2336066))
            }
            ApprovalReceipt(client, "hardware:policy")
            SectionTitle(str(R.string.text_authorized_devices_7cf82c6))
            value.devices.forEach { device ->
                DetailCard {
                    Text(if (device.current) str(R.string.text_this_device_ce825c6) else str(R.string.text_another_device_064e2a5), style = MaterialTheme.typography.titleSmall, color = if (device.current) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface)
                    SelectionContainer { Text(device.id, style = MaterialTheme.typography.bodySmall.code()) }
                    device.key?.let { key ->
                        SelectionContainer { Text(key, style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        if (!device.current && value.canManage) TextButton(onClick = { client.approvals.request("hardware:key:" + device.id) { it.retireApprovalKey(device.id) } }) {
                            Text(str(R.string.text_review_retiring_this_key_88ea641), color = MaterialTheme.colorScheme.error)
                        }
                    }
                }
                ApprovalReceipt(client, "hardware:key:" + device.id)
            }
            if (value.canManage) {
                Section(title = str(R.string.text_review_an_enrollment_request_65ae501)) {
                    ListRow(str(R.string.text_scan_device_request_c50056e), icon = Icons.Outlined.QrCodeScanner) { client.navigation.open("key-enrollment") }
                }
            }
        }
    }
    binding?.let { enabled -> AlertDialog(onDismissRequest = { binding = null }, title = { Text(if (enabled) str(R.string.text_bind_this_workspace_to_this_device_fbab30d) else str(R.string.text_remove_device_binding_a43e7dd)) }, text = { Text(str(R.string.text_this_changes_local_unlock_protection_it_does_not_change_recovery_869ba15)) }, confirmButton = { TextButton(onClick = {
        binding = null; client.perform({ it.setDeviceBound(enabled, form.biometryOnly) }, key = "hardware:binding") { client.account.hardware() }
    }) { Text(str(R.string.text_continue_fd19155)) } }, dismissButton = { TextButton(onClick = { binding = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}

@Composable fun RecoveryPolicyScreen(client: ClientController) {
    val security by client.account.security.state.collectAsStateWithLifecycle()
    val devices by client.account.devices.state.collectAsStateWithLifecycle()
    val form by client.account.recoveryDraft.collectAsStateWithLifecycle()
    val manage = devices is ReadState.Ready && devices.valueOrNull()?.canManage == true
    LaunchedEffect(Unit) { client.account.security(); client.approvals.settle() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(security, devices, client.account::security, client.account::devices)
        if (security.valueOrNull()?.recoveryUpdating == true) Notice(str(R.string.text_security_update_in_progress_new_content_waits_for_root_rotation_t_ca75cef))
        else Notice(str(R.string.text_on_chain_recovery_is_off_by_default_for_new_accounts_configure_it_ebbc51d))
        Section(footer = if (form.enabled) str(R.string.text_a_bound_login_can_recover_uploaded_content_after_the_waiting_peri_491ba41) else str(R.string.text_if_all_authorized_devices_are_lost_recovering_your_login_does_not_01fe5e2)) {
            SettingToggle(str(R.string.text_on_chain_recovery_dcf7d3f), form.enabled, { client.account.editRecovery(form.copy(enabled = it)) }, enabled = manage, icon = Icons.Outlined.Restore)
        }
        if (form.enabled) {
            DetailCard {
                LabelValue(str(R.string.text_waiting_period_75c0508).trimEnd(' ', '·', ':'), quantity(R.plurals.days, form.days))
                Slider(form.days.toFloat(), { client.account.editRecovery(form.copy(days = it.toInt().coerceIn(1, 7))) }, valueRange = 1f..7f, steps = 5, enabled = manage)
            }
        }
        Button(
            onClick = { client.approvals.request("recovery:policy") { it.changeRecoveryPolicy(form.enabled, form.days.toUInt()) } },
            enabled = manage && security.valueOrNull()?.let { form.enabled != it.recoveryEnabled || form.days.toULong() * 86_400_000uL != it.recoveryDelayMs } == true,
            modifier = Modifier.fillMaxWidth(),
        ) { Text(str(R.string.text_review_request_cb6fb73)) }
        ApprovalReceipt(client, "recovery:policy")
    }
}

@Composable fun DevicesScreen(client: ClientController) {
    val devices by client.account.devices.state.collectAsStateWithLifecycle()
    val session by client.state.collectAsStateWithLifecycle()
    var selected by remember { mutableStateOf<String?>(null) }
    val value = devices.valueOrNull()
    LaunchedEffect(session.epoch) { client.account.devices(); client.approvals.settle() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        if (value != null && value.rootState != "Ready") Notice(str(R.string.text_content_root_state_71fd133) + value.rootState)
        ReadFeedback(devices, client.account::devices)
        if (selected == null) {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Button(onClick = { client.navigation.open("account-invite") }, enabled = value?.canManage == true, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.Add, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_authorize_another_device_d67d951))
                }
                OutlinedButton(onClick = { client.navigation.open("pair-approve") }, enabled = value != null, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.QrCodeScanner, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_scan_device_request_c50056e))
                }
            }
            Section {
                ListRow(str(R.string.text_transfer_between_devices_fc09783), icon = Icons.Outlined.SyncAlt) { client.navigation.push(Route("device-transfer")) }
                ListRow(str(R.string.text_replace_a_device_fbe5457), icon = Icons.Outlined.PhonelinkSetup) { client.navigation.push(Route("handovers")) }
            }
            val allDevices = value?.devices.orEmpty()
            if (allDevices.isNotEmpty()) Section(title = str(R.string.text_authorized_devices_7cf82c6)) {
                allDevices.forEach { device ->
                    ListRow(
                        headline = if (device.current) str(R.string.text_this_android_device_7a65c2a) else device.id,
                        supporting = device.role + if (device.revokedAt != null) " · " + str(R.string.text_revoked_c4fbad3) else "",
                        glyph = R.drawable.glyph_device,
                    ) { selected = device.id }
                }
            }
        } else {
            val device = value?.devices?.find { it.id == selected }
            if (device != null) {
                OutlinedButton(onClick = { selected = null }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_all_devices_a5aa921)) }
                Section {
                    CodeRow(str(R.string.text_device_id_b31993b), device.id) { client.clipboard.copy(device.id) }
                    ListRow(str(R.string.text_role_b9e0249), value = device.role)
                    ListRow(str(R.string.text_added_e8b264d).trimEnd(' ', '·', ':'), value = messageTime(device.addedAt))
                    device.addedBy?.let { CodeRow(str(R.string.text_approved_by_808eaa3).trimEnd(' ', '·', ':'), it) { client.clipboard.copy(it) } }
                    device.revokedAt?.let { ListRow(str(R.string.text_revoked_bc16396).trimEnd(' ', '·', ':'), value = messageTime(it)) }
                }
                CopyButton(str(R.string.text_copy_device_id_70ef1a5), outlined = true) { client.clipboard.copy(device.id) }
                if (device.capabilities.isNotEmpty()) Section(title = str(R.string.text_permissions_be9338a)) {
                    device.capabilities.forEach { cap -> ListRow(cap, icon = Icons.Outlined.VerifiedUser) }
                }
                if (device.revokedAt == null) Section(
                    footer = str(R.string.text_revocation_stops_new_access_after_state_refresh_and_key_rotation_78b114d),
                    iconColor = MaterialTheme.colorScheme.errorContainer,
                ) {
                    ListRow(str(R.string.text_review_revoking_this_device_6fa4e7e), icon = Icons.Outlined.RemoveCircleOutline, destructive = true, enabled = value.canManage) {
                        client.approvals.request("device:revoke:" + device.id) { it.revokeDevice(device.id) }
                    }
                }
                ApprovalReceipt(client, "device:revoke:" + device.id)
            }
        }
    }
}
