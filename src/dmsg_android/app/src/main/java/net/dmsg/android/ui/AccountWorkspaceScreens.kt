package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.BuildConfig
import net.dmsg.android.core.*

@Composable fun AccountIdentityHeader(client: ClientController, openLogin: (String) -> Unit) {
    val state by client.state.collectAsStateWithLifecycle()
    val names by client.accountNames.collectAsStateWithLifecycle()
    val image by client.profile.bitmap.collectAsStateWithLifecycle()
    val profile by client.profile.saved.state.collectAsStateWithLifecycle()
    val account = state.session?.account.orEmpty()
    LaunchedEffect(account, state.epoch) { client.profile.load() }
    val name = if (profile.valueOrNull() != null) profile.valueOrNull()?.name?.takeIf(String::isNotBlank) else names[account]
    val identity: @Composable (Modifier) -> Unit = { modifier ->
        Row(modifier.clickable(role = Role.Button) { client.navigation.push(Route("profile")) }.padding(vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Avatar(name.orEmpty(), account, size = 56.dp, image = remember(image) { image?.asImageBitmap() }, icon = if (name == null) painterResource(R.drawable.glyph_profile) else null)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(name ?: str(R.string.text_your_account_ae9d708), style = MaterialTheme.typography.titleLarge)
                Text(str(R.string.text_public_profile_a67b108), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
            }
        }
    }
    Surface(Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.card) {
        BoxWithConstraints(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
            if (maxWidth >= 280.dp && LocalDensity.current.fontScale < 1.3f) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    identity(Modifier.weight(1f))
                    MembershipBadge(client)
                }
            } else Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                identity(Modifier.fillMaxWidth())
                MembershipBadge(client)
            }
        }
    }
    val loginExpired = state.session?.loginExpiresAtMs?.let { it <= System.currentTimeMillis().toULong() } == true
    if (loginExpired) {
        Notice(str(R.string.text_online_sign_in_has_expired_local_content_remains_available_b845b15))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = { client.passkey() }, enabled = !state.busy) { Text(str(R.string.text_sign_in_with_passkey_0c861d7)) }
            TextButton(onClick = { client.login(open = openLogin) }, enabled = !state.busy) { Text(str(R.string.text_use_internet_identity_cc89ec7)) }
        }
    }
}

@Composable fun AccountsScreen(client: ClientController, onSelected: (() -> Unit)? = null) {
    val entries by client.accountWorkspace.accounts.state.collectAsStateWithLifecycle()
    val names by client.accountNames.collectAsStateWithLifecycle()
    val notices by client.notifications.status.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    LaunchedEffect(state.epoch) { client.accountWorkspace.accounts() }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding()) {
        item { ReadFeedback(entries) { client.accountWorkspace.accounts() } }
        items(entries.valueOrNull().orEmpty(), key = { it.id }) { account ->
            val local = account.id == LOCAL_ACCOUNT
            val name = if (local) str(R.string.offline_account) else names[account.id]
            val count = notices.valueOrNull()?.accounts?.find { it.account == account.id }?.counts?.total ?: 0uL
            val canChoose = !account.current || onSelected != null
            ListItem(
                modifier = if (canChoose) Modifier.clickable(enabled = !state.busy, role = Role.Button) {
                    if (!account.current) client.switchAccount(account.id)
                    onSelected?.invoke()
                } else Modifier,
                leadingContent = { Icon(Icons.Outlined.Person, null) },
                headlineContent = { Text(name ?: account.id) },
                supportingContent = {
                    Column {
                        if (!local && name != null) Text(account.id)
                        if (account.moved) Text(str(R.string.text_copied_workspace_device_authorization_required_b552490))
                        else if (!account.current) {
                            val action = if (account.bound) R.string.text_unlock_workspace_ec9cb2a else R.string.text_authorize_this_device_3a0243f
                            Text(str(action))
                        }
                    }
                },
                trailingContent = {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (count > 0uL) Badge { Text(if (count > 99uL) "99+" else localizedNumber(count)) }
                        if (account.current) Icon(Icons.Outlined.Check, str(R.string.text_current_account_59c0362), tint = MaterialTheme.colorScheme.primary)
                    }
                })
        }
        if (entries is ReadState.Empty) item { EmptyState(str(R.string.text_no_saved_accounts_f8486b6), glyph = R.drawable.glyph_account) }
        if ((entries is ReadState.Ready || entries is ReadState.Empty) && entries.valueOrNull()?.none { it.id == LOCAL_ACCOUNT } == true) item {
            OutlinedButton(onClick = { client.openLocal(); onSelected?.invoke() }, enabled = !state.busy) { Text(str(R.string.use_on_device)) }
        }
        item {
            OutlinedButton(onClick = { onSelected?.invoke(); client.showAccountInvitation(true) }) { Text(str(R.string.access_existing_account)) }
            Button(onClick = { client.signInToAnotherAccount(); onSelected?.invoke() }, enabled = !state.busy) {
                Text(str(R.string.access_sign_in_another))
            }
        }
    }
}

@Composable internal fun LocalContentPanel(value: LocalContent, reviewUnbacked: (() -> Unit)? = null) {
    DetailCard {
        Text(str(R.string.text_only_on_this_device_6efee99), style = MaterialTheme.typography.titleMedium)
        LabelValue(str(R.string.text_changes_waiting_to_sync_344814b), localizedNumber(value.pending))
        LabelValue(str(R.string.text_unbacked_file_versions_f59bc2a), localizedNumber(value.unbacked))
        LabelValue(str(R.string.text_unbacked_file_size_ba4e1e4), fileSize(value.unbackedBytes))
        if (value.unbacked > 0u && reviewUnbacked != null) TextButton(onClick = reviewUnbacked) { Text(str(R.string.without_backup)) }
        LabelValue(str(R.string.text_local_file_versions_3f6a112), localizedNumber(value.versions))
        LabelValue(str(R.string.text_last_successful_sync_de178f0), value.lastSync?.let(::displayTime) ?: str(R.string.text_not_recorded_f28a3d3))
    }
}

@Composable fun RemoveLocalAccountScreen(client: ClientController) {
    val local by client.accountWorkspace.local.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var confirming by remember { mutableStateOf(false) }
    val account = state.session?.account.orEmpty()
    var counts by remember(account) { mutableStateOf<Pair<ULong,ULong>?>(null) }
    LaunchedEffect(state.epoch) {
        client.accountWorkspace.local()
        if (account == LOCAL_ACCOUNT) client.perform({ it.localAccountCounts().decode().objectValue() }, key = "local:counts") { counts = it.requiredUnsigned("vault") to it.requiredUnsigned("files") }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        if (account == LOCAL_ACCOUNT) {
            Text(str(R.string.offline_account))
            counts?.let { LabelValue(str(R.string.text_vault_ede2a06), localizedNumber(it.first)); LabelValue(str(R.string.text_files_c79ddc2), localizedNumber(it.second)) }
        } else SelectionContainer { Text(account) }
        Notice(str(R.string.text_this_removes_this_account_s_encrypted_workspace_and_saved_login_f_74b5e96))
        ReadFeedback(local, client.accountWorkspace::local)
        local.valueOrNull()?.let { LocalContentPanel(it) { client.navigation.push(Route("file-unbacked")) } }
        Button(onClick = { confirming = true }, enabled = local is ReadState.Ready && !state.busy && (account != LOCAL_ACCOUNT || counts != null), colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)) { Text(str(R.string.text_remove_this_account_s_local_data_a2189f8)) }
    }
    if (confirming) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_remove_local_data_permanently_46fc877)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) { SelectionContainer { Text(account) }; Text(str(R.string.text_anything_that_exists_only_in_this_copy_cannot_be_recovered_from_t_838c951)) }
    }, confirmButton = { TextButton(onClick = { confirming = false; client.accountWorkspace.remove() }) { Text(str(R.string.text_remove_local_data_c45125e), color = MaterialTheme.colorScheme.error) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable fun DisplayCacheScreen(client: ClientController) {
    val usage by client.accountWorkspace.cache.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var confirming by remember { mutableStateOf(false) }
    LaunchedEffect(state.epoch) { client.accountWorkspace.cache() }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Notice(str(R.string.text_this_clears_encrypted_display_copies_that_can_be_refreshed_vault_7af3f03))
        ReadFeedback(usage, client.accountWorkspace::cache)
        usage.valueOrNull()?.let { LabelValue(str(R.string.text_cached_data_on_this_device_0b0207b), fileSize(it.bytes)) }
        OutlinedButton(onClick = { confirming = true }, enabled = usage is ReadState.Ready && (usage.valueOrNull()?.bytes ?: 0u) > 0u && !state.working("account:clear-cache")) { Text(str(R.string.text_clear_cached_data_70df391)) }
    }
    if (confirming) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_clear_display_copies_a1c040d)) }, text = { Text(str(R.string.text_you_may_need_a_connection_to_refresh_them_next_time_3141771)) }, confirmButton = { TextButton(onClick = { confirming = false; client.accountWorkspace.clearCache() }) { Text(str(R.string.text_clear_cf745bc)) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable fun PublicProfileScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    val saved by client.profile.saved.state.collectAsStateWithLifecycle()
    val publication by client.profile.publication.state.collectAsStateWithLifecycle()
    val form by client.profile.form.collectAsStateWithLifecycle()
    val image by client.profile.bitmap.collectAsStateWithLifecycle()
    val conflict by client.profile.conflict.collectAsStateWithLifecycle()
    val failure by client.profile.error.collectAsStateWithLifecycle()
    val devices by client.account.devices.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val picker = LocalFilePickers.current
    val pending = publication.valueOrNull()?.let { !it.published || it.pending } == true
    val working = state.working("profile:publish") || state.working("profile:prepare-photo")
    val editable = saved.valueOrNull() != null && !working && !pending
    var confirming by remember { mutableStateOf(false) }
    LaunchedEffect(state.epoch) { client.profile.loadEditor() }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            ReadFeedback(saved, { client.profile.loadEditor(true) })
            state.session?.account?.let { account ->
                SelectionContainer { Text(account, style = MaterialTheme.typography.bodySmall) }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = { client.clipboard.copy(account) }) { Text(str(R.string.text_copy_account_id_7bcff24)) }
                    TextButton(onClick = { client.beginExternalInteraction(); context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, "https://dmsg.net/" + account) }, null)) }) { Text(str(R.string.text_share_public_profile_e6eaf6b)) }
                }
            }
            Notice(str(R.string.text_this_profile_is_public_only_publish_details_you_want_others_to_se_6415611))
            OutlinedTextField(form.name, { client.profile.edit(form.copy(name = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_name_dd4dc4c)) }, singleLine = true, enabled = editable, supportingText = { Text(localizedNumber(form.name.codePointCount(0, form.name.length)) + " / " + localizedNumber(80)) })
            OutlinedTextField(form.about, { client.profile.edit(form.copy(about = it)) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_about_1816eae)) }, minLines = 3, enabled = editable, supportingText = { Text(localizedNumber(form.about.codePointCount(0, form.about.length)) + " / " + localizedNumber(2000)) })
            Text(str(R.string.text_public_avatar_4f7d2b1), style = MaterialTheme.typography.titleMedium)
            image?.let {
                // Show every pixel that will be published, including outside the circular header crop.
                Surface(Modifier.fillMaxWidth().height(180.dp), shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainer) {
                    Image(it.asImageBitmap(), str(R.string.text_complete_image_to_publish_76c85fa), Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
                }
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = picker.profilePhoto, enabled = editable) { Text(str(R.string.text_choose_avatar_b061127)) }
                if (form.avatar != null || image != null) TextButton(onClick = client.profile::removePhoto, enabled = editable) { Text(str(R.string.text_remove_avatar_9bbff84)) }
            }
            Text(str(R.string.text_a_chosen_image_stays_local_until_you_publish_published_images_are_d7df855), style = MaterialTheme.typography.bodySmall)
            Text(str(R.string.text_links_0974fd3), style = MaterialTheme.typography.titleMedium)
            form.links.forEachIndexed { index, link ->
                OutlinedTextField(link.name, { text -> client.profile.edit(form.copy(links = form.links.mapIndexed { i, item -> if (i == index) item.copy(name = text) else item })) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_link_name_6536bde)) }, enabled = editable)
                OutlinedTextField(link.url, { text -> client.profile.edit(form.copy(links = form.links.mapIndexed { i, item -> if (i == index) item.copy(url = text) else item })) }, Modifier.fillMaxWidth().revealFocusedInput(), label = { Text(str(R.string.text_https_url_c761858)) }, enabled = editable, singleLine = true)
                TextButton(onClick = { client.profile.edit(form.copy(links = form.links.filterIndexed { i, _ -> i != index })) }, enabled = editable) { Text(str(R.string.text_remove_link_8556d0e)) }
            }
            TextButton(onClick = { client.profile.edit(form.copy(links = form.links + ProfileLink("", "https://"))) }, enabled = editable && form.links.size < 100) { Text(str(R.string.text_add_link_3b75d20)) }
            if (!form.valid) Notice(str(R.string.text_check_field_lengths_and_use_named_https_links_a6e7f91), true)
            if (devices.valueOrNull()?.canManage == false) Notice(str(R.string.text_an_administrator_device_is_required_to_publish_this_profile_3a6fada))
            ReadFeedback(devices, client.account::devices)
            ReadFeedback(publication, client.profile::status)
            failure?.let { Notice(errorText(it), true) }
            if (conflict) {
                Notice(str(R.string.text_the_saved_profile_changed_your_draft_is_kept_reload_the_saved_pro_22fbc85), true)
                TextButton(onClick = { client.profile.loadEditor(true, rebase = true) }, enabled = !working) { Text(str(R.string.text_reload_saved_profile_bcf1663)) }
                saved.valueOrNull()?.let { Text(str(R.string.text_currently_saved_d79fd8f) + it.name.orEmpty()); Text(it.about.orEmpty()) }
            }
            if (pending) Notice(str(R.string.text_the_original_profile_is_saved_or_awaiting_confirmation_directory_fb8bd6f))
        }
        HorizontalDivider()
        Button(onClick = { if (pending) client.profile.retryPublication() else confirming = true }, enabled = !working && if (pending) true else client.profile.ready(), modifier = Modifier.fillMaxWidth().padding(20.dp, 12.dp)) { Text(if (pending) str(R.string.text_retry_publication_1d487c8) else str(R.string.text_publish_profile_5b8d8d6)) }
    }
    if (confirming) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_publish_these_details_publicly_efc379b)) }, text = { Text(str(R.string.text_your_name_description_avatar_and_listed_links_will_be_public_1025600)) }, confirmButton = { TextButton(onClick = { confirming = false; client.profile.publish() }) { Text(str(R.string.text_publish_88bb01f)) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
