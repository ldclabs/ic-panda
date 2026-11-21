package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.quantity
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
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        ReadFeedback(security, devices, client.account::security, client.account::devices)
        if (!manage && devices is ReadState.Ready) Notice(str(R.string.text_an_administrator_device_is_required_to_change_account_security_b4649ea))
        value?.logins?.forEach { login ->
            SelectionContainer { Text(login.principal) }
            if (login.current) Text(str(R.string.text_current_login_3d84cda), style = MaterialTheme.typography.labelLarge)
            TextButton(onClick = { client.clipboard.copy(login.principal) }) { Text(str(R.string.text_copy_identity_f95ea28)) }
            TextButton(onClick = { client.approvals.request("login:remove:" + login.principal) { it.removeLogin(login.principal) } }, enabled = manage && !login.current && value.logins.size > 1) { Text(str(R.string.text_review_removing_this_login_7a4b004)) }
            HorizontalDivider()
        }
        OutlinedButton(onClick = { client.login(binding = true, open = openLogin) }, enabled = manage && !session.busy) { Text(str(R.string.text_bind_another_internet_identity_92200a6)) }
        if (session.bindingRetry) {
            Notice(str(R.string.text_the_verified_login_is_waiting_for_binding_continue_the_original_a_74d014f))
            Button(onClick = client::continueBrowserBinding, enabled = !session.busy) { Text(str(R.string.text_continue_internet_identity_binding_459296a)) }
        }
        if (session.session?.passkeyAvailable == true) {
            Button(onClick = { if (authentication.canContinue) client.continuePasskey() else choose = true }, enabled = manage && !session.busy && !authentication.working) { Text(if (authentication.canContinue) str(R.string.text_continue_adding_this_passkey_ed43cf3) else str(R.string.text_add_a_passkey_login_4978775)) }
            if (authentication.canContinue) Notice(str(R.string.text_the_passkey_is_saved_continue_this_same_attempt_without_creating_5c00d64))
        }
        if (session.session?.loginRenewable == true) TextButton(onClick = { logoutAll = true }, enabled = !session.busy) { Text(str(R.string.text_sign_out_all_devices_ad29137)) }
        SectionTitle(str(R.string.text_approve_a_binding_request_91c89d0))
        OutlinedTextField(token, { token = it }, Modifier.fillMaxWidth(), minLines = 2, label = { Text(str(R.string.text_binding_request_8d16877)) })
        Button(onClick = { client.approvals.request("login:bind") { it.reviewBinding(token.trim()) } }, enabled = manage && token.isNotBlank()) { Text(str(R.string.text_review_binding_ff96c1d)) }
        ApprovalReceipt(client, "login:bind")
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
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(recovery, client.account::recovery)
        Notice(str(R.string.text_recovery_requires_a_bound_login_and_an_on_chain_policy_enabled_be_a56b875))
        if (expectedDigest != null && recovery is ReadState.Ready && value?.pending?.digest != expectedDigest) {
            EmptyPanel(str(R.string.text_this_recovery_request_is_no_longer_pending_a8f8c13))
        }
        value?.takeIf { expectedDigest == null || it.pending?.digest == expectedDigest }?.let {
            Text(if (it.enabled) str(R.string.text_on_chain_recovery_enabled_63f689f) else str(R.string.text_on_chain_recovery_disabled_29431b5), style = MaterialTheme.typography.titleMedium)
            Text(str(R.string.text_waiting_period_75c0508) + quantity(R.plurals.days, it.delay / 86_400_000uL))
            if (it.member) Text(str(R.string.text_this_device_is_authorized_5186a9f))
            if (!it.enabled) Notice(str(R.string.text_keep_an_authorized_device_you_can_unlock_recovering_your_login_al_88630e3))
            it.pending?.let { pending ->
                Text(str(R.string.text_pending_recovery_ae47425), modifier = if (recovery is ReadState.Ready && pending.digest != null) Modifier.reminderVisible(client, NoticeTarget("recovery", pending.digest), revision = pending) else Modifier, style = MaterialTheme.typography.titleMedium)
                SelectionContainer { Text(pending.id, style = MaterialTheme.typography.bodySmall) }
                Text(str(R.string.text_waiting_period_ends_07bad56) + messageTime(pending.executeAfter))
                Text(str(R.string.text_expires_d2c7887) + messageTime(pending.expiresAt))
                if (now >= pending.expiresAt) Text(str(R.string.text_expired_3d7e71e))
                if (session.unlocked && it.member) TextButton(onClick = { client.approvals.request("recovery:cancel:" + pending.id) { core -> core.disputeRecovery(pending.id) } }) { Text(str(R.string.text_review_cancelling_recovery_6a26716)) }
                ApprovalReceipt(client, "recovery:cancel:" + pending.id)
            }
            if (it.canRequest(now)) Button(onClick = { confirm = true }, enabled = !session.working("recovery:request")) { Text(str(R.string.text_request_account_recovery_c8c08bd)) }
            if (it.canComplete(now) && (!it.member || session.session?.state == "root_required")) Button(onClick = { client.perform({ core -> core.completeRecovery() }, transition = true) { client.account.recovery() } }, enabled = !session.busy) { Text(str(R.string.text_complete_recovery_7f77126)) }
        }
        if (session.unlocked) AccountRow(str(R.string.text_on_chain_recovery_policy_2a130d0)) { client.navigation.open("recovery-policy") }
    }
    if (confirm) AlertDialog(onDismissRequest = { confirm = false }, title = { Text(str(R.string.text_start_account_recovery_0b7ddb1)) }, text = { Text(str(R.string.text_authorized_devices_can_cancel_the_request_during_the_waiting_peri_7e54d5f)) }, confirmButton = { TextButton(onClick = { confirm = false; client.perform({ it.requestRecovery() }, key = "recovery:request") { client.account.recovery() } }) { Text(str(R.string.text_request_recovery_9ec2322)) } }, dismissButton = { TextButton(onClick = { confirm = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable internal fun ApprovalReceipt(client: ClientController, key: String) {
    val receipts by client.approvals.receipts.collectAsStateWithLifecycle()
    receipts[key]?.let { id -> Column { Text(str(R.string.text_waiting_for_approval_45d78c0), style = MaterialTheme.typography.labelMedium); TextButton(onClick = { client.approvals.open(id) }) { Text(str(R.string.text_view_original_approval_ecd3b71)) } } }
}

@Composable fun HardwareScreen(client: ClientController) {
    val session by client.state.collectAsStateWithLifecycle()
    val status by client.account.hardware.state.collectAsStateWithLifecycle()
    val form by client.account.hardwareDraft.collectAsStateWithLifecycle()
    val value = status.valueOrNull()
    var binding by remember { mutableStateOf<Boolean?>(null) }
    var enrollment by remember { mutableStateOf<String?>(null) }
    val controls = setOf("account_control", "device_control", "principal_control")
    LaunchedEffect(session.epoch) { client.account.hardware(); client.approvals.settle() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Notice(str(R.string.text_quick_unlock_opens_local_content_device_binding_restricts_which_d_5f0009f))
        ReadFeedback(status, client.account::hardware)
        if (value != null) {
            SectionTitle(str(R.string.text_this_device_ce825c6))
            Text(if (value.key?.here == true) str(R.string.text_approval_key_registered_here_08e9f7b) else str(R.string.text_no_local_registered_approval_key_2f482d1))
            value.key?.let { key -> SelectionContainer { Text(key.id, style = MaterialTheme.typography.bodySmall) }; if (key.changed == true) Notice(str(R.string.text_biometric_enrollment_changed_review_replacing_this_key_a3ffff1)) }
            if (!value.available) Notice(str(R.string.text_hardware_approval_is_unavailable_on_this_device_be15b10))
            SettingToggle(str(R.string.text_biometrics_only_2dc6cc9), form.biometryOnly, { client.account.editHardware(form.copy(biometryOnly = it)) }, enabled = value.biometry, description = str(R.string.text_no_device_pin_fallback_changing_enrolled_biometrics_may_invalidat_196305d))
            Button(onClick = { client.approvals.request("hardware:key") { it.registerApprovalKey(form.biometryOnly) } }, enabled = value.available && !session.working("approval:request:hardware:key")) { Text(if (value.key?.here == true) str(R.string.text_review_replacement_key_e3e0fd1) else str(R.string.text_review_key_registration_0ac05aa)) }
            ApprovalReceipt(client, "hardware:key")
            OutlinedButton(onClick = { client.perform({ it.approvalKeyRequest(form.biometryOnly).decode().objectValue().required("token") }, key = "hardware:enrollment") { enrollment = it } }, enabled = value.available && !session.working("hardware:enrollment")) { Text(str(R.string.text_create_administrator_enrollment_request_42868af)) }
            enrollment?.let { token -> Notice(str(R.string.text_this_request_alone_does_not_register_a_key_share_it_with_an_autho_d461dfc)); TextButton(onClick = { client.clipboard.copy(token) }) { Text(str(R.string.text_copy_enrollment_request_d04fb33)) } }
            if (value.key != null) TextButton(onClick = { client.approvals.request("hardware:key") { it.retireApprovalKey(null) } }) { Text(str(R.string.text_review_retiring_this_key_88ea641)) }
            SectionTitle(str(R.string.text_device_bound_workspace_170f7b1))
            SettingToggle(str(R.string.text_require_this_device_to_unlock_c7b4d5f), value.bound, { binding = it }, enabled = value.available && !session.working("hardware:binding"))
            if (value.bound) Text(if (value.boundBiometryOnly == true) str(R.string.text_biometrics_only_2dc6cc9) else str(R.string.text_system_authentication_0802d32))
            if (value.boundAvailable == false) Notice(str(R.string.text_the_binding_key_is_unavailable_keep_this_workspace_and_review_rec_9f02498), true)
            Notice(str(R.string.text_keep_another_authorized_device_or_a_recovery_path_before_changing_eef945b))
            SectionTitle(str(R.string.text_require_hardware_approval_cdb4c98))
            SettingToggle(str(R.string.text_account_devices_and_identities_84f8f44), form.policy.any { it in controls }, { enabled -> client.account.editHardware(form.copy(policy = if (enabled) form.policy + controls else emptySet())) }, enabled = value.canManage)
            listOf("payment" to str(R.string.text_payments_01597be), "attestation" to str(R.string.text_attestations_9a07a08)).forEach { (purpose, title) ->
                SettingToggle(title, purpose in form.policy, { enabled -> client.account.editHardware(form.copy(policy = if (enabled) form.policy + purpose else form.policy - purpose)) }, enabled = value.canManage && form.policy.any { it in controls })
            }
            Button(onClick = { client.approvals.request("hardware:policy") { it.setHardwarePolicy(form.policy.sorted()) } }, enabled = value.canManage && form.policy != value.policy.toSet()) { Text(str(R.string.text_review_policy_changes_2336066)) }
            ApprovalReceipt(client, "hardware:policy")
            SectionTitle(str(R.string.text_authorized_devices_7cf82c6))
            value.devices.forEach { device ->
                Text(if (device.current) str(R.string.text_this_device_ce825c6) else str(R.string.text_another_device_064e2a5), style = MaterialTheme.typography.labelLarge)
                SelectionContainer { Text(device.id, style = MaterialTheme.typography.bodySmall) }
                device.key?.let { key -> SelectionContainer { Text(key, style = MaterialTheme.typography.bodySmall) }
                    if (!device.current && value.canManage) TextButton(onClick = { client.approvals.request("hardware:key:" + device.id) { it.retireApprovalKey(device.id) } }) { Text(str(R.string.text_review_retiring_this_key_88ea641)) }
                }
                ApprovalReceipt(client, "hardware:key:" + device.id)
                HorizontalDivider()
            }
            if (value.canManage) {
                SectionTitle(str(R.string.text_review_an_enrollment_request_65ae501))
                OutlinedTextField(form.token, { client.account.editHardware(form.copy(token = it)) }, Modifier.fillMaxWidth(), minLines = 2, label = { Text(str(R.string.text_enrollment_request_7039197)) })
                OutlinedTextField(form.device, { client.account.editHardware(form.copy(device = it.trim())) }, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_device_id_b31993b)) })
                Button(onClick = { client.approvals.request("hardware:enroll:" + form.device) { it.approveApprovalKey(form.token.trim(), form.device) } }, enabled = form.token.isNotBlank() && form.device.matches(Regex("[0-9a-f]{64}"))) { Text(str(R.string.text_review_key_enrollment_4a53b87)) }
                ApprovalReceipt(client, "hardware:enroll:" + form.device)
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
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(security, devices, client.account::security, client.account::devices)
        Notice(str(R.string.text_on_chain_recovery_is_off_by_default_for_new_accounts_configure_it_ebbc51d))
        SettingToggle(str(R.string.text_on_chain_recovery_dcf7d3f), form.enabled, { client.account.editRecovery(form.copy(enabled = it)) }, enabled = manage)
        Notice(if (form.enabled) str(R.string.text_a_bound_login_can_recover_uploaded_content_after_the_waiting_peri_491ba41) else str(R.string.text_if_all_authorized_devices_are_lost_recovering_your_login_does_not_01fe5e2))
        if (form.enabled) {
            Text(str(R.string.text_waiting_period_75c0508) + quantity(R.plurals.days, form.days))
            Slider(form.days.toFloat(), { client.account.editRecovery(form.copy(days = it.toInt().coerceIn(1, 7))) }, valueRange = 1f..7f, steps = 5, enabled = manage)
        }
        if (security.valueOrNull()?.recoveryUpdating == true) Notice(str(R.string.text_security_update_in_progress_new_content_waits_for_root_rotation_t_ca75cef))
        Button(onClick = { client.approvals.request("recovery:policy") { it.changeRecoveryPolicy(form.enabled, form.days.toUInt()) } }, enabled = manage && security.valueOrNull()?.let { form.enabled != it.recoveryEnabled || form.days.toULong() * 86_400_000uL != it.recoveryDelayMs } == true) { Text(str(R.string.text_review_request_cb6fb73)) }
        ApprovalReceipt(client, "recovery:policy")
    }
}

@Composable fun DevicesScreen(client: ClientController) {
    val devices by client.account.devices.state.collectAsStateWithLifecycle()
    val session by client.state.collectAsStateWithLifecycle()
    var selected by remember { mutableStateOf<String?>(null) }
    val value = devices.valueOrNull()
    LaunchedEffect(session.epoch) { client.account.devices(); client.approvals.settle() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        AccountRow(str(R.string.text_transfer_between_devices_fc09783)) { client.navigation.push(Route("device-transfer")) }
        AccountRow(str(R.string.text_replace_a_device_fbe5457)) { client.navigation.push(Route("handovers")) }
        ReadFeedback(devices, client.account::devices)
        if (selected == null) Button(onClick = { client.navigation.open("pair-approve") }, enabled = value?.canManage == true) { Text(str(R.string.text_authorize_another_device_d67d951)) }
        value?.devices?.filter { selected == null || it.id == selected }?.forEach { device ->
            if (selected == null) AccountRow(if (device.current) str(R.string.text_this_android_device_7a65c2a) else device.id) { selected = device.id }
            else {
                TextButton(onClick = { selected = null }) { Text(str(R.string.text_all_devices_a5aa921)) }
                SelectionContainer { Text(device.id) }
                TextButton(onClick = { client.clipboard.copy(device.id) }) { Text(str(R.string.text_copy_device_id_70ef1a5)) }
                Text(device.role); Text(str(R.string.text_added_e8b264d) + messageTime(device.addedAt))
                device.addedBy?.let { Text(str(R.string.text_approved_by_808eaa3) + it, style = MaterialTheme.typography.bodySmall) }
                device.revokedAt?.let { Text(str(R.string.text_revoked_bc16396) + messageTime(it), color = MaterialTheme.colorScheme.error) }
                SectionTitle(str(R.string.text_permissions_be9338a)); device.capabilities.forEach { Text(it) }
                if (device.revokedAt == null) TextButton(onClick = { client.approvals.request("device:revoke:" + device.id) { it.revokeDevice(device.id) } }, enabled = value.canManage) { Text(str(R.string.text_review_revoking_this_device_6fa4e7e), color = MaterialTheme.colorScheme.error) }
                ApprovalReceipt(client, "device:revoke:" + device.id)
                Notice(str(R.string.text_revocation_stops_new_access_after_state_refresh_and_key_rotation_78b114d))
            }
        }
        if (value != null && value.rootState != "Ready") Notice(str(R.string.text_content_root_state_71fd133) + value.rootState)
    }
}
