package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

@Composable fun FeedbackScreen(client: ClientController) {
    val state by client.state.collectAsStateWithLifecycle()
    val report by client.feedback.report.state.collectAsStateWithLifecycle()
    val draft by client.feedback.draft.collectAsStateWithLifecycle()
    val library by client.library.listing.state.collectAsStateWithLifecycle()
    var attaching by remember { mutableStateOf(false) }
    var confirming by remember { mutableStateOf(false) }
    var discard by remember { mutableStateOf(false) }
    LaunchedEffect(state.epoch) { client.feedback.load() }
    val value = report.valueOrNull()
    val form = draft
    val busy = state.working("feedback:send") || state.working("feedback:discard")
    val editable = !busy && (value == null || value.state == "draft")
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ReadFeedback(report, client.feedback::load)
        Text(str(R.string.text_tell_us_what_happened_3978a58), style = MaterialTheme.typography.headlineSmall)
        Notice(str(R.string.text_review_the_report_before_sending_no_vault_entries_messages_or_log_299c6b0))
        if (form != null) {
            val bytes = form.text.toByteArray().size
            OutlinedTextField(form.text, { client.feedback.update(form.copy(text = it)) }, Modifier.fillMaxWidth(), minLines = 6, enabled = editable, label = { Text(str(R.string.text_description_a993ca6)) }, isError = bytes > 4500,
                supportingText = { Text(localizedNumber(bytes) + " / " + localizedNumber(4500) + " " + str(R.string.text_utf_8_bytes_121f5ad)) })
            SectionTitle(str(R.string.text_included_context_3b2a6df))
            Text(form.context, style = MaterialTheme.typography.bodySmall)
            SectionTitle(str(R.string.text_screenshots_9699c0f))
            form.files.forEach { id ->
                val image = value?.screenshots?.find { it.file == id }
                Row(Modifier.fillMaxWidth()) {
                    Text(image?.name ?: library.valueOrNull()?.files?.find { it.id == id }?.name ?: id, Modifier.weight(1f))
                    if (editable) TextButton(onClick = { client.feedback.update(form.copy(files = form.files - id)) }) { Text(str(R.string.text_remove_78190c6)) }
                }
                image?.url?.let { link -> SelectionContainer { Text(link, style = MaterialTheme.typography.bodySmall) }; Text(messageTime(image.expiresAt), style = MaterialTheme.typography.labelSmall) }
            }
            if (editable) OutlinedButton(onClick = { client.library.browse("root", search = "", discarded = false); attaching = true }, enabled = form.files.size < 3) { Text(str(R.string.text_add_screenshot_8363406)) }
            value?.let { Text(when (it.state) { "sent" -> str(R.string.text_sent_a16d792); "pending" -> str(R.string.text_waiting_for_confirmation_9c87aad); else -> str(R.string.text_encrypted_draft_on_this_device_52e89e4) }, style = MaterialTheme.typography.labelLarge); it.error?.let { code -> Notice(errorText(code), true) } }
            if (value?.state != "sent") Button(onClick = { confirming = true }, enabled = form.text.isNotBlank() && bytes <= 4500 && !busy, modifier = Modifier.fillMaxWidth()) { Text(if (value?.state == "pending") str(R.string.text_retry_original_report_4bb9eaa) else str(R.string.text_review_and_send_1d7f384)) }
            if (value?.state == "pending" && value.editable) TextButton(onClick = client.feedback::edit, enabled = !busy) { Text(str(R.string.text_return_to_editing_190592d)) }
            if (value != null && value.state != "pending") TextButton(onClick = { discard = true }, enabled = !busy) { Text(if (value.state == "sent") str(R.string.text_new_feedback_c97ee75) else str(R.string.text_discard_draft_6008f2b)) }
        }
    }
    if (attaching && form != null) AlertDialog(onDismissRequest = { attaching = false }, title = { Text(str(R.string.text_choose_a_screenshot_bf4fbfd)) }, text = {
        Column(Modifier.heightIn(max = 400.dp).verticalScroll(rememberScrollState())) {
            ReadFeedback(library, { client.library.browse("root", search = "", discarded = false) })
            library.valueOrNull()?.files.orEmpty().filter { it.local && it.size <= 10uL * 1024u * 1024u && it.mime in setOf("image/png", "image/jpeg", "image/webp", "image/gif") && it.id !in form.files }.forEach { file ->
                TextButton(onClick = { client.feedback.update(form.copy(files = form.files + file.id)); attaching = false }) { Text(file.name) }
            }
            Notice(str(R.string.text_select_up_to_three_imported_images_at_most_10_mib_each_links_are_b2adcf5))
        }
    }, confirmButton = { TextButton(onClick = { attaching = false }) { Text(str(R.string.text_close_6cf4a77)) } })
    if (confirming && form != null) AlertDialog(onDismissRequest = { confirming = false }, title = { Text(str(R.string.text_send_this_report_589df5d)) }, text = {
        Column(Modifier.heightIn(max = 400.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(form.text); Text(form.context, style = MaterialTheme.typography.bodySmall)
            Text(str(R.string.text_recipient_cc4bc59) + "dbrt466u97gecq800000")
            if (form.files.isNotEmpty()) Notice(str(R.string.text_the_recipient_will_receive_private_links_to_the_selected_screensh_4d5142e))
        }
    }, confirmButton = { TextButton(onClick = { confirming = false; client.feedback.send() }) { Text(str(R.string.text_send_06acd73)) } }, dismissButton = { TextButton(onClick = { confirming = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (discard) AlertDialog(onDismissRequest = { discard = false }, title = { Text(str(R.string.text_clear_the_current_report_fb452fb)) }, text = { Text(str(R.string.text_already_delivered_copies_cannot_be_recalled_086aeef)) }, confirmButton = { TextButton(onClick = { discard = false; client.feedback.discard() }) { Text(str(R.string.text_continue_fd19155)) } }, dismissButton = { TextButton(onClick = { discard = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
