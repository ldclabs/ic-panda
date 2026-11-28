package net.dmsg.android.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.NearbyDiscoveredPeer
import net.dmsg.android.platform.str
import net.dmsg.android.ui.kit.*
import net.dmsg.core.SecretAttachmentSelection
import net.dmsg.core.SecretSnapshotInput
import net.dmsg.core.VaultItem

@Composable fun NearbyReceiveScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    val status by client.nearby.status.collectAsStateWithLifecycle()
    val error by client.nearby.error.collectAsStateWithLifecycle()
    val localDenied by client.nearby.localNetworkDenied.collectAsStateWithLifecycle()
    val receive = status?.receive
    val active = receive?.active == true
    val busy = state.working("nearby:receive:start") || state.working("nearby:receive:decision")

    var discoverable by remember { mutableStateOf(false) }
    val shareIdentity = remember { mutableStateMapOf<String, Boolean>() }
    var folder by remember { mutableStateOf("root") }
    var folderName by remember { mutableStateOf("") }
    var choosingFolder by remember { mutableStateOf(false) }
    val selectedFiles = remember { mutableStateMapOf<String, Set<UInt>>() }

    val view = LocalView.current
    DisposableEffect(view, active) { view.keepScreenOn = active; onDispose { view.keepScreenOn = false } }
    LaunchedEffect(state.epoch) {
        if (receive == null || (!receive.active && receive.state != "done")) {
            client.nearby.startReceive(discoverable)
        }
    }
    DisposableEffect(Unit) {
        onDispose {
            if (client.nearby.status.value?.receive?.active == true) {
                client.nearby.cancelReceive()
            }
        }
    }

    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val banner = when {
                error != null -> errorText(error!!) to true
                localDenied -> str(R.string.nearby_local_network_denied) to true
                receive?.error != null -> errorText(receive.error.code) to true
                else -> null
            }
            banner?.let { (message, warning) -> Notice(message, warning) }

            if (receive == null) {
                DetailCard {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(str(R.string.nearby_preparing_receive), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            } else when (receive.state) {
                "waiting" -> {
                    QrCode(receive.offer)
                    DetailCard {
                        LabelValue(
                            str(R.string.nearby_connection),
                            str(if (receive.relayReady) R.string.nearby_local_relay_ready else R.string.nearby_local_only),
                        )
                        CopyButton(str(R.string.nearby_copy_code), outlined = true, key = receive.offer) {
                            client.clipboard.copy(receive.offer)
                        }
                    }
                    Section(footer = str(R.string.nearby_qr_only_hint)) {
                        SettingToggle(
                            title = str(R.string.nearby_allow_discovery),
                            checked = discoverable,
                            change = { next ->
                                discoverable = next
                                client.nearby.startReceive(next)
                            },
                            enabled = !busy,
                            icon = Icons.Outlined.WifiTethering,
                        )
                    }
                    receive.verificationCodes.forEach { sas ->
                        DetailCard {
                            Text(str(R.string.nearby_verification_code), style = MaterialTheme.typography.bodyMedium)
                            Text(groupedNearbySas(sas), style = MaterialTheme.typography.headlineMedium.tabular())
                        }
                    }
                    if (receive.pendingPeers.isEmpty() && receive.verificationCodes.isEmpty()) {
                        DetailCard {
                            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                                Text(str(R.string.nearby_waiting_sender), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    } else {
                        receive.pendingPeers.forEach { peer ->
                            val chosen = selectedFiles[peer.peerId] ?: peer.files.map { it.index }.toSet()
                            DetailCard {
                                Text(str(R.string.nearby_incoming_transfer), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                                Text(peer.alias ?: str(R.string.nearby_device), style = MaterialTheme.typography.titleMedium)
                                Text(groupedNearbySas(peer.sas), style = MaterialTheme.typography.headlineMedium.tabular(), color = MaterialTheme.colorScheme.primary)
                                Text(
                                    str(if (peer.qrBound) R.string.nearby_qr_verified_hint else R.string.nearby_unverified_discovery_hint),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            if (peer.kind == "files") {
                                Section(title = str(R.string.nearby_files_to_send)) {
                                    peer.files.forEach { file ->
                                        SelectionRow(
                                            title = file.name,
                                            checked = file.index in chosen,
                                            enabled = !busy,
                                            subtitle = fileSize(file.size),
                                        ) { checked ->
                                            selectedFiles[peer.peerId] = if (checked) chosen + file.index else chosen - file.index
                                        }
                                    }
                                    ListRow(
                                        headline = str(R.string.text_destination_folder_2f893c8).trimEnd(' ', '·', ':'),
                                        supporting = folderName.ifBlank { str(R.string.text_files_c79ddc2) },
                                        icon = Icons.Outlined.Folder,
                                        enabled = !busy,
                                    ) { choosingFolder = true }
                                    SettingToggle(
                                        title = str(R.string.nearby_share_identity),
                                        checked = shareIdentity[peer.peerId] == true,
                                        change = { shareIdentity[peer.peerId] = it },
                                        enabled = !busy,
                                    )
                                }
                            } else {
                                Section {
                                    InfoRow(str(R.string.nearby_vault_snapshot), fileSize(peer.packageSize ?: 0uL))
                                    SettingToggle(
                                        title = str(R.string.nearby_share_identity),
                                        checked = shareIdentity[peer.peerId] == true,
                                        change = { shareIdentity[peer.peerId] = it },
                                        enabled = !busy,
                                    )
                                }
                            }
                            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                                OutlinedButton(
                                    onClick = { client.nearby.reject(peer.peerId) },
                                    enabled = !busy,
                                    modifier = Modifier.weight(1f),
                                ) { Text(str(R.string.text_reject_52e8be4)) }
                                Button(
                                    onClick = {
                                        val picks = if (peer.kind == "files") peer.files.map { it.index }.filter { it in chosen } else emptyList()
                                        client.nearby.accept(peer.peerId, folder, picks, shareIdentity[peer.peerId] == true)
                                    },
                                    enabled = !busy && (peer.kind != "files" || chosen.isNotEmpty()),
                                    modifier = Modifier.weight(1f),
                                ) { Text(str(R.string.nearby_accept)) }
                            }
                        }
                    }
                }
                "running" -> {
                    DetailCard {
                        Text(str(R.string.nearby_receiving), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                        receive.transport?.let { LabelValue(str(R.string.nearby_transport), it.uppercase()) }
                        val fraction = if (receive.progress.totalBytes == 0uL) 0f else (receive.progress.bytes.toDouble() / receive.progress.totalBytes.toDouble()).toFloat()
                        Progress(fraction)
                        receive.progress.currentName?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                        Text(
                            fileSize(receive.progress.bytes) + " / " + fileSize(receive.progress.totalBytes),
                            style = MaterialTheme.typography.labelSmall.tabular(),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                "done" -> {
                    DetailCard {
                        Text(str(R.string.nearby_transfer_complete), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                        receive.transport?.let { LabelValue(str(R.string.nearby_transport), it.uppercase()) }
                        val outcome = receive.outcome
                        if (outcome?.kind == "secret_package" && outcome.packagePreview != null) {
                            Text(str(R.string.nearby_vault_verified_hint), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            Button(
                                onClick = { client.secretSharing.openNearbyPackage(outcome.packagePreview) },
                                modifier = Modifier.fillMaxWidth(),
                            ) { Text(str(R.string.nearby_review_import_snapshot)) }
                        }
                    }
                    val files = receive.outcome?.files.orEmpty()
                    if (files.isNotEmpty()) {
                        Section(title = str(R.string.text_files_c79ddc2)) {
                            files.forEach { file ->
                                ListRow(
                                    headline = file.name,
                                    supporting = fileSize(file.size),
                                    glyph = R.drawable.glyph_file,
                                ) { client.navigation.push(Route("files", id = file.fileId)) }
                            }
                        }
                    }
                    OutlinedButton(
                        onClick = { client.nearby.startReceive(discoverable) },
                        enabled = !busy,
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(str(R.string.nearby_receive_more)) }
                }
                else -> {
                    Button(
                        onClick = { client.nearby.startReceive(discoverable) },
                        enabled = !busy,
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(str(if (receive.state == "failed") R.string.nearby_start_again else R.string.nearby_start_receiving)) }
                }
            }
        }
        if (active) {
            BottomActionBar {
                OutlinedButton(onClick = client.nearby::cancelReceive, modifier = Modifier.fillMaxWidth()) {
                    Text(str(R.string.text_cancel_998b9c4))
                }
            }
        }
    }
    if (choosingFolder) FileFolderDialog(client, str(R.string.text_files_c79ddc2), { choosingFolder = false }) { path ->
        folder = path.folderId
        folderName = path.lastOrNull()?.name ?: str(R.string.text_files_c79ddc2)
        choosingFolder = false
    }
}

@Composable fun NearbySendFilesScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    val status by client.nearby.status.collectAsStateWithLifecycle()
    val peers by client.nearby.peers.collectAsStateWithLifecycle()
    val browsing by client.nearby.browsing.collectAsStateWithLifecycle()
    val error by client.nearby.error.collectAsStateWithLifecycle()
    val localDenied by client.nearby.localNetworkDenied.collectAsStateWithLifecycle()
    val files by client.nearby.selectedFiles.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()

    var previousSend by remember { mutableStateOf(status?.send?.takeUnless { it.active }?.id) }
    val send = status?.send?.takeUnless { it.id == previousSend }
    val active = send?.active == true
    val busy = state.working("nearby:send:start") || state.working("nearby:send:sas")

    var offerText by remember { mutableStateOf("") }
    var shareIdentity by remember { mutableStateOf(false) }

    val view = LocalView.current
    DisposableEffect(view, active) { view.keepScreenOn = active; onDispose { view.keepScreenOn = false } }
    DisposableEffect(Unit) {
        client.nearby.startBrowse()
        onDispose {
            client.nearby.stopBrowse()
            if (client.nearby.status.value?.send?.active == true) client.nearby.cancelSend()
        }
    }
    LaunchedEffect(qr.result) {
        client.qr.consume("nearby-send-files")?.let { scanned ->
            offerText = scanned
            if (files.isNotEmpty()) client.nearby.sendFiles(scanned, null, files, shareIdentity)
        }
    }

    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val banner = when {
                error != null -> errorText(error!!) to true
                localDenied -> str(R.string.nearby_local_network_denied) to true
                else -> null
            }
            banner?.let { (message, warning) -> Notice(message, warning) }

            if (send != null && (send.active || send.state in setOf("done", "failed"))) {
                NearbySendProgressPanel(client, send)
                if (send.state == "failed") {
                    Button(onClick = { previousSend = send.id }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.nearby_start_again)) }
                }
            } else {
                if (files.isEmpty()) {
                    EmptyState(str(R.string.text_choose_a_file_2916860), glyph = R.drawable.glyph_file)
                } else {
                    Section(title = str(R.string.nearby_files_to_send)) {
                        files.forEach { file ->
                            ListRow(headline = file.name, supporting = fileSize(file.size), glyph = R.drawable.glyph_file)
                        }
                    }
                    NearbyTargetPicker(
                        peers = peers,
                        browsing = browsing,
                        busy = busy,
                        offerText = offerText,
                        onOfferChange = { offerText = it.take(4096) },
                        shareIdentity = shareIdentity,
                        onShareIdentityChange = { shareIdentity = it },
                        onScanQr = { client.qr.open("nearby-send-files") },
                        onSendOffer = { client.nearby.sendFiles(offerText.trim(), null, files, shareIdentity) },
                        onSendPeer = { peer -> client.nearby.sendFiles(null, peer, files, shareIdentity) },
                    )
                }
            }
        }
        if (active) {
            BottomActionBar {
                OutlinedButton(onClick = client.nearby::cancelSend, modifier = Modifier.fillMaxWidth()) {
                    Text(str(R.string.text_cancel_998b9c4))
                }
            }
        }
    }
}

@Composable fun NearbySendVaultScreen(client: ClientController, itemId: String) {
    val state by client.state.collectAsStateWithLifecycle()
    val status by client.nearby.status.collectAsStateWithLifecycle()
    val peers by client.nearby.peers.collectAsStateWithLifecycle()
    val browsing by client.nearby.browsing.collectAsStateWithLifecycle()
    val error by client.nearby.error.collectAsStateWithLifecycle()
    val localDenied by client.nearby.localNetworkDenied.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()

    var previousSend by remember { mutableStateOf(status?.send?.takeUnless { it.active }?.id) }
    val send = status?.send?.takeUnless { it.id == previousSend }
    val active = send?.active == true
    val busy = state.working("nearby:send:start") || state.working("nearby:send:sas")

    var source by remember(itemId) { mutableStateOf<Pair<VaultItem, SecretAttachments>?>(null) }
    var loadError by remember(itemId) { mutableStateOf<String?>(null) }
    var selectedFields by remember(itemId) { mutableStateOf<Set<String>>(emptySet()) }
    var includeNotes by remember(itemId) { mutableStateOf(false) }
    var selectedAttachments by remember(itemId) { mutableStateOf<Set<String>>(emptySet()) }
    var offerText by remember { mutableStateOf("") }
    var shareIdentity by remember { mutableStateOf(false) }

    val view = LocalView.current
    DisposableEffect(view, active) { view.keepScreenOn = active; onDispose { view.keepScreenOn = false } }
    LaunchedEffect(itemId, state.epoch) {
        loadError = null
        client.perform({ core ->
            val item = core.vaultItem(itemId, null).item
            val attachments = parseSecretAttachments(core.secretItemAttachments(itemId).decode())
            check(item.revision == attachments.revision) { "VERSION_CONFLICT" }
            check(!item.reserved && !item.deleted && !item.conflict) { "FORBIDDEN" }
            item to attachments
        }, key = "nearby:vault:source:" + itemId, replace = true, quiet = true, onError = { loadError = it }) {
            val (item, attachments) = it
            val firstLoad = source == null
            source = it
            if (firstLoad) {
                selectedFields = item.fields.filter { f -> f.value.isNotEmpty() }.mapTo(linkedSetOf()) { f -> f.id }
                includeNotes = item.notes.isNotEmpty()
                selectedAttachments = attachments.files.filter { a -> a.selectable }.mapTo(linkedSetOf()) { a -> a.id }
            }
        }
    }
    DisposableEffect(Unit) {
        client.nearby.startBrowse()
        onDispose { client.nearby.stopBrowse() }
    }

    fun buildInput(): SecretSnapshotInput? {
        val (item, attachments) = source ?: return null
        if (selectedFields.isEmpty() && !includeNotes && selectedAttachments.isEmpty()) return null
        val chosen = selectedAttachments.mapNotNull { id ->
            val entry = attachments.files.singleOrNull { it.id == id } ?: return@mapNotNull null
            if (!entry.selectable) return@mapNotNull null
            SecretAttachmentSelection(id, entry.details!!.file.version)
        }
        return SecretSnapshotInput(
            itemId = item.id,
            revision = item.revision,
            fieldIds = item.fields.map { it.id }.filter(selectedFields::contains),
            includeNotes = includeNotes,
            attachments = chosen,
            expiresInMs = null,
            maxDownloadSessions = null,
        )
    }

    LaunchedEffect(qr.result) {
        client.qr.consume("nearby-send-vault")?.let { scanned ->
            offerText = scanned
            buildInput()?.let { input -> client.nearby.sendVault(scanned, null, input, shareIdentity) }
        }
    }

    val hasSelection = selectedFields.isNotEmpty() || includeNotes || selectedAttachments.isNotEmpty()

    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val banner = when {
                loadError != null -> errorText(loadError!!) to true
                error != null -> errorText(error!!) to true
                localDenied -> str(R.string.nearby_local_network_denied) to true
                else -> null
            }
            banner?.let { (message, warning) -> Notice(message, warning) }

            if (send != null && (send.active || send.state in setOf("done", "failed"))) {
                NearbySendProgressPanel(client, send)
                if (send.state == "failed") {
                    Button(onClick = { previousSend = send.id }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.nearby_start_again)) }
                }
            } else {
                val loaded = source
                if (loaded == null) {
                    if (loadError == null) LinearProgressIndicator(Modifier.fillMaxWidth())
                } else {
                    val (item, attachments) = loaded
                    DetailCard {
                        Text(item.title, style = MaterialTheme.typography.titleLarge)
                        Text(
                            str(R.string.text_choose_the_exact_content_of_this_independent_snapshot_editing_the_a26f334),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    val selectableFields = item.fields.filter { it.value.isNotEmpty() }
                    if (selectableFields.isNotEmpty() || item.notes.isNotEmpty()) {
                        val sharesOtp = selectableFields.any { it.kind == "otp" && it.id in selectedFields }
                        Section(
                            title = str(R.string.text_select_fields_to_share_290fe99),
                            footer = if (sharesOtp) str(R.string.text_selected_otp_fields_include_the_reusable_seed_not_just_a_current_4a6ddda) else null,
                        ) {
                            selectableFields.forEach { field ->
                                SelectionRow(
                                    title = vaultFieldTitle(field.id, field.kind, field.label),
                                    checked = field.id in selectedFields,
                                    enabled = !busy,
                                    subtitle = field.section.takeIf(String::isNotEmpty),
                                ) { checked ->
                                    selectedFields = if (checked) selectedFields + field.id else selectedFields - field.id
                                }
                            }
                            if (item.notes.isNotEmpty()) {
                                SelectionRow(str(R.string.text_include_notes_bab8a75), includeNotes, !busy) { includeNotes = it }
                            }
                        }
                    }
                    if (attachments.files.isNotEmpty()) {
                        Section(title = str(R.string.text_select_attachments_to_share_2dbaa0d)) {
                            attachments.files.forEach { entry ->
                                SelectionRow(
                                    title = entry.details?.file?.name ?: str(R.string.text_attachment_unavailable_on_this_device_ede9476),
                                    checked = entry.id in selectedAttachments,
                                    enabled = !busy && entry.selectable,
                                    subtitle = entry.details?.file?.let { fileSize(it.size) } ?: entry.id,
                                ) { checked ->
                                    selectedAttachments = if (checked) selectedAttachments + entry.id else selectedAttachments - entry.id
                                }
                            }
                        }
                    }
                    if (hasSelection) {
                        NearbyTargetPicker(
                            peers = peers,
                            browsing = browsing,
                            busy = busy,
                            offerText = offerText,
                            onOfferChange = { offerText = it.take(4096) },
                            shareIdentity = shareIdentity,
                            onShareIdentityChange = { shareIdentity = it },
                            onScanQr = { client.qr.open("nearby-send-vault") },
                            onSendOffer = { buildInput()?.let { input -> client.nearby.sendVault(offerText.trim(), null, input, shareIdentity) } },
                            onSendPeer = { peer -> buildInput()?.let { input -> client.nearby.sendVault(null, peer, input, shareIdentity) } },
                        )
                    } else {
                        Text(
                            str(R.string.nearby_select_vault_hint),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        if (active) {
            BottomActionBar {
                OutlinedButton(onClick = client.nearby::cancelSend, modifier = Modifier.fillMaxWidth()) {
                    Text(str(R.string.text_cancel_998b9c4))
                }
            }
        }
    }
}

@Composable private fun NearbyTargetPicker(
    peers: List<NearbyDiscoveredPeer>,
    browsing: Boolean,
    busy: Boolean,
    offerText: String,
    onOfferChange: (String) -> Unit,
    shareIdentity: Boolean,
    onShareIdentityChange: (Boolean) -> Unit,
    onScanQr: () -> Unit,
    onSendOffer: () -> Unit,
    onSendPeer: (NearbyDiscoveredPeer) -> Unit,
) {
    DetailCard {
        Text(str(R.string.nearby_receiver_qr_or_code), style = MaterialTheme.typography.titleSmall)
        Button(onClick = onScanQr, enabled = !busy, modifier = Modifier.fillMaxWidth()) {
            Icon(Icons.Outlined.QrCodeScanner, null)
            Spacer(Modifier.width(8.dp))
            Text(str(R.string.nearby_scan_qr))
        }
        OutlinedTextField(
            value = offerText,
            onValueChange = onOfferChange,
            modifier = Modifier.fillMaxWidth(),
            enabled = !busy,
            label = { Text(str(R.string.nearby_paste_code)) },
            minLines = 2,
            maxLines = 4,
            keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
        )
        SettingToggle(
            title = str(R.string.nearby_share_identity),
            checked = shareIdentity,
            change = onShareIdentityChange,
            enabled = !busy,
        )
        OutlinedButton(
            onClick = onSendOffer,
            enabled = !busy && offerText.isNotBlank(),
            modifier = Modifier.fillMaxWidth(),
        ) { Text(str(R.string.nearby_send_via_code)) }
    }
    Section(
        title = str(R.string.nearby_devices),
        footer = str(R.string.nearby_discovery_list_hint),
    ) {
        if (peers.isEmpty()) {
            ListItem(
                headlineContent = { Text(str(R.string.nearby_looking_for_devices), color = MaterialTheme.colorScheme.onSurfaceVariant) },
                leadingContent = if (browsing) ({ CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp) }) else null,
                modifier = Modifier.clipTo(rowShape()),
            )
        } else {
            peers.forEach { peer ->
                ListRow(
                    headline = peer.alias,
                    supporting = peer.pkHint.ifEmpty { peer.host },
                    icon = Icons.Outlined.Devices,
                    enabled = !busy,
                ) { onSendPeer(peer) }
            }
        }
    }
}

@Composable private fun NearbySendProgressPanel(client: ClientController, send: NearbySendStatus) {
    DetailCard {
        Text(str(R.string.nearby_transfer_status), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
        send.sas?.let { sas ->
            Text(str(R.string.nearby_verification_code), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(groupedNearbySas(sas), style = MaterialTheme.typography.headlineMedium.tabular(), color = MaterialTheme.colorScheme.primary)
        }
        send.transport?.let { LabelValue(str(R.string.nearby_transport), it.uppercase()) }
        when (send.state) {
            "connecting" -> {
                LinearProgressIndicator(Modifier.fillMaxWidth())
                Text(str(R.string.nearby_connecting), style = MaterialTheme.typography.bodyMedium)
            }
            "awaiting_sas" -> {
                Text(str(R.string.nearby_confirm_sas_hint), style = MaterialTheme.typography.bodyMedium)
                Button(onClick = client.nearby::confirmSas, modifier = Modifier.fillMaxWidth()) {
                    Text(str(R.string.nearby_codes_match))
                }
            }
            "awaiting_accept" -> {
                LinearProgressIndicator(Modifier.fillMaxWidth())
                Text(str(R.string.nearby_waiting_accept), style = MaterialTheme.typography.bodyMedium)
            }
            "running" -> {
                val fraction = if (send.progress.totalBytes == 0uL) 0f else (send.progress.bytes.toDouble() / send.progress.totalBytes.toDouble()).toFloat()
                Progress(fraction)
                send.progress.currentName?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                Text(
                    fileSize(send.progress.bytes) + " / " + fileSize(send.progress.totalBytes),
                    style = MaterialTheme.typography.labelSmall.tabular(),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            "done" -> {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.CheckCircle, null, tint = MaterialTheme.colorScheme.primary)
                    Text(str(R.string.nearby_sent_success), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                }
            }
            "failed" -> {
                Text(
                    send.error?.code?.let(::errorText) ?: str(R.string.nearby_transfer_failed),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            else -> Text(send.state, style = MaterialTheme.typography.bodyMedium)
        }
    }
}
