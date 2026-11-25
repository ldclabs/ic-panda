package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
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
import androidx.compose.animation.core.Animatable
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
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

/** The mark grows in where the splash screen's icon was. */
@Composable private fun EntryMark() {
    val shown = remember { Animatable(0f) }
    LaunchedEffect(Unit) { shown.animateTo(1f, Motion.spatial()) }
    Image(painterResource(R.drawable.dmsg_mark), null, Modifier.size(72.dp).graphicsLayer {
        val scale = 0.8f + 0.2f * shown.value
        scaleX = scale; scaleY = scale; alpha = shown.value
    })
}

/** Whose workspace is locked: the account's avatar and saved name, or its ID. */
@Composable private fun LockedAccount(account: String, name: String?) {
    Row(Modifier.semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Avatar(name.orEmpty(), account, icon = if (name == null) painterResource(R.drawable.glyph_profile) else null)
        if (name != null) Text(name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        else Text(account, Modifier.widthIn(max = 240.dp), style = MaterialTheme.typography.bodyMedium.code(), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** A plain explanation under the action it belongs to; banners stay for errors. */
@Composable private fun EntryHint(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
}

@Composable private fun EntryLink(label: String, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    TextButton(onClick, modifier, enabled, contentPadding = PaddingValues(horizontal = 8.dp)) { Text(label, style = MaterialTheme.typography.labelMedium) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable internal fun EntryScreen(client: ClientController, state: UiState, browser: (String) -> Unit, openLogin: (String) -> Unit, snackbar: SnackbarHostState) {
    val page by client.access.collectAsStateWithLifecycle()
    val anotherAccount = page == AccessPage.SIGN_IN_ANOTHER
    val authentication by client.authentication.state.collectAsStateWithLifecycle()
    val incoming by client.incoming.intake.collectAsStateWithLifecycle()
    val names by client.accountNames.collectAsStateWithLifecycle()
    var information by remember { mutableStateOf<String?>(null) }
    var choosingAccount by remember { mutableStateOf(false) }
    var choosingLanguage by remember { mutableStateOf(false) }
    var demoOpen by rememberSaveable { mutableStateOf(false) }
    var demoCode by rememberSaveable { mutableStateOf("") }
    var demoError by remember { mutableStateOf<String?>(null) }
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
    Scaffold(containerColor = MaterialTheme.colorScheme.page, snackbarHost = { SnackbarHost(snackbar) }) { padding ->
        if (status == "moved" && !anotherAccount) { Box(Modifier.padding(padding)) { MovedWorkspaceScreen(client) }; return@Scaffold }
        if (page != AccessPage.WELCOME && !anotherAccount) {
            val pageTitle = when (page) {
                AccessPage.JOIN_ACCOUNT -> str(R.string.text_join_an_existing_account_cc7175b)
                AccessPage.PAIRING -> str(R.string.text_authorize_this_device_3a0243f)
                AccessPage.RECOVERY -> str(R.string.text_account_recovery_04aaf7e)
                AccessPage.HANDOVER -> str(R.string.text_replace_a_device_fbe5457)
                else -> ""
            }
            Column(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding)) {
                TopAppBar(
                    title = { Text(pageTitle, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                    navigationIcon = {
                        IconButton(onClick = { client.showAccess(AccessPage.WELCOME) }) {
                            Icon(Icons.AutoMirrored.Outlined.ArrowBack, str(R.string.text_back_5db5cac))
                        }
                    },
                    colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.page),
                )
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
        // The greeting and its one primary action sit centred; quiet links stay at the bottom edge.
        BoxWithConstraints(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
            val window = maxHeight
            Column(Modifier.widthIn(max = 560.dp).fillMaxWidth().verticalScroll(rememberScrollState()).heightIn(min = window).padding(pagePadding()),
                verticalArrangement = Arrangement.SpaceBetween, horizontalAlignment = Alignment.CenterHorizontally) {
                Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (incoming.working) { LinearProgressIndicator(Modifier.fillMaxWidth()); Text(str(R.string.text_encrypting_shared_items_for_later_import_8caa029)) }
                    else if (incoming.count > 0) Notice(str(R.string.text_shared_items_are_waiting_unlock_an_account_to_review_and_import_t_713dc4a))
                    incoming.error?.let { Notice(errorText(it), true) }
                    if (intent != null) IconButton(onClick = { intent = null; client.cancelAuthentication() }) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, str(R.string.text_back_5db5cac)) }
                    else if (anotherAccount) TextButton(onClick = ::backToWelcome, enabled = !state.busy) { Text(str(R.string.text_cancel_998b9c4)) }
                }
                Column(Modifier.fillMaxWidth().padding(vertical = 24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(14.dp)) {
                        EntryMark()
                        Text(str(when (intent) {
                            PasskeyIntent.NEW_WORKSPACE -> R.string.access_new_workspace
                            PasskeyIntent.EXISTING_ACCOUNT -> R.string.access_existing_account
                            null -> when {
                                anotherAccount -> R.string.access_sign_in_another
                                status == "locked" -> R.string.text_welcome_back_70da839
                                else -> R.string.text_keep_what_matters_private_2a3adeb
                            }
                        }), style = MaterialTheme.typography.headlineMedium, textAlign = TextAlign.Center)
                        val account = state.session?.account
                        if (intent == null && !anotherAccount && status == "locked" && account != null) LockedAccount(account, names[account])
                        else Text(str(when (intent) {
                            PasskeyIntent.NEW_WORKSPACE -> R.string.text_your_passkey_will_be_saved_by_your_credential_provider_this_creat_527af1f
                            PasskeyIntent.EXISTING_ACCOUNT -> R.string.access_linked_login
                            null -> if (anotherAccount) R.string.access_linked_login else R.string.text_protect_passwords_notes_and_files_with_end_to_end_encryption_sync_50ec150
                        }), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
                        Spacer(Modifier.height(8.dp))
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
                            Button(onClick = { client.passkey() }, enabled = !authentication.working && !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text(str(R.string.access_existing_passkey)) }
                            EntryHint(str(if (status == "root_required") R.string.pair_preparing_vault else R.string.text_approve_this_phone_from_an_authorized_device_then_continue_here_7cbcaea))
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
                            OutlinedButton(onClick = client::initialize, enabled = !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text(str(if (status == "no_account") R.string.join_create else R.string.access_new_workspace)) }
                            EntryHint(if (slow) str(R.string.text_the_network_is_taking_longer_than_usual_your_setup_progress_is_ke_d703303)
                                else str(if (status == "no_account") R.string.join_scan_hint else R.string.text_create_your_encrypted_workspace_to_start_saving_passwords_notes_a_4982122))
                            TextButton(onClick = client::signInToAnotherAccount, enabled = !state.busy) { Text(str(R.string.text_switch_account_52f71df)) }
                        }
                        authorize -> {
                            Button(onClick = { client.showAccess(AccessPage.PAIRING) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(str(if (status == "root_required") R.string.text_check_authorization_039f8a2 else R.string.text_authorize_this_device_3a0243f)) }
                            EntryHint(str(if (status == "root_required") R.string.pair_preparing_vault else R.string.text_approve_this_phone_from_an_authorized_device_then_continue_here_7cbcaea))
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
                                EntryHint(if (state.session?.fastUnlock == true) str(R.string.text_local_unlock_is_independent_of_online_sign_in_expiry_5c2417d)
                                    else if (state.session?.passkeyAvailable == true) str(R.string.signin_saved_passkey)
                                    else str(R.string.text_sign_in_opens_your_system_browser_content_access_still_requires_a_6a9bef5))
                            }
                        }
                    }
                    if (authentication.canContinue) {
                        Button(onClick = client::continuePasskey, enabled = !authentication.working) { Text(str(R.string.text_continue_registration_or_binding_e6a67ca)) }
                        EntryHint(str(R.string.text_continue_the_saved_attempt_without_creating_another_passkey_18a36a6))
                    }
                    if (authentication.canResetSessions) TextButton(onClick = { confirmingReset = true }) { Text(str(R.string.text_sign_out_this_passkey_s_devices_2927dca)) }
                    if (state.busy || authentication.working) LinearProgressIndicator(Modifier.fillMaxWidth())
                }
                FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp, Alignment.CenterHorizontally)) {
                    if (!anotherAccount) EntryLink(str(R.string.text_saved_accounts_e314a1f), enabled = !state.busy) { choosingAccount = true }
                    if (!anotherAccount) EntryLink(str(R.string.demo_access_link), Modifier.testTag("open-demo-access"), enabled = !state.busy) { demoOpen = true; demoError = null }
                    EntryLink(str(R.string.app_language), Modifier.testTag("open-language")) { choosingLanguage = true }
                    EntryLink(str(R.string.text_privacy_policy_d216307)) { information = "privacy" }
                    EntryLink(str(R.string.text_about_and_support_b62fe45)) { information = "about" }
                }
            }
        }
    }
    if (choosingLanguage) LanguagePicker { choosingLanguage = false }
    information?.let { PublicInformationDialog(it, { information = null }, browser) }
    if (choosingAccount) androidx.compose.ui.window.Dialog(onDismissRequest = { choosingAccount = false }, properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false)) {
        SyncSystemBars()
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.page) {
            Column(Modifier.fillMaxSize()) {
                TopAppBar(
                    title = { Text(str(R.string.text_saved_accounts_e314a1f)) },
                    navigationIcon = {
                        IconButton(onClick = { choosingAccount = false }) {
                            Icon(Icons.Outlined.Close, str(R.string.text_close_6cf4a77))
                        }
                    },
                    colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.page),
                )
                Box(Modifier.weight(1f).navigationBarsPadding()) { AccountsScreen(client) { choosingAccount = false } }
            }
        }
    }
    if (confirmingReset) AlertDialog(onDismissRequest = { confirmingReset = false }, title = { Text(str(R.string.text_make_room_for_this_device_93587ec)) },
        text = { Text(str(R.string.text_use_this_same_passkey_to_stop_renewal_on_its_other_devices_alread_9578651)) },
        confirmButton = { TextButton(onClick = { confirmingReset = false; client.resetPasskeySessions() }) { Text(str(R.string.text_continue_fd19155)) } },
        dismissButton = { TextButton(onClick = { confirmingReset = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (demoOpen) AlertDialog(
        onDismissRequest = { if (!state.busy) { demoOpen = false; demoError = null } },
        title = { Text(str(R.string.demo_access_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(str(R.string.demo_access_description), style = MaterialTheme.typography.bodyMedium)
                OutlinedTextField(
                    value = demoCode,
                    onValueChange = { demoCode = it; demoError = null },
                    modifier = Modifier.fillMaxWidth().testTag("demo-code-input"),
                    label = { Text(str(R.string.demo_code_label)) },
                    singleLine = true,
                    enabled = !state.busy,
                )
                if (state.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                demoError?.let { Text(errorText(it), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    demoError = null
                    client.startDemoWithCode(demoCode.trim(), onError = { demoError = it }) {
                        demoOpen = false
                        demoCode = ""
                        demoError = null
                    }
                },
                enabled = !state.busy && demoCode.isNotBlank(),
                modifier = Modifier.testTag("demo-code-submit"),
            ) { Text(str(R.string.text_continue_fd19155)) }
        },
        dismissButton = {
            TextButton(
                onClick = { demoOpen = false; demoError = null },
                enabled = !state.busy,
            ) { Text(str(R.string.text_cancel_998b9c4)) }
        },
    )
}
