package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import net.dmsg.core.*

@Composable fun VaultEditor(client: ClientController, close: () -> Unit) {
    val draft by client.vaultDrafts.current.collectAsStateWithLifecycle()
    val ready by client.vaultDrafts.ready.collectAsStateWithLifecycle()
    val qr by client.qr.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val library by client.library.listing.state.collectAsStateWithLifecycle()
    val files = library.valueOrNull()?.files.orEmpty()
    var failure by remember { mutableStateOf<String?>(null) }
    var validation by remember { mutableStateOf(false) }
    var choosingType by remember { mutableStateOf(false) }
    var adding by remember { mutableStateOf(false) }
    var attachment by remember { mutableStateOf(false) }
    var generator by remember { mutableStateOf<String?>(null) }
    val current = draft ?: return
    var customizing by remember(current.key, current.kind) { mutableStateOf(current.kind != "login") }
    val titleFocus = remember { androidx.compose.ui.focus.FocusRequester() }
    val focus = androidx.compose.ui.platform.LocalFocusManager.current
    LaunchedEffect(qr.result, current.key) {
        val scanned = qr.result ?: return@LaunchedEffect
        val field = current.fields.firstOrNull { it.kind == "otp" && scanned.target == "otp:" + current.key + ":" + it.id } ?: return@LaunchedEffect
        client.qr.consume(scanned.target)?.let { text ->
            client.vaultDrafts.update { value -> value.field(field.id) { it.copy(value = text, changed = true) } }
        }
    }
    val working = state.working("vault:save:" + current.key) || state.working("vault:discard") || !ready
    fun update(change: (VaultDraft) -> VaultDraft) { client.activity(); if (!working) client.vaultDrafts.update(change) }
    Column(Modifier.testTag("vault-editor").fillMaxWidth().imePadding()) {
        Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()).padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                TextButton(onClick = close, enabled = !working) { Text(str(R.string.text_cancel_998b9c4)) }
                Text(if (current.id == null) str(R.string.text_new_item_9916f9f) else str(R.string.text_edit_item_3638f4f), style = MaterialTheme.typography.titleLarge)
            }
            OutlinedTextField(current.title, { value -> update { it.copy(title = value) } }, Modifier.testTag("vault-editor-title").focusRequester(titleFocus).revealFocusedInput().fillMaxWidth(), label = { Text(str(R.string.text_title_4c18fa6)) }, singleLine = true, enabled = !working,
                isError = validation && (current.title.isBlank() || current.title.codePointCount(0, current.title.length) > 200))
            if (current.id == null) Box {
                OutlinedButton(onClick = { choosingType = true }, enabled = !working) { Text(vaultKindName(current.kind)); Icon(Icons.Outlined.ArrowDropDown, null) }
                DropdownMenu(choosingType, { choosingType = false }) { vaultKinds().forEach { kind ->
                    DropdownMenuItem(text = { Text(vaultKindName(kind)) }, onClick = { update { it.switchKind(kind) }; choosingType = false })
                } }
            }
            current.fields.forEachIndexed { index, field -> key(field.id) {
                var reveal by remember { mutableStateOf(false) }
                var visited by remember { mutableStateOf(false) }
                var focused by remember { mutableStateOf(false) }
                var types by remember { mutableStateOf(false) }
                val error = if (validation || visited) validateDraftField(field) else null
                Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                    Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        if (customizing) Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                            Box {
                                TextButton(onClick = { types = true }, enabled = !working && (current.id == null || !field.concealed || field.changed)) { Text(fieldKindName(field.kind)) }
                                DropdownMenu(types, { types = false }) { fieldKinds.forEach { kind ->
                                    DropdownMenuItem(text = { Text(fieldKindName(kind)) }, onClick = { update { it.field(field.id) { f -> f.copy(kind = kind, concealed = kind in listOf("concealed", "otp"), changed = true) } }; types = false })
                                } }
                            }
                            Row {
                                IconButton(onClick = { update { it.move(field.id, -1) } }, enabled = !working && index > 0) { Icon(Icons.Outlined.ArrowUpward, str(R.string.text_move_field_up_65720e4)) }
                                IconButton(onClick = { update { it.move(field.id, 1) } }, enabled = !working && index < current.fields.lastIndex) { Icon(Icons.Outlined.ArrowDownward, str(R.string.text_move_field_down_cc8899b)) }
                                IconButton(onClick = { update { it.copy(fields = it.fields.filterNot { f -> f.id == field.id }) } }, enabled = !working) { Icon(Icons.Outlined.DeleteOutline, str(R.string.text_remove_field_373cf91)) }
                            }
                        }
                        if (customizing) OutlinedTextField(field.label, { value -> update { it.field(field.id) { f -> f.copy(label = value) } } }, Modifier.fillMaxWidth(),
                            label = { Text(str(R.string.text_field_name_fb3c7fc)) }, placeholder = { Text(vaultFieldTitle(field.id, field.kind, "")) }, enabled = !working, singleLine = true)
                        OutlinedTextField(field.value, { value -> update { it.field(field.id) { f -> f.copy(value = value, changed = true) } } },
                            Modifier.fillMaxWidth().revealFocusedInput().onFocusChanged { focus -> if (focused && !focus.isFocused) visited = true; focused = focus.isFocused },
                            label = { Text(if (customizing) str(R.string.text_value_293fc8c) else vaultFieldTitle(field.id, field.kind, field.label)) }, placeholder = { Text(if (current.id != null && field.concealed && !field.changed) str(R.string.text_leave_empty_to_keep_existing_value_d1cba32) else "") },
                            visualTransformation = if (field.concealed && !reveal) PasswordVisualTransformation() else VisualTransformation.None,
                            keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = when {
                                field.concealed -> KeyboardType.Password
                                field.kind == "email" -> KeyboardType.Email
                                field.kind == "url" -> KeyboardType.Uri
                                field.kind == "phone" -> KeyboardType.Phone
                                else -> KeyboardType.Text
                            }), minLines = if (field.id in listOf("private_key", "public_key", "phrase")) 3 else 1, enabled = !working, isError = error != null,
                            supportingText = { error?.let { Text(errorText(it)) } })
                        if (field.concealed) {
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                TextButton(onClick = { reveal = !reveal }) { Text(if (reveal) str(R.string.text_hide_draft_a649c8a) else str(R.string.text_show_draft_5987b01)) }
                                if (field.kind != "otp" && field.id !in listOf("number", "cvv", "pin", "private_key", "public_key", "phrase"))
                                    TextButton(onClick = { generator = field.id }, enabled = !working) { Text(str(R.string.text_generate_password_4fe4cd0)) }
                            }
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
                        if (customizing) OutlinedTextField(field.section, { value -> update { it.field(field.id) { f -> f.copy(section = value) } } }, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_section_optional_9eb10c6)) }, enabled = !working, singleLine = true)
                    }
                }
            } }
            if (current.kind == "login") SettingToggle(str(R.string.customize_fields), customizing, { customizing = it }, enabled = !working)
            if (customizing) {
                SettingToggle(str(R.string.text_favorite_898f4c6), current.favorite, { value -> update { it.copy(favorite = value) } }, enabled = !working)
                OutlinedTextField(current.tags, { value -> update { it.copy(tags = value) } }, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_tags_33e9c4d)) }, supportingText = { Text(str(R.string.text_separate_with_commas_up_to_20_tags_11cf2a5)) }, enabled = !working)
                Box {
                    OutlinedButton(onClick = { adding = true }, enabled = !working && current.fields.size < 64) { Icon(Icons.Outlined.Add, null); Text(str(R.string.text_add_field_a6fc993)) }
                    DropdownMenu(adding, { adding = false }) { fieldKinds.forEach { kind -> DropdownMenuItem(text = { Text(fieldKindName(kind)) }, onClick = { update { it.newField(kind) }; adding = false }) } }
                }
                OutlinedTextField(current.notes, { value -> update { it.copy(notes = value) } }, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_notes_f7df47b)) }, minLines = 4, keyboardOptions = KeyboardOptions(autoCorrectEnabled = false), enabled = !working)
                SectionTitle(str(R.string.text_attachments_be5733e))
                current.files.forEach { id -> Row(Modifier.fillMaxWidth()) { Text(files.firstOrNull { it.id == id }?.name ?: id, Modifier.weight(1f), style = MaterialTheme.typography.bodySmall); IconButton(onClick = { update { it.copy(files = it.files - id) } }, enabled = !working) { Icon(Icons.Outlined.Close, str(R.string.text_remove_attachment_1096343)) } } }
                TextButton(onClick = { client.library.browse("root", search = "", discarded = false); attachment = true }, enabled = !working && current.files.size < 20) { Text(str(R.string.text_choose_from_files_c1e8160)) }
            }
        }
        Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            failure?.let { Notice(errorText(it), true) }
            if (!ready) TextButton(onClick = { client.vaultDrafts.load() }) { Text(str(R.string.text_retry_563171c)) }
            Text(str(R.string.text_saved_on_this_device_first_then_synchronized_unsynced_changes_can_0921bb4), style = MaterialTheme.typography.bodySmall)
            Button(onClick = {
                validation = true
                val edit = current.edit()
                failure = when {
                    edit.title.isBlank() || edit.title.codePointCount(0, edit.title.length) > 200 -> "INVALID_TITLE"
                    edit.tags.size > 20 || edit.tags.any { it.codePointCount(0, it.length) > 40 } -> "INVALID_TAGS"
                    current.fields.any { validateDraftField(it) != null } -> "INVALID_FIELDS"
                    else -> null
                }
                if (failure == "INVALID_TITLE") titleFocus.requestFocus()
                if (failure == "INVALID_TAGS" || failure == "INVALID_FIELDS") customizing = true
                if (failure == null) {
                    focus.clearFocus()
                    val input = client.vaultDrafts.input()
                    client.perform({ it.commitVaultDraft(input, edit) }, key = "vault:save:" + current.key, onError = { failure = it }) { saved ->
                        client.vaultDrafts.finished(input.id); client.vault.browse(); client.vault.select(saved.item.id)
                    }
                }
            }, enabled = !working, modifier = Modifier.testTag("vault-editor-save").fillMaxWidth().heightIn(min = 52.dp)) { if (working) CircularProgressIndicator(Modifier.size(20.dp)) else Text(str(R.string.text_save_on_this_device_dfe466d)) }
        }
    }
    if (attachment) AlertDialog(onDismissRequest = { attachment = false }, title = { Text(str(R.string.text_choose_a_file_3703fda)) }, text = {
        Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState())) {
            files.filter { it.id !in current.files }.forEach { file -> TextButton(onClick = { update { it.copy(files = it.files + file.id) }; attachment = false }) { Text(file.name) } }
            if (files.isEmpty()) Text(str(R.string.text_import_a_file_in_files_first_d76a295))
        }
    }, confirmButton = { TextButton(onClick = { attachment = false }) { Text(str(R.string.text_done_9ffd599)) } })
    generator?.let { id -> PasswordGenerator(onDismiss = { generator = null }) { value -> update { it.field(id) { f -> f.copy(value = value, changed = true) } }; generator = null } }
}

private val fieldKinds = listOf("text", "concealed", "url", "email", "phone", "date", "month", "otp")
internal fun fieldKindName(kind: String): String = when (kind) {
    "concealed" -> str(R.string.text_hidden_value_b28dd5f); "url" -> str(R.string.text_website_bbb4521); "email" -> str(R.string.text_email_6d5db1e)
    "phone" -> str(R.string.text_phone_9460aa6); "date" -> str(R.string.text_date_5da2748); "month" -> str(R.string.text_month_350fe84); "otp" -> str(R.string.text_one_time_code_c7b1bc6); else -> str(R.string.text_text_e349a14)
}
@Composable private fun PasswordGenerator(onDismiss: () -> Unit, use: (String) -> Unit) {
    var length by remember { mutableIntStateOf(20) }
    var symbols by remember { mutableStateOf(true) }
    var value by remember { mutableStateOf(generatedPassword(length, symbols)) }
    AlertDialog(onDismissRequest = onDismiss, title = { Text(str(R.string.text_generate_password_4fe4cd0)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(value, style = MaterialTheme.typography.bodyLarge)
            Text(str(R.string.text_length_889fe99) + localizedNumber(length))
            Slider(length.toFloat(), { length = it.toInt(); value = generatedPassword(length, symbols) }, valueRange = 12f..64f, steps = 51)
            SettingToggle(str(R.string.text_symbols_04fef1c), symbols, { symbols = it; value = generatedPassword(length, symbols) })
            TextButton(onClick = { value = generatedPassword(length, symbols) }) { Text(str(R.string.text_generate_again_c8b3202)) }
        }
    }, confirmButton = { TextButton(onClick = { use(value) }) { Text(str(R.string.text_use_password_b417e48)) } }, dismissButton = { TextButton(onClick = onDismiss) { Text(str(R.string.text_cancel_998b9c4)) } })
}
