package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import android.icu.text.BreakIterator
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.serialization.json.*
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.str
import java.text.DateFormat
import java.text.Normalizer
import java.util.Date
import java.util.Locale

@Composable internal fun BackupApprovalContent(detail: JsonObject) {
    DetailCard {
        LabelValue(str(R.string.text_items_802a927), detail.requiredUnsigned("items").toString())
        LabelValue(str(R.string.text_attachments_be5733e), detail.requiredUnsigned("files").toString())
        LabelValue(str(R.string.vault_backup_omitted), detail.requiredUnsigned("omitted").toString())
    }
    if (detail.requiredFlag("wallet")) Notice(str(R.string.vault_backup_wallet))
}

private fun validBackupPassword(value: String, repeat: String): Boolean {
    val normalized = Normalizer.normalize(value, Normalizer.Form.NFC)
    if (normalized.toByteArray().size > 256 || normalized != Normalizer.normalize(repeat, Normalizer.Form.NFC)) return false
    val chars = BreakIterator.getCharacterInstance(Locale.ROOT)
    chars.setText(normalized)
    return (1..6).all { chars.next() != BreakIterator.DONE }
}

@Composable fun VaultBackupScreen(client: ClientController) {
    val controller = client.vaultBackup
    val form by controller.form.collectAsStateWithLifecycle()
    val app by client.state.collectAsStateWithLifecycle()
    val working = app.working("vault:backup")
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument(), controller::picked)
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/vnd.dmsg.vault-backup+cbor"), controller::destination)
    var password by remember(form.id, form.step) { mutableStateOf("") }
    var confirmation by remember(form.id, form.step) { mutableStateOf("") }
    var include by remember(form.id) { mutableStateOf(true) }
    var folder by remember(form.id) { mutableStateOf("root") }
    var chooseFolder by remember { mutableStateOf(false) }
    val inventory = form.view["inventory"] as? JsonObject
    val rows = (inventory?.get("items") as? JsonArray)?.map { it.jsonObject }.orEmpty()
    val preview = (form.view["items"] as? JsonArray)?.map { it.jsonObject }.orEmpty()
    var selection by remember(form.id, inventory) { mutableStateOf(rows.map { it.required("id") }.toSet()) }
    var picks by remember(form.id, preview) { mutableStateOf(preview.filter { it.required("status") == "new" }.map { it.requiredUInt("index") }.toSet()) }
    val chosen = rows.filter { it.required("id") in selection }
    val files = chosen.flatMap { it.getValue("files").jsonArray.map { f -> f.jsonPrimitive.content } }.distinct()
    val fileInfo = inventory?.get("files") as? JsonObject
    val missing = files.any { (fileInfo?.get(it) as? JsonObject)?.requiredFlag("available") != true }
    val bytes = files.sumOf { id ->
        val meta = (fileInfo?.get(id) as? JsonObject)?.get("meta") as? JsonObject
        meta?.requiredUnsigned("size") ?: 0uL
    }
    var processed by remember(form.id, working) { mutableStateOf(0uL) }
    LaunchedEffect(form.id, working) {
        while (working && form.id.isNotEmpty()) {
            kotlinx.coroutines.delay(1000)
            processed = controller.processedBytes(form.id)
        }
    }
    val included = preview.filter { it.requiredUInt("index") in picks }.flatMap { it.getValue("files").jsonArray.filterNot { f -> f == JsonNull } }.distinct().size
    LaunchedEffect(form.step) { if (form.step == "waiting") controller.approved() }
    LaunchedEffect(form.failure) { if (form.failure == "INTEGRITY_FAILED") password = "" }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        item {
            LabelValue(str(R.string.text_account_4ab56e9), app.session?.account.orEmpty(), code = true)
        }
        if (working) item {
            DetailCard {
                LinearProgressIndicator(Modifier.fillMaxWidth())
                if (processed > 0uL) Text(fileSize(processed), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        if (form.failure != null) item { Notice(str(if (form.failure == "uncommitted") R.string.vault_backup_uncommitted else R.string.vault_backup_unlockerror), true) }
        when (form.step) {
            "home" -> item {
                DetailCard {
                    Text(str(R.string.vault_backup_scope), style = MaterialTheme.typography.bodyMedium)
                    Button(onClick = { controller.begin(false) }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.vault_backup_export))
                    }
                    OutlinedButton(onClick = { controller.begin(true) }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.vault_backup_import))
                    }
                    if (controller.previous() != null) TextButton(onClick = controller::reconcile, enabled = !working) { Text(str(R.string.text_review_9a1a906)) }
                }
            }
            "loading" -> item {
                DetailCard {
                    OutlinedButton(onClick = controller::inventory, enabled = !working, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_retry_563171c)) }
                }
            }
            "export" -> {
                item {
                    DetailCard {
                        Text(str(R.string.vault_backup_scope), style = MaterialTheme.typography.bodyMedium)
                        Text(DateFormat.getDateTimeInstance().format(Date(form.view.requiredUnsigned("at").toLong())),
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        Text(str(R.string.text_trash_8af1079) + " " + inventory?.get("trash") + " · " + str(R.string.text_managed_by_dmsg_35b3681) + " " + inventory?.get("managed"),
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(str(R.string.text_version_conflicts_0df1b3c) + " " + inventory?.get("conflicts"),
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(str(R.string.text_pending_upload_3dce896) + chosen.count { it.text("save_state") == "pending" || (it.text("save_state") == null && !it.requiredFlag("synced")) },
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton({ selection = rows.map { it.required("id") }.toSet() }, enabled = !working) { Text(str(R.string.text_all_eae0ae1)) }
                            TextButton({ selection = emptySet() }, enabled = !working) { Text(str(R.string.text_clear_selection_66ecd44)) }
                        }
                    }
                }
                items(rows, key = { it.required("id") }) { row ->
                    val id = row.required("id")
                    Section {
                        SelectionRow(row.required("title"), id in selection, !working, vaultKindName(row.required("kind"))) {
                            selection = if (it) selection + id else selection - id
                        }
                    }
                }
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        if (files.isNotEmpty()) Section {
                            SelectionRow(str(R.string.vault_backup_files) + " (" + files.size + " · " + fileSize(bytes) + ")", include, !working) { include = it }
                        }
                        if (missing && include) Notice(str(R.string.vault_backup_unavailable), true)
                        DetailCard {
                            if (chosen.any { it.required("kind") == "wallet" }) {
                                Text(str(R.string.vault_backup_wallet), style = MaterialTheme.typography.bodyMedium)
                            }
                            SecretTextField(password, { password = it }, str(R.string.vault_backup_password), enabled = !working)
                            Text(str(R.string.vault_backup_hint), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            TextButton({ controller.generate { password = it } }, enabled = !working) {
                                Icon(Icons.Outlined.Key, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_generate_password_4fe4cd0))
                            }
                            val mismatch = confirmation.isNotEmpty() && !Normalizer.normalize(password, Normalizer.Form.NFC)
                                .equals(Normalizer.normalize(confirmation, Normalizer.Form.NFC))
                            SecretTextField(
                                confirmation,
                                { confirmation = it },
                                str(R.string.vault_backup_repeat),
                                enabled = !working,
                                isError = mismatch,
                                supportingText = if (mismatch) ({ Text(str(R.string.vault_backup_mismatch)) }) else null,
                            )
                            Button({ controller.request(selection.toList(), include, password, confirmation) },
                                enabled = !working && selection.isNotEmpty() && (!include || !missing) && validBackupPassword(password, confirmation),
                                modifier = Modifier.fillMaxWidth()) {
                                Text(str(R.string.vault_backup_export) + " (" + selection.size + ")")
                            }
                        }
                    }
                }
            }
            "waiting" -> item {
                DetailCard {
                    Button(controller::approved, enabled = !working, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_continue_fd19155)) }
                    OutlinedButton({ form.approval?.let(client.approvals::open) }, enabled = !working, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_review_approval_db00f36)) }
                }
            }
            "ready" -> item {
                DetailCard {
                    LabelValue(str(R.string.text_items_802a927), form.view["items"].toString())
                    LabelValue(str(R.string.text_attachments_be5733e), form.view["files"].toString())
                    Button({
                        controller.prepareSave { save.launch("dmsg-vault-" + java.time.LocalDate.now() + ".dmsgvault") }
                    }, enabled = !working, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_save_ec8e6d5)) }
                }
            }
            "pick" -> item {
                DetailCard {
                    OutlinedButton({ if (controller.external()) pick.launch(arrayOf("*/*")) }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Outlined.FileOpen, null); Spacer(Modifier.width(8.dp)); Text(str(R.string.text_choose_a_file_2916860))
                    }
                    form.input?.let { Text(it.substringAfterLast('/'), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis) }
                    SecretTextField(password, { password = it }, str(R.string.vault_backup_password), enabled = !working)
                    Button({ controller.inspect(password) }, enabled = !working && form.input != null && password.isNotEmpty(), modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_review_9a1a906))
                    }
                }
            }
            "preview" -> {
                item {
                    DetailCard {
                        LabelValue(str(R.string.text_source_fcdba0f), form.view.required("source"), code = true)
                        LabelValue(str(R.string.text_destination_63c1289), form.view.required("account"), code = true)
                        LabelValue(str(R.string.text_created_36ba304), DateFormat.getDateTimeInstance().format(Date(form.view.requiredUnsigned("at").toLong())))
                        LabelValue(str(R.string.vault_backup_omitted), form.view["omitted"].toString())
                        Text(str(R.string.vault_backup_new) + " " + preview.count { it.required("status") == "new" } + " · " + str(R.string.vault_backup_same) + " " + preview.count { it.required("status") == "same" } + " · " + str(R.string.text_review_9a1a906) + " " + preview.count { it.required("status") in setOf("changed", "deleted") },
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton({ picks = preview.filter { it.required("status") == "new" }.map { it.requiredUInt("index") }.toSet() }, enabled = !working) { Text(str(R.string.vault_backup_new)) }
                            TextButton({ picks = emptySet() }, enabled = !working) { Text(str(R.string.text_clear_selection_66ecd44)) }
                        }
                    }
                }
                items(preview, key = { it.requiredUInt("index").toInt() }) { row ->
                    val index = row.requiredUInt("index")
                    val status = row.required("status")
                    val reason = when (status) { "same" -> R.string.vault_backup_same; "changed" -> R.string.vault_backup_changed; "deleted" -> R.string.vault_backup_deleted; else -> R.string.vault_backup_new }
                    val refs = row.getValue("files").jsonArray
                    val omitted = refs.count { it == JsonNull }
                    val details = vaultKindName(row.required("kind")) + " · " + str(R.string.text_attachments_be5733e) + " " + (refs.size - omitted) +
                        (if (omitted > 0) " · " + str(R.string.vault_backup_omitted) + " " + omitted else "") + " · " + str(reason)
                    Section {
                        SelectionRow(row.required("title"), index in picks, !working && status != "same", details) {
                            picks = if (it) picks + index else picks - index
                        }
                    }
                }
                item {
                    DetailCard {
                        if (included > 0) Box(Modifier.fillMaxWidth()) {
                            OutlinedButton({ chooseFolder = true }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                                Icon(Icons.Outlined.Folder, null)
                                Spacer(Modifier.width(8.dp))
                                Text(str(R.string.text_folder_b0d1441) + " · " + (form.folders.find { it.first == folder }?.second ?: str(R.string.text_files_c79ddc2)))
                            }
                            DropdownMenu(chooseFolder, { chooseFolder = false }) {
                                (listOf("root" to str(R.string.text_files_c79ddc2)) + form.folders.filterNot { it.first == "root" }).forEach { f ->
                                    DropdownMenuItem(
                                        text = { Text(f.second) },
                                        leadingIcon = { Icon(Icons.Outlined.Folder, null) },
                                        onClick = { folder = f.first; chooseFolder = false },
                                    )
                                }
                            }
                        }
                        if (picks.isEmpty()) OutlinedButton(controller::clear, enabled = !working, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_done_9ffd599)) }
                        else Button({ controller.commit(picks.sorted(), folder) }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                            Text(str(R.string.vault_backup_import) + " (" + picks.size + " · " + str(R.string.text_attachments_be5733e) + " " + included + ")")
                        }
                    }
                }
            }
            "done" -> item {
                DetailCard {
                    Text(str(R.string.vault_backup_saved), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                    LabelValue(str(R.string.text_items_802a927), form.view["items"].toString())
                    LabelValue(str(R.string.text_attachments_be5733e), form.view["files"].toString())
                    LabelValue(str(R.string.vault_backup_omitted), form.view["omitted"].toString())
                    if ("skipped" in form.view) Text(str(R.string.vault_backup_sync), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Button(controller::clear, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_done_9ffd599)) }
                }
            }
        }
    }
}
