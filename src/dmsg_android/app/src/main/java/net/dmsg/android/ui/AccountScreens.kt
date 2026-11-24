package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
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
import androidx.compose.material.icons.automirrored.outlined.Logout
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.serialization.json.*
import net.dmsg.android.core.*
import net.dmsg.core.*

@Composable fun AccountScreen(client: ClientController, state: UiState, go: (String) -> Unit, openLogin: (String) -> Unit) {
    var signingOut by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().verticalScroll(workspaceScrollState(client.navigation, "account:index")).padding(pagePadding())) {
        AccountIdentityHeader(client, openLogin)
        SectionTitle(str(R.string.text_identity_and_security_b4f1b0c))
        Section {
            val waiting = state.session?.approvalsWaiting
            ListRow(str(R.string.text_approvals_1b5e94c), glyph = R.drawable.glyph_approvals,
                trailing = { CountBadge((waiting ?: 0u).toLong()) { count -> if (count > 99) "99+" else localizedNumber(count) } }) { go("approvals") }
            ListRow(str(R.string.text_account_security_41c2886), glyph = R.drawable.glyph_security) { go("security") }
            ListRow(str(R.string.text_devices_bebe058), glyph = R.drawable.glyph_device) { go("devices") }
            ListRow(str(R.string.text_switch_account_52f71df), glyph = R.drawable.glyph_account) { go("accounts") }
        }
        SectionTitle(str(R.string.text_sync_and_storage_6513532))
        Section(iconColor = MaterialTheme.colorScheme.secondaryContainer) {
            ListRow(str(R.string.text_account_synchronization_77ccb72), glyph = R.drawable.glyph_sync) { go("sync") }
            ListRow(str(R.string.text_plan_and_usage_acf1a9a), glyph = R.drawable.glyph_plan) { go("membership") }
            ListRow(str(R.string.text_system_integrations_9ea81ce), icon = Icons.Outlined.Layers) { go("system-integrations") }
        }
        SectionTitle(str(R.string.text_appearance_7bcd380))
        Section(iconColor = MaterialTheme.colorScheme.tertiaryContainer) {
            AppearanceRow(client, state)
            SettingToggle(str(R.string.dynamic_color), state.dynamicColor, client::dynamicColor, description = str(R.string.dynamic_color_detail), icon = Icons.Outlined.Palette)
            AccountLanguageRow()
        }
        SectionTitle(str(R.string.text_settings_8d4d0d8))
        Section(iconColor = MaterialTheme.colorScheme.tertiaryContainer) {
            ListRow(str(R.string.text_send_feedback_73e1781), icon = Icons.Outlined.Feedback) { go("feedback") }
            ListRow(str(R.string.text_notifications_3e7769a), icon = Icons.Outlined.NotificationsNone) { go("notifications") }
            ListRow(str(R.string.text_advanced), icon = Icons.Outlined.Tune) { go("advanced") }
            ListRow(str(R.string.text_about_dmsg_630408f), icon = Icons.Outlined.Info) { go("about") }
        }
        Section(Modifier.padding(top = 24.dp), iconColor = MaterialTheme.colorScheme.errorContainer) {
            ListRow(str(R.string.text_sign_out_7bfa045), icon = Icons.AutoMirrored.Outlined.Logout, destructive = true) { signingOut = true }
        }
    }
    if (signingOut) AlertDialog(onDismissRequest = { signingOut = false },
        title = { Text(str(R.string.text_sign_out_of_this_account_c08b5b1)) },
        text = { Text(str(R.string.text_encrypted_content_stays_on_this_device_saved_login_and_quick_unlo_40bc13b)) },
        confirmButton = { TextButton(onClick = { signingOut = false; client.logout() }) { Text(str(R.string.text_sign_out_7bfa045), color = MaterialTheme.colorScheme.error) } },
        dismissButton = { TextButton(onClick = { signingOut = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

/** The app theme as one segmented control: following the system, light or dark. */
@Composable private fun AppearanceRow(client: ClientController, state: UiState) {
    val choices = listOf(null, false, true)
    ListItem(
        leadingContent = { RowIcon(Icons.Outlined.Contrast) },
        headlineContent = {
            SegmentedChoice(listOf(str(R.string.text_system_b9dd0f7), str(R.string.text_light_f3bfabb), str(R.string.text_dark_0bea023)), choices.indexOf(state.dark)) { index ->
                choices[index]?.let(client::dark) ?: client.systemAppearance()
            }
        },
        modifier = Modifier.clipTo(rowShape()),
    )
}

@Composable fun SystemIntegrationsScreen(client: ClientController) {
    Column(Modifier.fillMaxSize().verticalScroll(workspaceScrollState(client.navigation, "account:system-integrations")).padding(pagePadding())) {
        Section {
            ListRow(str(R.string.text_password_autofill_872af78), icon = Icons.Outlined.Key) { client.navigation.push(Route("system-autofill")) }
            ListRow(str(R.string.text_files_app_copies_96ebae6), icon = Icons.Outlined.Folder) { client.navigation.push(Route("system-files")) }
            ListRow(str(R.string.text_shared_with_dmsg_0df2fa8), icon = Icons.Outlined.Share) { client.navigation.push(Route("incoming-shares")) }
            ListRow(str(R.string.text_home_screen_entries_7e5f087), icon = Icons.Outlined.Widgets) { client.navigation.push(Route("workspace-entries")) }
        }
    }
}

@Composable fun AdvancedAccountScreen(client: ClientController) {
    Column(Modifier.fillMaxSize().verticalScroll(workspaceScrollState(client.navigation, "account:advanced")).padding(pagePadding())) {
        Section {
            ListRow(str(R.string.text_agents_dcfd6ed), glyph = R.drawable.glyph_agents) { client.navigation.push(Route("agents")) }
            ListRow(str(R.string.text_multisig_wallets_451404c), glyph = R.drawable.glyph_coins) { client.navigation.push(Route("wallets")) }
            ListRow(str(R.string.text_connected_browsers_e404840), glyph = R.drawable.glyph_browser) { client.navigation.push(Route("browser-connections")) }
            ListRow(str(R.string.text_hardware_security_04e42f4), icon = Icons.Outlined.Memory) { client.navigation.push(Route("hardware")) }
            ListRow(str(R.string.text_activity_on_this_device_288740a), glyph = R.drawable.glyph_audit) { client.navigation.push(Route("secret-activity")) }
            ListRow(str(R.string.text_cached_data_b29fb6f), icon = Icons.Outlined.Storage) { client.navigation.push(Route("display-cache")) }
        }
    }
}

@Composable fun SecurityScreen(client: ClientController, state: UiState) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Section(title = str(R.string.text_security_and_unlock_7ba00a7)) {
            SettingToggle(str(R.string.text_quick_unlock_bdfe73d), checked = state.session?.fastUnlock == true, change = client::fastUnlock, enabled = !state.busy, description = str(R.string.text_local_unlock_is_independent_of_online_sign_in_expiry_5c2417d), icon = Icons.Outlined.Fingerprint)
            SettingToggle(str(R.string.text_lock_immediately_in_background_67ea031), state.immediateLock, client::immediateLock, icon = Icons.Outlined.Lock)
            ListRow(str(R.string.text_hardware_security_04e42f4), icon = Icons.Outlined.Memory) { client.navigation.open("hardware") }
            ListRow(str(R.string.text_login_identities_09c2f23), icon = Icons.Outlined.Key) { client.navigation.open("bindings") }
            ListRow(str(R.string.text_account_recovery_a3a426c), glyph = R.drawable.glyph_recovery) { client.navigation.open("recovery") }
            ListRow(str(R.string.text_message_settings_1a28455), icon = Icons.Outlined.MarkEmailRead) { client.navigation.push(Route("message-settings")) }
        }
        Section(title = str(R.string.text_account_4ab56e9), iconColor = MaterialTheme.colorScheme.errorContainer) {
            ListRow(str(R.string.text_remove_local_data_c45125e), icon = Icons.Outlined.DeleteOutline, destructive = true) { client.navigation.push(Route("account-remove")) }
            ListRow(str(R.string.text_delete_service_account_1475636), icon = Icons.Outlined.DeleteForever, destructive = true) { client.navigation.push(Route("account-delete")) }
        }
    }
}

private fun approvalSource(source: String): String = when {
    source == "gui" -> "dMsg"
    source == "cli" -> "dMsg CLI"
    source.startsWith("connect:https://") || source.startsWith("connect:http://") -> source.removePrefix("connect:")
    else -> source
}

@Composable private fun ApprovalQueueRows(title: String, entries: List<Approval>, select: (String) -> Unit) {
    if (entries.isEmpty()) return
    Section(title = title) {
        entries.forEach { entry ->
            val status = if (entry.state == "awaiting_user") str(R.string.approval_request_expires) + " · " + messageTime(entry.expiresAt)
                else secretStateName(entry.state)
            ListRow(approvalTitle(entry.kind), glyph = if (entry.kind == "device_pair") R.drawable.glyph_device else R.drawable.glyph_signature,
                supporting = approvalSource(entry.source) + "\n" + status) { select(entry.id) }
        }
    }
}

@Composable private fun StatementReview(value: ApprovalDetail) {
    val statement = value.statement() ?: return
    val content = statement["content"] as? JsonObject ?: return
    val body = value.detail as JsonObject
    DetailCard {
        SelectionContainer {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                statement.text("subject")?.let { Text(it, style = MaterialTheme.typography.titleMedium) }
                content.text("text")?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                (body["file"] as? JsonObject)?.text("name")?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                content.text("sha256")?.let { hash ->
                    LabelValue("SHA-256", hash, code = true)
                    value.fileCheck()?.let { check ->
                        Text(
                            str(when (check) {
                                ApprovalFileCheck.Checked -> R.string.text_the_original_file_was_checked_against_this_digest_c5ffd5d
                                ApprovalFileCheck.ChangedOrUnreadable -> R.string.approval_file_changed
                                ApprovalFileCheck.DigestOnly -> R.string.text_only_the_digest_is_being_reviewed_the_original_file_has_not_been_241cfe0
                            }),
                            style = MaterialTheme.typography.bodySmall,
                            color = if (check == ApprovalFileCheck.ChangedOrUnreadable) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                content.text("app_id")?.let { app -> content.text("command")?.let { Text(app + " · " + it, style = MaterialTheme.typography.bodySmall.code()) } }
            }
        }
    }
}

@Composable private fun BrowserLoginReview(body: JsonObject) {
    val content = body["content"] as? JsonObject ?: return
    DetailCard {
        content.text("purpose")?.let { purpose ->
            Text(when (purpose) {
                "Login" -> str(R.string.approval_login)
                "Link" -> str(R.string.approval_link)
                "Reauthenticate" -> str(R.string.approval_reauthenticate)
                else -> purpose
            }, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
        }
        body.text("origin")?.let { LabelValue(str(R.string.text_website_ad80f52), it) }
        content.text("receiver")?.let { LabelValue(str(R.string.approval_receiver), it, code = true) }
        content.unsigned("expires_at")?.let { LabelValue(str(R.string.approval_valid_until), displayTime(it)) }
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
    val haptics = LocalHapticFeedback.current
    LaunchedEffect(selected, state.epoch) { if (selected.isEmpty()) client.approvals.refreshRemote() else client.approvals.select(selected) }
    DisposableEffect(selected) { onDispose { client.approvals.leave(selected) } }
    val scroll = if (selected.isEmpty()) workspaceScrollState(client.navigation, "approvals:index") else key(selected) { rememberScrollState() }
    LaunchedEffect(current?.id, current?.state) {
        if (current != null && current.state != "awaiting_user") scroll.scrollTo(0)
    }
    LaunchedEffect(current?.digest) { readyAt = if (current == null) Long.MAX_VALUE else android.os.SystemClock.elapsedRealtime() + 1000 }
    LaunchedEffect(current?.id, current?.state) {
        while (current?.state == "awaiting_user") {
            now = System.currentTimeMillis().toULong()
            uptime = android.os.SystemClock.elapsedRealtime()
            kotlinx.coroutines.delay(1000)
        }
    }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(scroll).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (selected.isEmpty()) {
                ReadFeedback(listing, client.approvals::refreshRemote)
                val entries = listing.valueOrNull().orEmpty()
                val (waiting, processing) = entries.partition { it.state == "awaiting_user" }
                ApprovalQueueRows(str(R.string.text_waiting_for_approval_45d78c0), waiting.sortedBy { it.expiresAt }, select)
                ApprovalQueueRows(str(R.string.text_in_progress_6906b67), processing, select)
                if (listing is ReadState.Empty) EmptyState(str(R.string.text_no_pending_approvals_58ba7ee), glyph = R.drawable.glyph_approvals)
                Text(str(R.string.text_desktop_local_requests_stay_on_that_desktop_only_requests_availab_7e3389e),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                ReadFeedback(detail, { client.approvals.select(selected) })
                current?.let { value ->
                    val body = value.detail as? JsonObject
                    val disclosure = when {
                        value.state == "awaiting_user" && value.kind == "device_pair" -> R.string.text_this_device_may_receive_your_private_content_approval_must_be_fol_6938488
                        value.kind == "connect" -> R.string.text_the_source_is_relayed_by_the_browser_through_connect_dmsg_net_a_b_8ce6a30
                        value.kind == "account" && body?.text("change") == "recovery_policy" ->
                            if (body.flag("enabled")) R.string.text_a_bound_login_can_recover_uploaded_content_after_the_waiting_peri_31381f9
                            else R.string.text_keep_an_authorized_device_you_can_unlock_recovering_a_login_alone_11b8c07
                        else -> null
                    }
                    val banner = when {
                        value.error != null -> errorText(value.error) to true
                        value.state == "awaiting_user" && !value.supported -> str(R.string.text_this_action_requires_a_supported_desktop_client_85a09eb) to false
                        value.state in setOf("executing", "unknown") -> str(R.string.text_the_result_is_not_confirmed_check_the_original_operation_before_r_57342fb) to false
                        else -> null
                    }
                    banner?.let { (message, warning) -> Notice(message, warning) }
                    DetailCard {
                        Text(approvalTitle(value.kind),
                            modifier = if (detail is ReadState.Ready) Modifier.reminderVisible(client, NoticeTarget("approval", value.id), revision = value) else Modifier,
                            style = MaterialTheme.typography.titleLarge)
                        if (value.state != "awaiting_user") Text(secretStateName(value.state), style = MaterialTheme.typography.titleSmall,
                            color = MaterialTheme.colorScheme.primary, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                        if (value.state == "awaiting_user" && value.risk == "high") {
                            Text(str(R.string.text_review_this_sensitive_action_carefully_9a6034a), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.error)
                        }
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        SelectionContainer { LabelValue(str(R.string.text_source_fcdba0f), approvalSource(value.source)) }
                        if (value.state == "awaiting_user") LabelValue(str(R.string.approval_request_expires), messageTime(value.expiresAt))
                        SelectionContainer { LabelValue(str(R.string.text_account_id_43f0a66), state.session?.account.orEmpty(), code = true) }
                        disclosure?.let { Text(str(it), style = MaterialTheme.typography.bodyMedium) }
                    }
                    if (value.kind == "attest" || value.kind == "connect") StatementReview(value)
                    val webLogin = value.browserLogin()
                    val statement = value.statement()
                    val plainText = statement?.text("profile") == "text" && !(statement["content"] as? JsonObject)?.text("text").isNullOrEmpty()
                    if (webLogin != null) BrowserLoginReview(webLogin)
                    if (value.kind == "vault_backup" && body != null) BackupApprovalContent(body)
                    else if (value.secret != null) SecretApprovalContent(value.secret)
                    else if (webLogin == null && !plainText) HumanRecord(value.detail)
                    value.record["result"]?.takeIf { it != JsonNull }?.let { SectionTitle(str(R.string.text_result_d6b21bc)); HumanRecord(it) }
                    value.record["delivery_report"]?.takeIf { it != JsonNull }?.let { SectionTitle(str(R.string.text_delivery_report_29f483d)); HumanRecord(it) }
                    Section {
                        ListRow(
                            headline = str(R.string.text_technical_details_5ec1b41),
                            icon = Icons.Outlined.DataObject,
                            trailing = { Icon(if (technical) Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore, null) },
                        ) { technical = !technical }
                    }
                    if (technical) {
                        if (webLogin != null || plainText) HumanRecord(value.detail)
                        HumanRecord(JsonObject(value.record.filterKeys { it !in setOf("detail", "result", "delivery_report") }))
                    }
                    if (value.state in setOf("executing", "unknown")) {
                        OutlinedButton(onClick = { client.approvals.reconcile(value) }, enabled = !state.working("approval:reconcile:" + value.id), modifier = Modifier.fillMaxWidth()) {
                            Text(str(R.string.text_reconcile_original_operation_065467d))
                        }
                    }
                }
            }
        }
        current?.takeIf { it.state == "awaiting_user" }?.let { value ->
            BottomActionBar {
                val submitting = state.working("approval:decide:" + selected)
                if (now >= value.expiresAt) {
                    Text(str(R.string.text_this_request_expired_refresh_before_continuing_04ccc12), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                    OutlinedButton(onClick = { client.approvals.select(selected) }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_refresh_cba212b)) }
                } else {
                    Text(str(if (submitting) R.string.text_waiting_for_confirmation_9c87aad else R.string.text_your_decision_applies_only_to_this_request_a7693fb),
                        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                    val actionable = !submitting && detail is ReadState.Ready && uptime >= readyAt
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlinedButton(enabled = actionable, onClick = { decision = value to false }, modifier = Modifier.weight(1f)) { Text(str(R.string.text_reject_52e8be4)) }
                        Button(enabled = actionable && value.supported, onClick = { decision = value to true }, modifier = Modifier.weight(1f)) { Text(str(R.string.text_approve_8d032fb)) }
                    }
                }
            }
        }
    }
    LaunchedEffect(current?.id, current?.state, current?.digest, now >= (current?.expiresAt ?: ULong.MAX_VALUE)) {
        val pending = decision?.first ?: return@LaunchedEffect
        if (current?.id != pending.id || current.digest != pending.digest || current.state != "awaiting_user" || now >= pending.expiresAt) decision = null
    }
    decision?.let { (value, approve) ->
        AlertDialog(onDismissRequest = { decision = null }, title = { Text(if (approve) str(R.string.text_approve_the_reviewed_request_18d90f3) else str(R.string.text_reject_this_request_24df6c9)) }, text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(approvalTitle(value.kind), style = MaterialTheme.typography.titleMedium)
                LabelValue(str(R.string.text_source_fcdba0f), approvalSource(value.source))
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
                    val webLogin = value.browserLogin()
                    if (webLogin != null) BrowserLoginReview(webLogin)
                    else if (value.kind == "vault_backup" && body != null) BackupApprovalContent(body)
                    else HumanRecord(value.detail)
                }
                HorizontalDivider()
                SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    LabelValue(str(R.string.text_request_id_3182520), value.id, code = true)
                    LabelValue(str(R.string.text_reviewed_digest_ea1e6f9), value.digest, code = true)
                } }
                Text(str(R.string.text_this_decision_is_bound_to_the_exact_request_and_digest_shown_0594140), style = MaterialTheme.typography.bodySmall)
            }
        }, confirmButton = {
            TextButton(onClick = {
                decision = null
                client.approvals.decide(value, approve) { haptics.performHapticFeedback(HapticFeedbackType.Confirm) }
            }, enabled = current?.id == value.id && current.digest == value.digest && current.state == "awaiting_user" &&
                detail is ReadState.Ready && now < value.expiresAt && (!approve || current.supported) &&
                !state.working("approval:decide:" + selected) && state.unlocked) {
                Text(if (approve) str(R.string.text_approve_this_request_fac866f) else str(R.string.text_reject_this_request_46758a1))
            }
        }, dismissButton = { TextButton(onClick = { decision = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
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
    if (rows.isEmpty()) return
    DetailCard {
        rows.forEachIndexed { index, (path, child) ->
            if (index > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                if (path.isNotBlank()) Text(path, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                SelectionContainer {
                    Text(
                        if (child == null || child == JsonNull) str(R.string.text_none_2d00776) else if (child.isString && child.content.isEmpty()) str(R.string.text_empty_text_8d2743c) else child.content,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        }
    }
}
private fun fieldLabel(key: String): String = when (key) {
    "key_id" -> str(R.string.pair_key_fingerprint)
    "this_device" -> str(R.string.text_this_device_ce825c6)
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
