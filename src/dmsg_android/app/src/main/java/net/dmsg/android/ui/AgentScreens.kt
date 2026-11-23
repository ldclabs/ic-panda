package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.numericExpression

@Composable fun AgentsScreen(client: ClientController) {
    val listing by client.agents.listing.state.collectAsStateWithLifecycle()
    val quota by client.agents.quota.state.collectAsStateWithLifecycle()
    val label by client.agents.label.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val focus = LocalFocusManager.current
    val busy = state.working("agents:create")
    LaunchedEffect(state.epoch) { client.agents.load(); client.approvals.settle() }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Notice(str(R.string.text_manage_agent_identities_and_permissions_here_android_does_not_run_2cd2699))
            AccountRow(str(R.string.text_controllers_and_delegations_8ce7d82)) { client.navigation.push(Route("agent-delegations")) }
            ReadFeedback(listing, client.agents::load)
            if (quota is ReadState.Ready) quota.valueOrNull()?.let {
                LabelValue(str(R.string.hosted_agent_ids), numericExpression(localizedNumber(it.used) + " / " + (it.limit?.let(::localizedNumber) ?: "—")))
            }
            if (listing is ReadState.Empty) EmptyPanel(str(R.string.text_no_agents_f2fe175))
            listing.valueOrNull().orEmpty().forEach { agent ->
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        AccountRow(agent.label) { client.navigation.push(Route("agent", agent.id)) }
                        Text(if (agent.paused) str(R.string.text_paused_cf54b60) else str(R.string.text_enabled_b62610f), style = MaterialTheme.typography.labelMedium)
                        SelectionContainer { Text(agent.id, style = MaterialTheme.typography.bodySmall) }
                    }
                }
            }
            SectionTitle(str(R.string.text_create_an_agent_identity_4207389))
            OutlinedTextField(label, client.agents::editLabel, Modifier.fillMaxWidth().revealFocusedInput().testTag("agent-label"),
                label = { Text(str(R.string.text_local_label_c31fe59)) }, supportingText = { Text(str(R.string.text_1_40_lowercase_letters_digits_or_525c917)) }, singleLine = true, enabled = !busy)
        }
        Surface {
            Button(onClick = { focus.clearFocus(); client.agents.create(label) }, enabled = validAgentLabel(label.trim()) && !busy && listing.valueOrNull().orEmpty().size < 16,
                modifier = Modifier.fillMaxWidth().padding(pagePadding())) { Text(str(R.string.text_create_agent_f6a5a91)) }
        }
    }
}

@Composable fun AgentDetailScreen(client: ClientController, id: String) {
    val listing by client.agents.listing.state.collectAsStateWithLifecycle()
    val grants by client.agents.grants.state.collectAsStateWithLifecycle()
    val drafts by client.agents.permissions.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val draft = drafts[id] ?: AgentPermissionsDraft()
    val agent = listing.valueOrNull()?.firstOrNull { it.id == id }
    val fresh = listing is ReadState.Ready
    val busy = state.working("agents:pause:" + id) || state.working("agents:remove:" + id)
    var removing by remember { mutableStateOf(false) }
    var revoking by remember { mutableStateOf<AgentDeviceGrant?>(null) }
    val focus = LocalFocusManager.current
    LaunchedEffect(id, state.epoch) { client.agents.load(); client.approvals.settle() }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            ReadFeedback(listing, client.agents::load)
            if (agent != null) {
                Text(agent.label, style = MaterialTheme.typography.headlineSmall)
                AgentFingerprint(str(R.string.text_agent_id_badfded), agent.id)
                NoticeSwitch(str(R.string.text_pause_this_agent_10ad216), agent.paused, fresh && !busy) { client.agents.pause(agent, it) }
                SectionTitle(str(R.string.text_device_permissions_104e275))
                Text(str(R.string.text_external_ai_clients_handle_shared_content_under_their_own_data_po_3612220), style = MaterialTheme.typography.bodyMedium)
                ReadFeedback(grants, client.agents::load)
                val currentGrants = grants.valueOrNull().orEmpty().filter { it.grantee == id }
                if (grants is ReadState.Empty || grants is ReadState.Ready && currentGrants.isEmpty()) Text(str(R.string.text_no_device_grants_2e01884))
                currentGrants.forEach { grant ->
                    Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        SelectionContainer { Text(grant.capabilities.joinToString("\n", transform = ::permissionDescription)) }
                        grant.expiresAt?.let { LabelValue(str(R.string.text_expires_12414d7), displayTime(it)) }
                        AgentFingerprint(str(R.string.text_grant_id_1fc1a04), grant.id)
                        if (grant.revokedAt != null) Text(str(R.string.text_revoked_c4fbad3))
                        else {
                            if (grant.expiresAt?.let { it <= System.currentTimeMillis().toULong() } == true) Text(str(R.string.text_expired_70446d7))
                            TextButton(onClick = { revoking = grant }, enabled = grants is ReadState.Ready && !state.working("agents:revoke:" + grant.id)) {
                                Text(str(R.string.text_revoke_grant_f246313), color = MaterialTheme.colorScheme.error)
                            }
                        }
                    } }
                }
                AgentPermissionPicker(client, id, draft)
                AgentDaysField(draft.days) { client.agents.editPermissions(id, draft.copy(days = it)) }
                TextButton(onClick = { removing = true }, enabled = fresh && !busy) { Text(str(R.string.text_remove_this_local_agent_27c101c), color = MaterialTheme.colorScheme.error) }
            } else if (listing is ReadState.Ready || listing is ReadState.Empty) EmptyPanel(str(R.string.text_this_local_agent_was_removed_e18a5d0))
        }
        Surface {
            Button(onClick = { focus.clearFocus(); agent?.let(client.agents::requestPermissions) }, enabled = agent != null && fresh && draft.valid && !busy && !state.working("approval:request:agent:permissions:" + id),
                modifier = Modifier.fillMaxWidth().padding(pagePadding())) { Text(str(R.string.text_review_permissions_7194a14)) }
        }
    }
    if (removing && agent != null) AlertDialog(onDismissRequest = { removing = false }, title = { Text(str(R.string.text_remove_this_local_agent_4a373f8)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) { AgentFingerprint(str(R.string.text_agent_id_badfded), id); Text(str(R.string.text_previously_delivered_credentials_and_copies_remain_independent_re_045f5cb)) }
    }, confirmButton = { TextButton(onClick = { removing = false; client.agents.remove(agent) }) { Text(str(R.string.text_remove_78190c6), color = MaterialTheme.colorScheme.error) } }, dismissButton = { TextButton(onClick = { removing = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    revoking?.let { grant -> AlertDialog(onDismissRequest = { revoking = null }, title = { Text(str(R.string.text_revoke_this_device_grant_bfda0fb)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) { Text(grant.capabilities.joinToString("\n", transform = ::permissionDescription)); AgentFingerprint(str(R.string.text_grant_id_1fc1a04), grant.id) }
    }, confirmButton = { TextButton(onClick = { revoking = null; client.agents.revokeGrant(grant) }) { Text(str(R.string.text_revoke_411a6c9), color = MaterialTheme.colorScheme.error) } }, dismissButton = { TextButton(onClick = { revoking = null }) { Text(str(R.string.text_cancel_998b9c4)) } }) }
}

@Composable private fun AgentPermissionPicker(client: ClientController, id: String, draft: AgentPermissionsDraft) {
    val options = remember { net.dmsg.core.agentCapabilityVocabulary().filterNot { it == "profile.read" } }
    val permissions = agentLines(draft.capabilities)
    var expanded by remember(id) { mutableStateOf(false) }
    val pattern = draft.pattern
    val target = draft.target
    var choosing by remember(pattern, id) { mutableStateOf(false) }
    val kind = pattern.substringAfter(":<", "").removeSuffix(">")
    val candidate = if (kind.isEmpty()) pattern else pattern.substringBefore(':') + ":" + target.trim()
    fun edit(next: AgentPermissionsDraft) { client.agents.editPermissions(id, next) }
    permissions.forEach { value ->
        ListItem(headlineContent = { Text(permissionDescription(value)) }, trailingContent = {
            IconButton(onClick = { edit(draft.copy(capabilities = (permissions - value).joinToString("\n"))) }) {
                Icon(Icons.Outlined.Close, str(R.string.text_remove_78190c6) + " · " + permissionDescription(value))
            }
        })
    }
    Box {
        OutlinedButton(onClick = { expanded = true }, modifier = Modifier.testTag("agent-permission-picker")) {
            Text(if (pattern.isEmpty()) str(R.string.choose_permission) else permissionDescription(pattern))
        }
        DropdownMenu(expanded, { expanded = false }) {
            options.forEach { option ->
                DropdownMenuItem(text = { Text(permissionDescription(option)) }, modifier = Modifier.testTag("permission-option:" + option),
                    onClick = { edit(draft.copy(pattern = option, target = "")); expanded = false })
            }
        }
    }
    if (kind.isNotEmpty()) {
        val label = str(when (kind) {
            "folder" -> R.string.text_folder_b0d1441
            "channel" -> R.string.permission_channel_id
            "policy" -> R.string.permission_policy_id
            "wallet" -> R.string.text_wallet_canister_id_a1a88b6
            "origin" -> R.string.text_website_ad80f52
            else -> R.string.text_tags_33e9c4d
        })
        OutlinedTextField(target, { edit(draft.copy(target = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), singleLine = true, label = { Text(label) })
        if (kind in setOf("channel", "folder")) {
            val channels by client.messages.channels.state.collectAsStateWithLifecycle()
            val folders by client.library.tree.state.collectAsStateWithLifecycle()
            LaunchedEffect(kind) { if (kind == "channel") client.messages.browse() else client.library.loadTree() }
            val choices = if (kind == "channel") channels.valueOrNull().orEmpty().map { it.id to it.name }
                else listOf("root" to str(R.string.shortcut_files)) + folders.valueOrNull().orEmpty().filterNot { it.id == "root" }.map { it.id to (if (it.path.lastOrNull() == it.name) it.path else it.path + it.name).joinToString(" / ") }
            if (choices.isNotEmpty()) Box {
                TextButton(onClick = { choosing = true }) { Text(str(if (kind == "channel") R.string.text_conversations_23a085e else R.string.text_folder_b0d1441)) }
                DropdownMenu(choosing, { choosing = false }) {
                    choices.forEach { (value, title) -> DropdownMenuItem(text = { Text(title) }, onClick = { edit(draft.copy(target = value)); choosing = false }) }
                }
            }
        }
    }
    TextButton(onClick = { edit(draft.copy(capabilities = (permissions + candidate).joinToString("\n"), pattern = "", target = "")) },
        enabled = pattern.isNotEmpty() && permissions.size < 16 && candidate !in permissions && net.dmsg.core.validAgentCapability(candidate),
        modifier = Modifier.testTag("agent-permission-add")) { Text(str(R.string.add_permission)) }
}

internal fun permissionDescription(value: String): String {
    val code = value.substringBefore(':')
    val group = when (code.substringBeforeLast('.')) {
        "account" -> str(R.string.permission_account_information)
        "vault", "secret" -> str(R.string.shortcut_vault)
        "files" -> str(R.string.shortcut_files)
        "channel" -> str(R.string.text_conversations_23a085e)
        "inbox" -> str(R.string.text_inbox_2cd8ee3)
        "profile" -> str(R.string.text_public_profile_a67b108)
        "wallet" -> str(R.string.text_wallet_77c08be)
        "agent.delegation" -> str(R.string.permission_delegations)
        "agent.profile" -> "Agent Profile"
        "discourse" -> "Agent Discourse"
        "mail" -> "Agent Mail"
        "knowledge" -> "Agent Knowledge"
        else -> return when (code) {
            "attest.request" -> str(R.string.permission_request_a_signature)
            "action.request" -> str(R.string.permission_request_an_action)
            "events.endpoint" -> str(R.string.permission_manage_event_endpoints)
            else -> value
        }
    }
    val action = when (code.substringAfterLast('.')) {
        "read" -> R.string.text_read_e0ed4e0
        "write" -> R.string.permission_write
        "list" -> R.string.permission_list_items
        "publish" -> R.string.text_publish_88bb01f
        "contact" -> R.string.text_write_a_letter_1a8f9c9
        "use" -> R.string.permission_use_a_secret_through_a_device_policy
        "propose" -> R.string.text_create_proposal_4946bd1
        "execute" -> R.string.text_execute_approved_proposal_99a02a0
        else -> return value
    }
    val target = value.substringAfter(':', "")
    return listOfNotNull(group, str(action), target.takeIf { it.isNotEmpty() && !it.startsWith('<') }?.let { if (it == "*") str(R.string.text_all_eae0ae1) + " (*)" else it }).joinToString(" · ")
}

@Composable fun AgentDelegationsScreen(client: ClientController) {
    val principal by client.agents.principal.state.collectAsStateWithLifecycle()
    val credentials by client.agents.credentials.state.collectAsStateWithLifecycle()
    val devices by client.account.devices.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val value = principal.valueOrNull()
    val manages = devices is ReadState.Ready && devices.valueOrNull()?.canManage == true
    val fresh = principal is ReadState.Ready
    LaunchedEffect(state.epoch) { client.agents.loadPrincipal(); client.approvals.settle() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            ReadFeedback(principal, devices, client.agents::loadPrincipal, client.account::devices)
            if (!manages) Notice(str(R.string.text_account_changes_need_an_administrator_with_account_management_per_5eddc16))
            if (principal is ReadState.Empty) Button(onClick = client.agents::enablePrincipal, enabled = manages && !state.working("approval:request:agent:enable")) { Text(str(R.string.text_review_enabling_a_principal_a184a02)) }
            value?.let { record ->
                AgentFingerprint(str(R.string.text_principal_415a07f), record.id)
                LabelValue(str(R.string.text_current_version_b3f03bb), record.version.toString())
                LabelValue(str(R.string.text_published_version_a8c0a2f), record.publishedVersion.toString())
                if (!record.published) {
                    Notice(str(R.string.text_account_state_is_saved_but_its_current_directory_publication_has_fe37881))
                    OutlinedButton(onClick = client.agents::publish, enabled = fresh && manages && !state.working("agents:publish")) { Text(str(R.string.text_retry_directory_publication_361ba81)) }
                } else Text(str(R.string.text_current_version_published_baed38a), style = MaterialTheme.typography.labelMedium)
                SectionTitle(str(R.string.text_controllers_3e82974))
                record.controllers.forEach { controller ->
                    Card(Modifier.fillMaxWidth().padding(vertical = 6.dp)) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(controller.name ?: ("#" + controller.generation), style = MaterialTheme.typography.titleMedium)
                        AgentFingerprint(str(R.string.text_controller_id_b3d512a), controller.id)
                        LabelValue(str(R.string.text_valid_from_a50ea2d), displayTime(controller.validFrom))
                        if (controller.unrestricted) Notice(str(R.string.text_unrestricted_delegation_authority_db589ee), true)
                        else { LabelValue(str(R.string.text_scopes_b1c46da), controller.scopes.joinToString("\n")); LabelValue(str(R.string.text_audiences_bb87be7), controller.audiences.joinToString("\n")) }
                        if (controller.retiredAt != null) LabelValue(str(R.string.text_retired_at_f2a5e63), displayTime(controller.retiredAt))
                        controller.invalidFrom?.let { LabelValue(str(R.string.text_invalid_from_f0dbf64), displayTime(it)) }
                        if (controller.retiredAt == null) TextButton(onClick = { client.agents.retireController(controller.generation) }, enabled = fresh && manages && !state.busy) { Text(str(R.string.text_review_retiring_controller_1270578), color = MaterialTheme.colorScheme.error) }
                    } }
                }
                Text(str(R.string.text_retiring_a_controller_does_not_retrieve_copied_private_keys_or_cr_5018e54), style = MaterialTheme.typography.bodySmall)
                if (manages && fresh) AccountRow(str(R.string.text_new_controller_4d1faa5)) { client.navigation.push(Route("agent-controller")) }
                if (manages && fresh && record.published) AccountRow(str(R.string.text_grant_delegation_ab521bf)) { client.navigation.push(Route("agent-grant")) }
                SectionTitle(str(R.string.text_issued_credentials_0417e9e))
                ReadFeedback(credentials, client.agents::loadCredentials)
                if (credentials is ReadState.Empty) Text(str(R.string.text_no_credentials_f01d2bc))
            }
        }
        credentials.valueOrNull().orEmpty().forEach { credential ->
            item(key = credential.id) {
                Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    AgentFingerprint(str(R.string.text_subject_agent_id_a88565c), credential.subject)
                    Text(agentCredentialState(credential.status), style = MaterialTheme.typography.titleSmall)
                    LabelValue(str(R.string.text_scopes_b1c46da), credential.scopes.joinToString("\n"))
                    LabelValue(str(R.string.text_audiences_bb87be7), credential.audiences.joinToString("\n"))
                    credential.notBefore?.let { LabelValue(str(R.string.text_valid_from_a50ea2d), displayTime(it)) }
                    credential.expiresAt?.let { LabelValue(str(R.string.text_expires_12414d7), displayTime(it)) }
                    AgentFingerprint(str(R.string.text_credential_id_b5cd509), credential.id)
                    if (credential.status != "revoked") TextButton(onClick = { client.agents.revokeDelegation(credential.id) }, enabled = manages && fresh && credentials is ReadState.Ready && !state.busy) { Text(str(R.string.text_review_revocation_e403289), color = MaterialTheme.colorScheme.error) }
                } }
            }
        }
    }
}

@Composable fun AgentDelegationFormScreen(client: ClientController, controller: Boolean) {
    val draft by client.agents.delegation.collectAsStateWithLifecycle()
    val principal by client.agents.principal.state.collectAsStateWithLifecycle()
    val devices by client.account.devices.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val focus = LocalFocusManager.current
    val ready = devices is ReadState.Ready && devices.valueOrNull()?.canManage == true && principal is ReadState.Ready
    LaunchedEffect(state.epoch) { client.agents.loadPrincipal(); client.approvals.settle() }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            ReadFeedback(principal, devices, client.agents::loadPrincipal, client.account::devices)
            principal.valueOrNull()?.let { AgentFingerprint(str(R.string.text_principal_415a07f), it.id) }
            if (controller) {
                OutlinedTextField(draft.name, { client.agents.editDelegation(draft.copy(name = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_controller_name_e2bf892)) }, singleLine = true)
                NoticeSwitch(str(R.string.text_unrestricted_delegation_authority_db589ee), draft.unrestricted, !state.busy) { client.agents.editDelegation(draft.copy(unrestricted = it)) }
                if (draft.unrestricted) Notice(str(R.string.text_this_controller_can_delegate_any_supported_scope_to_any_audience_ba44588), true)
                else AgentAuthorityFields(draft.controller) { client.agents.editDelegation(draft.copy(controller = it)) }
            } else {
                OutlinedTextField(draft.subject, { client.agents.editDelegation(draft.copy(subject = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_subject_agent_id_a88565c)) })
                AgentAuthorityFields(draft.grant) { client.agents.editDelegation(draft.copy(grant = it)) }
                AgentDaysField(draft.days) { client.agents.editDelegation(draft.copy(days = it)) }
                Text(str(R.string.text_delegation_starts_when_the_service_issues_the_credential_its_stat_1e20149), style = MaterialTheme.typography.bodySmall)
            }
        }
        Surface {
            Button(onClick = { focus.clearFocus(); if (controller) client.agents.registerController() else client.agents.grantDelegation() },
                enabled = ready && !state.busy && if (controller) draft.validController else draft.validGrant && principal.valueOrNull()?.published == true,
                modifier = Modifier.fillMaxWidth().padding(pagePadding())) { Text(if (controller) str(R.string.text_review_controller_registration_992d053) else str(R.string.text_review_delegation_grant_15c19b3)) }
        }
    }
}
@Composable private fun AgentAuthorityFields(value: AgentAuthority, edit: (AgentAuthority) -> Unit) {
    OutlinedTextField(value.scopes, { edit(value.copy(scopes = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_scopes_one_per_line_20c88cb)) }, supportingText = { Text(str(R.string.text_1_8_scopes_no_wildcards_404684a)) }, minLines = 2, maxLines = 8)
    OutlinedTextField(value.audiences, { edit(value.copy(audiences = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_audiences_one_per_line_ae393a1)) }, supportingText = { Text(str(R.string.text_1_4_https_origins_or_agent_ids_da10a1d)) }, minLines = 2, maxLines = 4)
}
@Composable private fun AgentDaysField(value: String, edit: (String) -> Unit) {
    OutlinedTextField(value, edit, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_valid_for_days_13075ad)) }, supportingText = { Text(str(R.string.text_1_366_days_602f2fd)) }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
}
@Composable private fun AgentFingerprint(label: String, value: String) {
    Text(label, style = MaterialTheme.typography.labelMedium)
    SelectionContainer { Text(value, style = MaterialTheme.typography.bodySmall.code()) }
}
private fun agentCredentialState(value: String) = when (value) {
    "active" -> str(R.string.text_active_45e5d33); "revoked" -> str(R.string.text_revoked_c4fbad3); "suspended" -> str(R.string.text_suspended_ae8b286)
    "expired" -> str(R.string.text_expired_70446d7); else -> str(R.string.text_status_unknown_a31f531) + value
}
