package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.approvalTitle
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material.icons.automirrored.outlined.ArrowForward
import androidx.compose.material.icons.automirrored.outlined.FactCheck
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.serialization.json.*
import net.dmsg.android.core.*
import net.dmsg.core.*

@Composable fun AccountScreen(client: ClientController, state: UiState, go: (String) -> Unit, browser: (String) -> Unit) {
    var choosingLanguage by remember { mutableStateOf(false) }
    if (choosingLanguage) LanguagePicker { choosingLanguage = false }
    Column(Modifier.fillMaxSize().verticalScroll(workspaceScrollState(client.navigation, "account:index")).padding(20.dp)) {
        AccountIdentityHeader(client, browser)
        SectionTitle(str(R.string.text_identity_and_security_b4f1b0c))
        AccountRow(str(R.string.text_approvals_1b5e94c), Icons.AutoMirrored.Outlined.FactCheck,
            state.session?.approvalsWaiting?.let(::localizedNumber) ?: "—") { go("approvals") }
        AccountRow(str(R.string.text_account_security_41c2886), Icons.Outlined.Security) { go("security") }
        AccountRow(str(R.string.text_devices_bebe058), Icons.Outlined.Devices) { go("devices") }
        AccountRow(str(R.string.text_switch_account_52f71df), Icons.Outlined.SwitchAccount) { go("accounts") }
        SectionTitle(str(R.string.text_sync_and_storage_6513532))
        AccountRow(str(R.string.text_account_synchronization_77ccb72), Icons.Outlined.Sync) { go("sync") }
        AccountRow(str(R.string.text_plan_and_usage_acf1a9a), Icons.Outlined.Verified) { go("membership") }
        AccountRow(str(R.string.text_system_integrations_9ea81ce), Icons.Outlined.Layers) { go("system-integrations") }
        SectionTitle(str(R.string.text_settings_8d4d0d8))
        AccountRow(str(R.string.text_send_feedback_73e1781), Icons.Outlined.Feedback) { go("feedback") }
        AccountRow(str(R.string.text_notifications_3e7769a), Icons.Outlined.NotificationsNone) { go("notifications") }
        AccountRow(str(R.string.app_language), Icons.Outlined.Translate) { choosingLanguage = true }
        AccountRow(str(R.string.text_advanced), Icons.Outlined.Tune) { go("advanced") }
        AccountRow(str(R.string.text_about_dmsg_630408f), Icons.Outlined.Info) { go("about") }
        HorizontalDivider()
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            var signingOut by remember { mutableStateOf(false) }
            TextButton(onClick = { signingOut = true }) { Text(str(R.string.text_sign_out_7bfa045), color = MaterialTheme.colorScheme.error) }
            if (signingOut) AlertDialog(onDismissRequest = { signingOut = false },
                title = { Text(str(R.string.text_sign_out_of_this_account_c08b5b1)) },
                text = { Text(str(R.string.text_encrypted_content_stays_on_this_device_saved_login_and_quick_unlo_40bc13b)) },
                confirmButton = { TextButton(onClick = { signingOut = false; client.logout() }) { Text(str(R.string.text_sign_out_7bfa045)) } },
                dismissButton = { TextButton(onClick = { signingOut = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
        }
    }
}

@Composable internal fun AccountRow(label: String, icon: ImageVector? = null, value: String? = null, click: () -> Unit) {
    ListItem(headlineContent = { Text(label) },
        leadingContent = icon?.let { glyph -> { Icon(glyph, null, tint = MaterialTheme.colorScheme.primary) } },
        trailingContent = { Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            value?.let { Text(it, style = MaterialTheme.typography.labelLarge) }
            Icon(Icons.AutoMirrored.Outlined.ArrowForward, null)
        } },
        modifier = Modifier.clickable(role = Role.Button, onClick = click).heightIn(min = 56.dp))
}

@Composable fun SystemIntegrationsScreen(client: ClientController) {
    Column(Modifier.fillMaxSize().verticalScroll(workspaceScrollState(client.navigation, "account:system-integrations")).padding(20.dp)) {
        AccountRow(str(R.string.text_password_autofill_872af78), Icons.Outlined.Key) { client.navigation.push(Route("system-autofill")) }
        AccountRow(str(R.string.text_files_app_copies_96ebae6), Icons.Outlined.Folder) { client.navigation.push(Route("system-files")) }
        AccountRow(str(R.string.text_shared_with_dmsg_0df2fa8), Icons.Outlined.Share) { client.navigation.push(Route("incoming-shares")) }
        AccountRow(str(R.string.text_home_screen_entries_7e5f087), Icons.Outlined.Widgets) { client.navigation.push(Route("workspace-entries")) }
    }
}

@Composable fun AdvancedAccountScreen(client: ClientController) {
    Column(Modifier.fillMaxSize().verticalScroll(workspaceScrollState(client.navigation, "account:advanced")).padding(20.dp)) {
        AccountRow(str(R.string.text_agents_dcfd6ed), Icons.Outlined.SmartToy) { client.navigation.push(Route("agents")) }
        AccountRow(str(R.string.text_multisig_wallets_451404c), Icons.Outlined.AccountBalanceWallet) { client.navigation.push(Route("wallets")) }
        AccountRow(str(R.string.text_connected_browsers_e404840), Icons.Outlined.Language) { client.navigation.push(Route("browser-connections")) }
        AccountRow(str(R.string.text_hardware_security_04e42f4), Icons.Outlined.Memory) { client.navigation.push(Route("hardware")) }
        AccountRow(str(R.string.text_activity_on_this_device_288740a), Icons.Outlined.History) { client.navigation.push(Route("secret-activity")) }
        AccountRow(str(R.string.text_cached_data_b29fb6f), Icons.Outlined.Storage) { client.navigation.push(Route("display-cache")) }
    }
}

@Composable fun SecurityScreen(client: ClientController, state: UiState) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp)) {
        Text(str(R.string.text_security_and_unlock_7ba00a7), style = MaterialTheme.typography.headlineSmall)
        SettingToggle(str(R.string.text_quick_unlock_bdfe73d), checked = state.session?.fastUnlock == true, change = client::fastUnlock, enabled = !state.busy, description = str(R.string.text_local_unlock_is_independent_of_online_sign_in_expiry_5c2417d))
        SettingToggle(str(R.string.text_lock_immediately_in_background_67ea031), state.immediateLock, client::immediateLock)
        SectionTitle(str(R.string.text_appearance_7bcd380))
        listOf(null to str(R.string.text_system_b9dd0f7), false to str(R.string.text_light_f3bfabb), true to str(R.string.text_dark_0bea023)).forEach { (value, label) ->
            RadioChoiceRow(label, state.dark == value) { if (value == null) client.systemAppearance() else client.dark(value) }
        }
        AccountRow(str(R.string.text_hardware_security_04e42f4)) { client.navigation.open("hardware") }
        AccountRow(str(R.string.text_login_identities_09c2f23)) { client.navigation.open("bindings") }
        AccountRow(str(R.string.text_account_recovery_a3a426c)) { client.navigation.open("recovery") }
        AccountRow(str(R.string.text_message_settings_1a28455)) { client.navigation.push(Route("message-settings")) }
        SectionTitle(str(R.string.text_account_4ab56e9))
        AccountRow(str(R.string.text_remove_local_data_c45125e)) { client.navigation.push(Route("account-remove")) }
        AccountRow(str(R.string.text_delete_service_account_1475636)) { client.navigation.push(Route("account-delete")) }
    }
}

@Composable fun ApprovalsScreen(client: ClientController, state: UiState, selected: String, select: (String) -> Unit) {
    val listing by client.approvals.listing.state.collectAsStateWithLifecycle()
    val detail by client.approvals.detail.state.collectAsStateWithLifecycle()
    var now by remember { mutableStateOf(System.currentTimeMillis().toULong()) }
    var uptime by remember { mutableLongStateOf(android.os.SystemClock.elapsedRealtime()) }
    var readyAt by remember(selected, state.epoch) { mutableLongStateOf(Long.MAX_VALUE) }
    var decision by remember(selected, state.epoch) { mutableStateOf<Pair<ApprovalDetail, Boolean>?>(null) }
    var technical by remember(selected) { mutableStateOf(false) }
    val current = detail.valueOrNull()?.takeIf { it.id == selected }
    LaunchedEffect(selected, state.epoch) { if (selected.isEmpty()) client.approvals.refreshRemote() else client.approvals.select(selected) }
    LaunchedEffect(current?.digest) { readyAt = if (current == null) Long.MAX_VALUE else android.os.SystemClock.elapsedRealtime() + 1000 }
    LaunchedEffect(Unit) { while (true) { now = System.currentTimeMillis().toULong(); uptime = android.os.SystemClock.elapsedRealtime(); kotlinx.coroutines.delay(1000) } }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (selected.isEmpty()) {
                ReadFeedback(listing, client.approvals::refreshRemote)
                Notice(str(R.string.text_desktop_local_requests_stay_on_that_desktop_only_requests_availab_7e3389e))
                if (listing is ReadState.Empty) EmptyPanel(str(R.string.text_no_pending_approvals_58ba7ee))
                listing.valueOrNull().orEmpty().forEach { entry ->
                    ListItem(headlineContent = { Text(approvalTitle(entry.kind)) }, supportingContent = { Column { Text(entry.source); Text(secretStateName(entry.state)); Text(str(R.string.text_expires_1efbbe3) + messageTime(entry.expiresAt)) } }, modifier = Modifier.clickable { select(entry.id) })
                }
            } else {
                ReadFeedback(detail, { client.approvals.select(selected) })
                current?.let { value ->
                    Text(approvalTitle(value.kind), modifier = if (detail is ReadState.Ready) Modifier.reminderVisible(client, NoticeTarget("approval", value.id), revision = value) else Modifier, style = MaterialTheme.typography.headlineSmall)
                    SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { Text(state.session?.account.orEmpty()); Text(value.source)  } }
                    Text(if (value.risk == "high") str(R.string.text_review_this_sensitive_action_carefully_9a6034a) else str(R.string.text_review_the_complete_request_9d9ec00), style = MaterialTheme.typography.titleSmall)
                    Text(str(R.string.text_expires_d2c7887) + messageTime(value.expiresAt))
                    if (value.kind == "device_pair") Notice(str(R.string.text_this_device_may_receive_your_private_content_approval_must_be_fol_6938488))
                    if (value.kind == "connect") Notice(str(R.string.text_the_source_is_relayed_by_the_browser_through_connect_dmsg_net_a_b_8ce6a30))
                    val body = value.detail as? JsonObject
                    val document = value.kind == "attest" || value.kind == "connect" && body?.text("method") == "signDocument"
                    if (document) Notice(if (body?.optionalFlag("file_checked") == true) str(R.string.text_the_original_file_was_checked_against_this_digest_c5ffd5d) else str(R.string.text_only_the_digest_is_being_reviewed_the_original_file_has_not_been_241cfe0))
                    if (value.kind == "account" && body?.text("change") == "recovery_policy") Notice(if (body.flag("enabled")) str(R.string.text_a_bound_login_can_recover_uploaded_content_after_the_waiting_peri_31381f9) else str(R.string.text_keep_an_authorized_device_you_can_unlock_recovering_a_login_alone_11b8c07))
                    if (value.secret != null) SecretApprovalContent(value.secret) else HumanRecord(value.detail)
                    SectionTitle(str(R.string.text_request_state_e7453d3))
                    Text(secretStateName(value.state))
                    value.error?.let { Notice(errorText(it), true) }
                    value.record["result"]?.takeIf { it != JsonNull }?.let { SectionTitle(str(R.string.text_result_d6b21bc)); HumanRecord(it) }
                    value.record["delivery_report"]?.takeIf { it != JsonNull }?.let { SectionTitle(str(R.string.text_delivery_report_29f483d)); HumanRecord(it) }
                    TextButton(onClick = { technical = !technical }) { Text(str(R.string.text_technical_details_5ec1b41)) }
                    if (technical) HumanRecord(JsonObject(value.record.filterKeys { it !in setOf("detail", "result", "delivery_report") }))
                    if (!value.supported) Notice(str(R.string.text_this_action_requires_a_supported_desktop_client_85a09eb))
                    if (now >= value.expiresAt && value.state == "awaiting_user") Notice(str(R.string.text_this_request_expired_refresh_before_continuing_04ccc12))
                    if (value.state in setOf("executing", "unknown")) {
                        Notice(str(R.string.text_the_result_is_not_confirmed_check_the_original_operation_before_r_57342fb))
                        OutlinedButton(onClick = { client.approvals.reconcile(value) }, enabled = !state.working("approval:reconcile:" + value.id)) { Text(str(R.string.text_reconcile_original_operation_065467d)) }
                    }
                }
            }
        }
        current?.takeIf { it.state == "awaiting_user" }?.let { value ->
            Surface {
                Column(Modifier.fillMaxWidth().padding(16.dp)) {
                    Text(str(R.string.text_your_decision_applies_only_to_this_request_a7693fb), style = MaterialTheme.typography.bodySmall)
                    val actionable = now < value.expiresAt && !state.working("approval:decide:" + selected) && detail is ReadState.Ready && uptime >= readyAt
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlinedButton(enabled = actionable, onClick = { decision = value to false }, modifier = Modifier.weight(1f)) { Text(str(R.string.text_reject_52e8be4)) }
                        Button(enabled = actionable && value.supported, onClick = { decision = value to true }, modifier = Modifier.weight(1f)) { Text(str(R.string.text_approve_8d032fb)) }
                    }
                }
            }
        }
    }
    decision?.let { (value, approve) ->
        AlertDialog(onDismissRequest = { decision = null }, title = { Text(if (approve) str(R.string.text_approve_the_reviewed_request_18d90f3) else str(R.string.text_reject_this_request_24df6c9)) }, text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(approvalTitle(value.kind), style = MaterialTheme.typography.titleMedium)
                val secret = value.secret
                if (secret != null) {
                    Text(secret.title, style = MaterialTheme.typography.titleSmall)
                    val recipient = when {
                        secret.importing -> secret.destination
                        secret.channel != null -> secret.channel.name + " · " + secret.channel.id
                        secret.password == true -> str(R.string.review_anyone_with_the_link_and_password)
                        else -> str(R.string.review_anyone_with_the_link)
                    }
                    recipient?.let { LabelValue(str(R.string.text_destination_63c1289), it) }
                    if (secret.fields.isNotEmpty()) LabelValue(str(R.string.text_selected_fields_cf5d199), secret.fields.joinToString { vaultFieldTitle(it.id, it.kind, it.label) })
                    if (secret.notes) Text(str(R.string.text_notes_f7df47b))
                    if (secret.files.isNotEmpty()) LabelValue(str(R.string.text_selected_attachments_4c5e318), secret.files.joinToString { it.name })
                    secret.expiry?.let { Text(str(R.string.text_expires_1efbbe3) + displayTime(it)) }
                } else {
                    val body = value.detail as? JsonObject
                    if (value.kind == "wallet" && (body?.get("action") as? JsonPrimitive)?.contentOrNull == "create") {
                        listOf("price_e8s" to R.string.text_amount_bfc0adf, "ledger_fee_e8s" to R.string.text_fee_92d2050).forEach { (key, label) ->
                            val atomic = (body[key] as? JsonPrimitive)?.contentOrNull
                            val formatted = atomic?.let { runCatching { AtomicAmount.display(it, 8) }.getOrNull() }
                            if (formatted != null) LabelValue(str(label), formatted + " ICP")
                        }
                    }
                    HumanRecord(value.detail)
                }
                HorizontalDivider()
                SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(value.id, style = MaterialTheme.typography.bodySmall)
                    Text(value.digest, style = MaterialTheme.typography.bodySmall)
                } }
                Text(str(R.string.text_this_decision_is_bound_to_the_exact_request_and_digest_shown_0594140), style = MaterialTheme.typography.bodySmall)
            }
        }, confirmButton = { TextButton(onClick = { decision = null; client.approvals.decide(value, approve) }, enabled = current?.id == value.id && current.digest == value.digest && current.state == "awaiting_user" && detail is ReadState.Ready && now < value.expiresAt && state.unlocked) { Text(if (approve) str(R.string.text_approve_this_request_fac866f) else str(R.string.text_reject_this_request_46758a1)) } }, dismissButton = { TextButton(onClick = { decision = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
    }
}

/** Exact lexical values with readable labels. Never display a formatted JSON blob. */
@Composable internal fun HumanRecord(value: JsonElement) {
    val rows = remember(value) {
        val output = mutableListOf<Pair<String, JsonPrimitive?>>()
        fun visit(next: JsonElement, path: String) {
            when (next) {
                is JsonObject -> if (next.isEmpty()) output += path to null else next.forEach { (key, child) -> visit(child, if (path.isEmpty()) fieldLabel(key) else path + " › " + fieldLabel(key)) }
                is JsonArray -> if (next.isEmpty()) output += path to null else next.forEachIndexed { index, child -> visit(child, path + " [" + (index + 1) + "]") }
                is JsonPrimitive -> output += path to next
            }
        }
        visit(value, ""); output
    }
    rows.forEach { (path, child) ->
        Column(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
            if (path.isNotBlank()) Text(path, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            SelectionContainer { Text(if (child == null || child == JsonNull) str(R.string.text_none_2d00776) else if (child.isString && child.content.isEmpty()) str(R.string.text_empty_text_8d2743c) else child.content, style = MaterialTheme.typography.bodyMedium) }
        }
    }
}
private fun fieldLabel(key: String): String = when (key) {
    "account" -> str(R.string.text_account_4ab56e9); "device_id" -> str(R.string.text_device_id_b31993b)
    "source" -> str(R.string.text_source_fcdba0f); "expires_at", "expiresAt" -> str(R.string.text_expires_at_ms_0d84f58)
    "role" -> str(R.string.text_role_b9e0249)
    "grantee", "recipient" -> str(R.string.text_destination_63c1289)
    "capabilities" -> str(R.string.text_permissions_be9338a)
    "days" -> str(R.string.text_valid_for_days_13075ad)
    "action" -> str(R.string.text_action_91069d1)
    "wallet" -> str(R.string.text_wallet_77c08be)
    "name" -> str(R.string.text_name_dd4dc4c)
    "state" -> str(R.string.text_state_91c2d40); "digest" -> str(R.string.text_reviewed_digest_ea1e6f9)
    "type" -> str(R.string.text_action_91069d1); "detail" -> str(R.string.text_details_8b33589); "risk" -> str(R.string.text_risk_d65cf0d)
    else -> key.replace('_', ' ').replaceFirstChar { it.uppercase() }
}
