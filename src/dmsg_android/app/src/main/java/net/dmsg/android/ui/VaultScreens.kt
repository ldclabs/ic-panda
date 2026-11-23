package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ShortText
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import net.dmsg.core.*

/** A full-screen dialog: close at the start of the bar, Save at its end, the form scrolls under it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable fun VaultEditor(client: ClientController, close: () -> Unit) {
    val draft by client.vaultDrafts.current.collectAsStateWithLifecycle()
    val ready by client.vaultDrafts.ready.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val library by client.vault.attachmentFiles.state.collectAsStateWithLifecycle()
    val files = library.valueOrNull().orEmpty()
    val fileNames = remember(files) { files.associate { it.id to it.name } }
    var failure by remember { mutableStateOf<String?>(null) }
    var validation by remember { mutableStateOf(false) }
    var pendingFocus by remember { mutableStateOf<String?>(null) }
    var choosingType by remember { mutableStateOf(false) }
    var adding by remember { mutableStateOf(false) }
    var attachment by remember { mutableStateOf(false) }
    var generator by remember { mutableStateOf<String?>(null) }
    val current = draft ?: return
    LaunchedEffect(current.key) { if (current.files.isNotEmpty()) client.vault.loadAttachmentFiles() }
    var customizing by remember(current.key, current.kind) { mutableStateOf(current.kind != "login") }
    val titleFocus = remember { FocusRequester() }
    val tagFocus = remember { FocusRequester() }
    LaunchedEffect(pendingFocus, customizing) {
        if (pendingFocus == "title") { titleFocus.requestFocus(); pendingFocus = null }
        if (pendingFocus == "tags" && customizing) { tagFocus.requestFocus(); pendingFocus = null }
    }
    val focus = androidx.compose.ui.platform.LocalFocusManager.current
    LaunchedEffect(qr.result, current.key) {
        val scanned = qr.result ?: return@LaunchedEffect
        val field = current.fields.firstOrNull { it.kind == "otp" && scanned.target == "otp:" + current.key + ":" + it.id } ?: return@LaunchedEffect
        client.qr.consume(scanned.target)?.let { text ->
            client.vaultDrafts.update { value -> value.field(field.id) { it.copy(value = text, changed = true) } }
        }
    }
    val working = state.working("vault:save:" + current.key) || state.working("vault:discard") || !ready
    fun update(change: (VaultDraft) -> VaultDraft) { client.activity(); if (!working) { failure = null; client.vaultDrafts.update(change) } }
    fun save() {
        validation = true
        val edit = current.edit()
        failure = when {
            edit.title.isBlank() || edit.title.codePointCount(0, edit.title.length) > 200 -> "INVALID_TITLE"
            edit.tags.size > 20 || edit.tags.any { it.codePointCount(0, it.length) > 40 } -> "INVALID_TAGS"
            current.fields.any { validateDraftField(it) != null } -> "INVALID_FIELDS"
            else -> null
        }
        when (failure) {
            "INVALID_TITLE" -> pendingFocus = "title"
            "INVALID_TAGS" -> { customizing = true; pendingFocus = "tags" }
            "INVALID_FIELDS" -> { customizing = true; pendingFocus = current.fields.first { validateDraftField(it) != null }.id }
        }
        if (failure == null) {
            focus.clearFocus()
            val input = client.vaultDrafts.input()
            client.perform({ it.commitVaultDraft(input, edit) }, key = "vault:save:" + current.key, quiet = true, onError = { failure = it }) { saved ->
                client.vaultDrafts.finished(input.id); client.vault.changed(); client.vault.select(saved.item.id)
                if (client.navigation.state.value.current.page == "vault" && (current.id == null && client.vault.query.filtered || current.id != null && current.id != saved.item.id)) client.navigation.select(saved.item.id)
                client.notice(if (saved.item.conflict) "VAULT_CONFLICT_SAVED" else "VAULT_SAVED")
            }
        }
    }
    Column(Modifier.testTag("vault-editor").fillMaxSize()) {
        TopAppBar(
            title = { Text(if (current.id == null) str(R.string.text_new_item_9916f9f) else str(R.string.text_edit_item_3638f4f), maxLines = 1) },
            navigationIcon = { IconButton(onClick = close, enabled = !working) { Icon(Icons.Outlined.Close, str(R.string.text_cancel_998b9c4)) } },
            actions = {
                TextButton(onClick = ::save, enabled = !working && (current.id == null || client.vaultDrafts.dirty), modifier = Modifier.testTag("vault-editor-save")) {
                    if (working) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp) else Text(str(R.string.text_save_ec8e6d5))
                }
            },
            colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.page),
        )
        failure?.takeUnless { it in setOf("INVALID_TITLE", "INVALID_TAGS", "INVALID_FIELDS") }?.let { reason ->
            Box(Modifier.padding(horizontal = pageMargin(), vertical = 4.dp)) { Notice(errorText(reason), true) }
        }
        if (!ready) TextButton(onClick = { client.vaultDrafts.load() }, modifier = Modifier.padding(horizontal = pageMargin())) {
            Text(str(R.string.text_retry_563171c))
        }
        Column(Modifier.weight(1f).imePadding().verticalScroll(rememberScrollState()).padding(horizontal = pageMargin()).navigationBarsPadding().padding(bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(current.title, { value -> update { it.copy(title = value) } }, Modifier.testTag("vault-editor-title").focusRequester(titleFocus).revealFocusedInput().fillMaxWidth(), label = { Text(str(R.string.text_title_4c18fa6)) }, singleLine = true, enabled = !working,
                isError = validation && (current.title.isBlank() || current.title.codePointCount(0, current.title.length) > 200),
                supportingText = if (failure == "INVALID_TITLE") { { Text(errorText("INVALID_TITLE")) } } else null)
            if (current.id == null) Box {
                OutlinedButton(onClick = { choosingType = true }, enabled = !working) {
                    Icon(painterResource(vaultGlyph(current.kind)), null, Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text(vaultKindName(current.kind))
                    Spacer(Modifier.width(4.dp))
                    Icon(Icons.Outlined.ArrowDropDown, null)
                }
                DropdownMenu(choosingType, { choosingType = false }) { vaultKinds().forEach { kind ->
                    DropdownMenuItem(
                        text = { Text(vaultKindName(kind)) },
                        leadingIcon = { Icon(painterResource(vaultGlyph(kind)), null) },
                        trailingIcon = if (current.kind == kind) ({ Icon(Icons.Outlined.Check, null) }) else null,
                        onClick = { update { it.switchKind(kind) }; choosingType = false },
                    )
                } }
            }
            current.fields.forEachIndexed { index, field -> key(field.id) {
                var reveal by remember { mutableStateOf(false) }
                var visited by remember { mutableStateOf(false) }
                var focused by remember { mutableStateOf(false) }
                var types by remember { mutableStateOf(false) }
                val error = if (validation || visited) validateDraftField(field) else null
                val part = when {
                    error == "FIELD_NAME_REQUIRED" || error == "FIELD_LABEL_TOO_LONG" && field.label.codePointCount(0, field.label.length) > 100 -> "label"
                    error == "FIELD_LABEL_TOO_LONG" -> "section"
                    else -> "value"
                }
                val labelFocus = remember { FocusRequester() }
                val valueFocus = remember { FocusRequester() }
                val sectionFocus = remember { FocusRequester() }
                LaunchedEffect(pendingFocus, customizing) {
                    if (pendingFocus == field.id && customizing) {
                        when (part) { "label" -> labelFocus; "section" -> sectionFocus; else -> valueFocus }.requestFocus()
                        pendingFocus = null
                    }
                }
                val fieldContent: @Composable () -> Unit = {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (customizing) Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                            Box {
                                TextButton(onClick = { types = true }, enabled = !working && (current.id == null || !field.concealed || field.changed)) {
                                    Icon(fieldKindIcon(field.kind), null, Modifier.size(18.dp))
                                    Spacer(Modifier.width(6.dp))
                                    Text(fieldKindName(field.kind))
                                    Icon(Icons.Outlined.ArrowDropDown, null)
                                }
                                DropdownMenu(types, { types = false }) { fieldKinds.forEach { kind ->
                                    DropdownMenuItem(
                                        text = { Text(fieldKindName(kind)) },
                                        leadingIcon = { Icon(fieldKindIcon(kind), null) },
                                        trailingIcon = if (field.kind == kind) ({ Icon(Icons.Outlined.Check, null) }) else null,
                                        onClick = { update { it.field(field.id) { f -> f.copy(kind = kind, concealed = kind in listOf("concealed", "otp"), changed = true) } }; types = false },
                                    )
                                } }
                            }
                            Row {
                                IconButton(onClick = { update { it.move(field.id, -1) } }, enabled = !working && index > 0) { Icon(Icons.Outlined.ArrowUpward, str(R.string.text_move_field_up_65720e4)) }
                                IconButton(onClick = { update { it.move(field.id, 1) } }, enabled = !working && index < current.fields.lastIndex) { Icon(Icons.Outlined.ArrowDownward, str(R.string.text_move_field_down_cc8899b)) }
                                IconButton(onClick = { update { it.copy(fields = it.fields.filterNot { f -> f.id == field.id }) } }, enabled = !working) { Icon(Icons.Outlined.DeleteOutline, str(R.string.text_remove_field_373cf91)) }
                            }
                        }
                        if (customizing) OutlinedTextField(field.label, { value -> update { it.field(field.id) { f -> f.copy(label = value) } } }, Modifier.fillMaxWidth().focusRequester(labelFocus).revealFocusedInput(),
                            label = { Text(str(R.string.text_field_name_fb3c7fc)) }, placeholder = { Text(vaultFieldTitle(field.id, field.kind, "")) }, enabled = !working, singleLine = true,
                            isError = part == "label", supportingText = if (part == "label" && error != null) { { Text(errorText(error)) } } else null)
                        OutlinedTextField(field.value, { value -> update { it.field(field.id) { f -> f.copy(value = value, changed = true) } } },
                            Modifier.fillMaxWidth().focusRequester(valueFocus).revealFocusedInput().onFocusChanged { focus -> if (focused && !focus.isFocused) visited = true; focused = focus.isFocused },
                            label = { Text(if (customizing) str(R.string.text_value_293fc8c) else vaultFieldTitle(field.id, field.kind, field.label)) }, placeholder = { Text(if (current.id != null && field.concealed && !field.changed) str(R.string.text_leave_empty_to_keep_existing_value_d1cba32) else "") },
                            visualTransformation = if (field.concealed && !reveal) PasswordVisualTransformation() else VisualTransformation.None,
                            keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = when {
                                field.concealed -> KeyboardType.Password
                                field.kind == "email" -> KeyboardType.Email
                                field.kind == "url" -> KeyboardType.Uri
                                field.kind == "phone" -> KeyboardType.Phone
                                else -> KeyboardType.Text
                            }), minLines = if (field.id in listOf("private_key", "public_key", "phrase")) 3 else 1, enabled = !working, isError = error != null && part == "value",
                            supportingText = if (error != null && part == "value") { { Text(errorText(error)) } } else null,
                            trailingIcon = if (!field.concealed) null else { {
                                Row {
                                    if (field.kind != "otp" && field.id !in listOf("number", "cvv", "pin", "private_key", "public_key", "phrase"))
                                        IconButton(onClick = { generator = field.id }, enabled = !working) { Icon(Icons.Outlined.Password, str(R.string.text_generate_password_4fe4cd0)) }
                                    IconButton(onClick = { reveal = !reveal }) {
                                        Icon(if (reveal) Icons.Outlined.VisibilityOff else Icons.Outlined.Visibility, str(if (reveal) R.string.text_hide_draft_a649c8a else R.string.text_show_draft_5987b01))
                                    }
                                }
                            } })
                        if (field.concealed) {
                            if (current.id != null) {
                                Text(if (field.changed) str(R.string.text_an_empty_replacement_clears_the_saved_value_b460222) else str(R.string.text_the_saved_value_will_be_kept_11b9a97), style = MaterialTheme.typography.bodySmall)
                                TextButton(onClick = { update { it.field(field.id) { f -> f.copy(value = "", changed = !field.changed) } } }, enabled = !working) { Text(if (field.changed) str(R.string.text_keep_existing_value_3bf91d5) else str(R.string.text_clear_saved_value_1df3dd1)) }
                            }
                        }
                        if (field.kind in listOf("date", "month")) Text(if (field.kind == "date") "YYYY-MM-DD" else "YYYY-MM", style = MaterialTheme.typography.labelSmall)
                        if (field.kind == "otp") {
                            Text(str(R.string.text_enter_a_setup_key_or_otpauth_url_b3809f4), style = MaterialTheme.typography.bodySmall)
                            TextButton(onClick = { client.qr.open("otp:" + current.key + ":" + field.id) }, enabled = !working) { Text(str(R.string.text_scan_setup_code_8388c0c)) }
                            Text(str(R.string.text_scanned_input_stays_in_this_draft_until_you_review_and_save_it_821789c), style = MaterialTheme.typography.bodySmall)
                        }
                        if (customizing) OutlinedTextField(field.section, { value -> update { it.field(field.id) { f -> f.copy(section = value) } } }, Modifier.fillMaxWidth().focusRequester(sectionFocus).revealFocusedInput(), label = { Text(str(R.string.text_section_optional_9eb10c6)) }, enabled = !working, singleLine = true,
                            isError = part == "section", supportingText = if (part == "section" && error != null) { { Text(errorText(error)) } } else null)
                    }
                }
                if (customizing) DetailCard { fieldContent() } else fieldContent()
            } }
            if (current.kind == "login" || customizing) {
                Section {
                    if (current.kind == "login") SettingToggle(str(R.string.customize_fields), customizing, { customizing = it }, enabled = !working, icon = Icons.Outlined.Tune)
                    if (customizing) SettingToggle(str(R.string.text_favorite_898f4c6), current.favorite, { value -> update { it.copy(favorite = value) } }, enabled = !working, icon = Icons.Outlined.StarBorder)
                }
            }
            if (customizing) {
                OutlinedTextField(current.tags, { value -> update { it.copy(tags = value) } }, Modifier.fillMaxWidth().focusRequester(tagFocus).revealFocusedInput(), label = { Text(str(R.string.text_tags_33e9c4d)) }, isError = failure == "INVALID_TAGS",
                    supportingText = { Text(if (failure == "INVALID_TAGS") errorText("INVALID_TAGS") else str(R.string.text_separate_with_commas_up_to_20_tags_11cf2a5)) }, enabled = !working)
                Box {
                    OutlinedButton(onClick = { adding = true }, enabled = !working && current.fields.size < 64) { Icon(Icons.Outlined.Add, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_add_field_a6fc993)) }
                    DropdownMenu(adding, { adding = false }) { fieldKinds.forEach { kind ->
                        DropdownMenuItem(
                            text = { Text(fieldKindName(kind)) },
                            leadingIcon = { Icon(fieldKindIcon(kind), null) },
                            onClick = { update { it.newField(kind) }; adding = false },
                        )
                    } }
                }
                OutlinedTextField(current.notes, { value -> update { it.copy(notes = value) } }, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_notes_f7df47b)) }, minLines = 4, keyboardOptions = KeyboardOptions(autoCorrectEnabled = false), enabled = !working)
                SectionTitle(str(R.string.text_attachments_be5733e))
                current.files.forEach { id ->
                    val name = fileNames[id] ?: if (library is ReadState.Loading || library is ReadState.Idle) str(R.string.vault_loading) else str(R.string.text_unavailable_4ae32d2) + " · " + id.take(8)
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Text(name, Modifier.weight(1f), style = MaterialTheme.typography.bodySmall)
                        IconButton(onClick = { update { it.copy(files = it.files - id) } }, enabled = !working) { Icon(Icons.Outlined.Close, str(R.string.text_remove_attachment_1096343) + " · " + name) }
                    }
                }
                TextButton(onClick = { client.vault.loadAttachmentFiles(); attachment = true }, enabled = !working && current.files.size < 20) { Text(str(R.string.text_choose_from_files_c1e8160)) }
            }
            if (failure == null && state.session?.accountKind != "local") Text(str(R.string.text_saved_on_this_device_first_then_synchronized_unsynced_changes_can_0921bb4),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
    if (attachment) {
        var term by remember { mutableStateOf("") }
        val matches = remember(files, term) { files.filter { it.name.contains(term.trim(), true) || it.path.joinToString("/").contains(term.trim(), true) } }
        AlertDialog(onDismissRequest = { attachment = false }, title = { Text(str(R.string.text_choose_a_file_3703fda)) }, text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                WorkspaceSearchField(term, { term = it }, str(R.string.vault_search_files))
                ReadFeedback(library, client.vault::loadAttachmentFiles)
                LazyColumn(Modifier.heightIn(max = 320.dp)) {
                    items(matches, key = { it.id }) { file ->
                        val selected = file.id in current.files
                        ListItem(
                            headlineContent = { Text(file.name) },
                            supportingContent = { Text((listOf(str(R.string.text_files_c79ddc2)) + file.path).joinToString(" / ") + " · " + fileSize(file.size)) },
                            leadingContent = { Icon(painterResource(R.drawable.glyph_file), null) },
                            trailingContent = if (selected) ({ Icon(Icons.Outlined.Check, str(R.string.text_selected_fea261f), tint = MaterialTheme.colorScheme.primary) }) else null,
                            colors = ListItemDefaults.colors(containerColor = androidx.compose.ui.graphics.Color.Transparent),
                            modifier = Modifier.clickable(enabled = !selected) { update { it.copy(files = it.files + file.id) }; attachment = false },
                        )
                    }
                    if (matches.isEmpty() && (library is ReadState.Ready || library is ReadState.Empty)) item {
                        Text(str(if (term.isBlank()) R.string.text_import_a_file_in_files_first_d76a295 else R.string.text_no_matching_files_2fc3030))
                    }
                }
            }
        }, confirmButton = { TextButton(onClick = { attachment = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    }
    generator?.let { id -> PasswordGenerator(onDismiss = { generator = null }) { value -> update { it.field(id) { f -> f.copy(value = value, changed = true) } }; generator = null } }
}

private val fieldKinds = listOf("text", "concealed", "url", "email", "phone", "date", "month", "otp")
internal fun fieldKindName(kind: String): String = when (kind) {
    "concealed" -> str(R.string.text_hidden_value_b28dd5f); "url" -> str(R.string.text_website_bbb4521); "email" -> str(R.string.text_email_6d5db1e)
    "phone" -> str(R.string.text_phone_9460aa6); "date" -> str(R.string.text_date_5da2748); "month" -> str(R.string.text_month_350fe84); "otp" -> str(R.string.text_one_time_code_c7b1bc6); else -> str(R.string.text_text_e349a14)
}
internal fun fieldKindIcon(kind: String): ImageVector = when (kind) {
    "concealed" -> Icons.Outlined.Password
    "url" -> Icons.Outlined.Link
    "email" -> Icons.Outlined.Email
    "phone" -> Icons.Outlined.Phone
    "date", "month" -> Icons.Outlined.CalendarToday
    "otp" -> Icons.Outlined.Timer
    else -> Icons.AutoMirrored.Outlined.ShortText
}
@Composable private fun PasswordGenerator(onDismiss: () -> Unit, use: (String) -> Unit) {
    var length by remember { mutableIntStateOf(20) }
    var symbols by remember { mutableStateOf(true) }
    var value by remember { mutableStateOf(generatedPassword(length, symbols)) }
    AlertDialog(onDismissRequest = onDismiss, title = { Text(str(R.string.text_generate_password_4fe4cd0)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            DetailCard { Text(value, style = MaterialTheme.typography.bodyLarge.code()) }
            Text(str(R.string.text_length_889fe99) + localizedNumber(length), style = MaterialTheme.typography.labelLarge)
            Slider(length.toFloat(), { length = it.toInt(); value = generatedPassword(length, symbols) }, valueRange = 12f..64f, steps = 51)
            Section { SettingToggle(str(R.string.text_symbols_04fef1c), symbols, { symbols = it; value = generatedPassword(length, symbols) }) }
            TextButton(onClick = { value = generatedPassword(length, symbols) }) { Icon(Icons.Outlined.Refresh, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_generate_again_c8b3202)) }
        }
    }, confirmButton = { TextButton(onClick = { use(value) }) { Text(str(R.string.text_use_password_b417e48)) } }, dismissButton = { TextButton(onClick = onDismiss) { Text(str(R.string.text_cancel_998b9c4)) } })
}
