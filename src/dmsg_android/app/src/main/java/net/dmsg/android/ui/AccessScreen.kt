package net.dmsg.android.ui

import net.dmsg.android.platform.str
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonPrimitive
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.core.PasskeyOperation

private enum class PasskeyIntent { NEW_WORKSPACE, EXISTING_ACCOUNT }

@Composable private fun InternetIdentityButton(primary: Boolean, enabled: Boolean, onClick: () -> Unit) {
    val modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)
    val label = str(R.string.text_sign_in_with_internet_identity_4d3941e)
    if (primary) Button(onClick, modifier, enabled) { Text(label) }
    else OutlinedButton(onClick, modifier, enabled) { Text(label) }
}

@Composable internal fun EntryScreen(client: ClientController, state: UiState, browser: (String) -> Unit, openLogin: (String) -> Unit, snackbar: SnackbarHostState) {
    val page by client.access.collectAsStateWithLifecycle()
    val anotherAccount = page == AccessPage.SIGN_IN_ANOTHER
    val authentication by client.authentication.state.collectAsStateWithLifecycle()
    val incoming by client.incoming.intake.collectAsStateWithLifecycle()
    var information by remember { mutableStateOf<String?>(null) }
    var choosingAccount by remember { mutableStateOf(false) }
    var choosingLanguage by remember { mutableStateOf(false) }
    LaunchedEffect(state.epoch) { choosingAccount = false }
    var confirmingReset by remember { mutableStateOf(false) }
    var slow by remember { mutableStateOf(false) }
    val status = state.session?.state
    var intent by rememberSaveable(page, status, state.session?.account, state.session?.passkeyAvailable) { mutableStateOf<PasskeyIntent?>(null) }
    val setup = state.session?.setupPending == true || status == "no_account"
    val authorize = status == "pairing_required" || status == "root_required"
    LaunchedEffect(state.busy, setup) { slow = false; if (state.busy && setup) { delay(15_000); slow = true } }
    fun backToWelcome() {
        if (anotherAccount) {
            if (state.busy) return
            client.cancelAuthentication()
        }
        client.showAccess(AccessPage.WELCOME)
    }
    BackHandler(page != AccessPage.WELCOME, onBack = ::backToWelcome)
    BackHandler(intent != null) { intent = null; client.cancelAuthentication() }
    Scaffold(snackbarHost = { SnackbarHost(snackbar) }) { padding ->
        if (status == "moved" && !anotherAccount) { Box(Modifier.padding(padding)) { MovedWorkspaceScreen(client) }; return@Scaffold }
        if (page != AccessPage.WELCOME && !anotherAccount) {
            Column(Modifier.fillMaxSize().padding(padding)) {
                TextButton(onClick = { client.showAccess(AccessPage.WELCOME) }) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, null); Text(str(R.string.text_back_5db5cac)) }
                when (page) {
                    AccessPage.JOIN_ACCOUNT -> JoinAccountScreen(client)
                    AccessPage.PAIRING -> DevicePairingScreen(client)
                    AccessPage.RECOVERY -> RecoveryScreen(client)
                    AccessPage.HANDOVER -> HandoverRequestScreen(client)
                    else -> Unit
                }
            }
            return@Scaffold
        }
        Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 560.dp).fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                if (incoming.working) { LinearProgressIndicator(Modifier.fillMaxWidth()); Text(str(R.string.text_encrypting_shared_items_for_later_import_8caa029)) }
                else if (incoming.count > 0) Notice(str(R.string.text_shared_items_are_waiting_unlock_an_account_to_review_and_import_t_713dc4a))
                incoming.error?.let { Notice(errorText(it), true) }
                if (intent != null) IconButton(onClick = { intent = null; client.cancelAuthentication() }) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, str(R.string.text_back_5db5cac)) }
                else if (anotherAccount) TextButton(onClick = ::backToWelcome, enabled = !state.busy) { Text(str(R.string.text_cancel_998b9c4)) }
                DetailCard(color = MaterialTheme.colorScheme.surface) {
                    Image(painterResource(R.drawable.dmsg_mark), null, Modifier.size(54.dp))
                    Text(str(when (intent) {
                        PasskeyIntent.NEW_WORKSPACE -> R.string.access_new_workspace
                        PasskeyIntent.EXISTING_ACCOUNT -> R.string.access_existing_account
                        null -> when {
                            anotherAccount -> R.string.access_sign_in_another
                            status == "locked" -> R.string.text_welcome_back_70da839
                            else -> R.string.text_keep_what_matters_private_2a3adeb
                        }
                    }), style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
                    Text(str(when (intent) {
                        PasskeyIntent.NEW_WORKSPACE -> R.string.text_your_passkey_will_be_saved_by_your_credential_provider_this_creat_527af1f
                        PasskeyIntent.EXISTING_ACCOUNT -> R.string.access_linked_login
                        null -> if (anotherAccount) R.string.access_linked_login else R.string.text_protect_passwords_notes_and_files_with_end_to_end_encryption_sync_50ec150
                    }), color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                when {
                    state.session == null -> {
                        Notice(str(R.string.text_the_encrypted_workspace_could_not_be_opened_on_this_device_871676c), true)
                        Button(onClick = client::start) { Text(str(R.string.text_try_again_672e778)) }
                    }
                    intent == PasskeyIntent.NEW_WORKSPACE -> {
                        Button(onClick = { client.passkey(PasskeyOperation.REGISTER) }, enabled = !authentication.working && !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
                            Icon(Icons.Outlined.Key, null); Spacer(Modifier.width(10.dp)); Text(str(R.string.text_create_passkey_fb912be))
                        }
                        TextButton(onClick = { intent = PasskeyIntent.EXISTING_ACCOUNT }, enabled = !authentication.working && !state.busy) { Text(str(R.string.text_already_have_a_passkey_sign_in_9207f1f)) }
                    }
                    intent == PasskeyIntent.EXISTING_ACCOUNT -> {
                        Notice(str(if (status == "root_required") R.string.pair_preparing_vault else R.string.text_approve_this_phone_from_an_authorized_device_then_continue_here_7cbcaea))
                        Button(onClick = { client.passkey() }, enabled = !authentication.working && !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text(str(R.string.access_existing_passkey)) }
                    }
                    anotherAccount -> {
                        OutlinedButton(onClick = { client.showAccountInvitation(true) }) { Text(str(R.string.access_existing_account)) }
                        if (state.session?.passkeyAvailable == true) Button(onClick = { intent = PasskeyIntent.EXISTING_ACCOUNT }, enabled = !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
                            Text(str(R.string.text_sign_in_with_passkey_0c861d7))
                        }
                        InternetIdentityButton(primary = state.session?.passkeyAvailable != true, enabled = !state.busy) { client.login(open = openLogin) }
                    }
                    setup -> {
                        if (status == "no_account") Button(onClick = { client.showAccountInvitation(true) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.access_existing_account)) }
                        OutlinedButton(onClick = client::initialize, enabled = !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text(str(R.string.join_create)) }
                        Notice(if (slow) str(R.string.text_the_network_is_taking_longer_than_usual_your_setup_progress_is_ke_d703303)
                            else str(if (status == "no_account") R.string.join_scan_hint else R.string.text_create_your_encrypted_workspace_to_start_saving_passwords_notes_a_4982122))
                        TextButton(onClick = client::signInToAnotherAccount, enabled = !state.busy) { Text(str(R.string.text_switch_account_52f71df)) }
                    }
                    authorize -> {
                        Notice(str(if (status == "root_required") R.string.pair_preparing_vault else R.string.text_approve_this_phone_from_an_authorized_device_then_continue_here_7cbcaea))
                        Button(onClick = { client.showAccess(AccessPage.PAIRING) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(str(if (status == "root_required") R.string.text_check_authorization_039f8a2 else R.string.text_authorize_this_device_3a0243f)) }
                        TextButton(onClick = { client.showAccess(AccessPage.RECOVERY) }) { Text(str(R.string.text_all_devices_lost_c768f48)) }
                    }
                    else -> {
                        if (state.session?.fastUnlock == true) Button(onClick = client::unlock, enabled = !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
                            Icon(Icons.Outlined.LockOpen, null); Spacer(Modifier.width(10.dp)); Text(str(R.string.text_unlock_workspace_ec9cb2a))
                        }
                        if (state.session?.state == "logged_out") {
                            if (state.session?.passkeyAvailable != true) OutlinedButton(onClick = { client.showAccountInvitation(true) }) { Text(str(R.string.access_existing_account)) }
                            OutlinedButton(onClick = client::openLocal, enabled = !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp).testTag("access.local")) { Text(str(R.string.use_on_device)) }
                        }
                        if (state.session?.accountKind != "local") {
                            if (state.session?.passkeyAvailable == true) {
                                if (state.session?.account == null && state.session?.fastUnlock != true) {
                                    Button(onClick = { client.showAccountInvitation(true) }, enabled = !authentication.working && !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text(str(R.string.access_existing_account)) }
                                    OutlinedButton(onClick = { intent = PasskeyIntent.NEW_WORKSPACE }, enabled = !authentication.working && !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text(str(R.string.join_create)) }
                                    TextButton(onClick = { intent = PasskeyIntent.EXISTING_ACCOUNT }, enabled = !authentication.working && !state.busy) { Text(str(R.string.access_existing_passkey)) }
                                } else {
                                    OutlinedButton(onClick = { client.passkey() }, enabled = !authentication.working && !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text(str(R.string.text_sign_in_with_passkey_0c861d7)) }
                                    TextButton(onClick = { intent = PasskeyIntent.NEW_WORKSPACE }, enabled = !authentication.working && !state.busy) { Text(str(R.string.join_create)) }
                                }
                            }
                            InternetIdentityButton(primary = state.session?.passkeyAvailable != true && state.session?.fastUnlock != true, enabled = !state.busy) { client.login(open = openLogin) }
                            Text(if (state.session?.fastUnlock == true) str(R.string.text_local_unlock_is_independent_of_online_sign_in_expiry_5c2417d)
                                else if (state.session?.passkeyAvailable == true) str(R.string.signin_saved_passkey)
                                else str(R.string.text_sign_in_opens_your_system_browser_content_access_still_requires_a_6a9bef5), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
                if (authentication.canContinue) {
                    Notice(str(R.string.text_continue_the_saved_attempt_without_creating_another_passkey_18a36a6))
                    Button(onClick = client::continuePasskey, enabled = !authentication.working) { Text(str(R.string.text_continue_registration_or_binding_e6a67ca)) }
                }
                if (authentication.canResetSessions) TextButton(onClick = { confirmingReset = true }) { Text(str(R.string.text_sign_out_this_passkey_s_devices_2927dca)) }
                if (!anotherAccount) TextButton(onClick = { choosingAccount = true }, enabled = !state.busy) { Text(str(R.string.text_saved_accounts_e314a1f)) }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = { choosingLanguage = true }, modifier = Modifier.testTag("open-language")) { Text(str(R.string.app_language)) }
                    TextButton(onClick = { information = "privacy" }) { Text(str(R.string.text_privacy_policy_d216307)) }
                    TextButton(onClick = { information = "about" }) { Text(str(R.string.text_about_and_support_b62fe45)) }
                }
                if (state.busy || authentication.working) LinearProgressIndicator(Modifier.fillMaxWidth())
            }
        }
    }
    if (choosingLanguage) LanguagePicker { choosingLanguage = false }
    information?.let { PublicInformationDialog(it, { information = null }, browser) }
    if (choosingAccount) androidx.compose.ui.window.Dialog(onDismissRequest = { choosingAccount = false }, properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().systemBarsPadding()) {
                TextButton(onClick = { choosingAccount = false }) { Text(str(R.string.text_close_6cf4a77)) }
                Box(Modifier.weight(1f)) { AccountsScreen(client) { choosingAccount = false } }
            }
        }
    }
    if (confirmingReset) AlertDialog(onDismissRequest = { confirmingReset = false }, title = { Text(str(R.string.text_make_room_for_this_device_93587ec)) },
        text = { Text(str(R.string.text_use_this_same_passkey_to_stop_renewal_on_its_other_devices_alread_9578651)) },
        confirmButton = { TextButton(onClick = { confirmingReset = false; client.resetPasskeySessions() }) { Text(str(R.string.text_continue_fd19155)) } },
        dismissButton = { TextButton(onClick = { confirmingReset = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
