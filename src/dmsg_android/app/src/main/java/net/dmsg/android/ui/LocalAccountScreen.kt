package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.DeleteOutline
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.serialization.json.*
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str

@Composable
fun LocalSettings(client: ClientController, go: (String) -> Unit) {
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Section(title = str(R.string.saved_on_device)) {
                ListRow(str(R.string.text_switch_account_52f71df), glyph = R.drawable.glyph_account) { go("accounts") }
                ListRow(str(R.string.text_lock_workspace_78665ed), icon = Icons.Outlined.Lock) { client.lock() }
            }
        }
        item {
            Section(iconColor = MaterialTheme.colorScheme.errorContainer) {
                ListRow(str(R.string.text_remove_local_data_c45125e), icon = Icons.Outlined.DeleteOutline, destructive = true) { go("account-remove") }
            }
        }
    }
}

@Composable
fun LocalImportScreen(client: ClientController) {
    var plan by remember { mutableStateOf<JsonObject?>(null) }
    var selected by remember { mutableStateOf(emptySet<UInt>()) }
    var files by remember { mutableStateOf(emptySet<UInt>()) }
    var working by remember { mutableStateOf(false) }
    var copied by remember { mutableStateOf(0L) }
    var alive by remember { mutableStateOf(true) }
    LaunchedEffect(working, plan?.text("id")) {
        val id = plan?.text("id") ?: return@LaunchedEffect
        while (working) {
            client.perform({ it.localImportStatus(id).decode().objectValue().unsigned("bytes") }, key = "local-import:status", quiet = true) {
                if (alive && it != null) copied = it.toLong()
            }
            kotlinx.coroutines.delay(750)
        }
    }
    DisposableEffect(Unit) {
        onDispose {
            alive = false
            plan?.text("id")?.let { id -> client.perform({ it.localImportCancel(id) }, key = "local-import:cancel", quiet = true) {} }
        }
    }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        val reviewed = plan
        if (reviewed == null) {
            item {
                DetailCard {
                    Text(str(R.string.vault_backup_sync), style = MaterialTheme.typography.bodyMedium)
                    if (working) LinearProgressIndicator(Modifier.fillMaxWidth())
                    Button(enabled = !working, modifier = Modifier.fillMaxWidth(), onClick = {
                        working = true
                        client.clearNotice()
                        client.perform({ it.localImportPreview().decode().objectValue() }, key = "local-import:preview", quiet = true, userInitiated = true, onError = { code ->
                            working = false
                            if (alive) client.showError(code)
                        }) { value ->
                            working = false
                            if (alive) {
                                plan = value
                                selected = value.rows("items").filter { it.text("state") == "new" && JsonNull !in it.getValue("attachments").jsonArray }.map { it.requiredUInt("index") }.toSet()
                            } else client.perform({ it.localImportCancel(value.required("id")) }, key = "local-import:cancel", quiet = true) {}
                        }
                    }) { Text(str(R.string.import_local_account)) }
                }
            }
        } else {
            val required = reviewed.rows("items").filter { it.requiredUInt("index") in selected }.flatMap { row ->
                row["attachments"]?.jsonArray.orEmpty().mapNotNull { it.jsonPrimitive.intOrNull?.toUInt() }
            }.toSet()
            item {
                DetailCard {
                    LabelValue(str(R.string.text_import_destination_4146867), reviewed.required("target"), code = true)
                    Text(str(R.string.vault_backup_sync), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            item { SectionTitle(str(R.string.text_vault_ede2a06)) }
            items(reviewed.rows("items")) { row ->
                val index = row.requiredUInt("index")
                val reason = when (row.text("state")) {
                    "same" -> R.string.vault_backup_same
                    "changed" -> R.string.vault_backup_changed
                    "deleted" -> R.string.vault_backup_deleted
                    else -> R.string.vault_backup_new
                }
                val omitted = row.getValue("attachments").jsonArray.count { it == JsonNull }
                val summary = if (omitted > 0) str(reason) + " · " + str(R.string.vault_backup_omitted) + " · " + localizedNumber(omitted) else str(reason)
                Section {
                    SelectionRow(row.required("title"), index in selected, !working && row.text("state") != "same", summary) { on ->
                        selected = if (on) selected + index else selected - index
                    }
                }
            }
            item { SectionTitle(str(R.string.text_files_c79ddc2)) }
            items(reviewed.rows("files")) { row ->
                val index = row.requiredUInt("index")
                val path = (row.strings("path") + row.required("name")).joinToString("/")
                val summary = fileSize(row.requiredUnsigned("size")) + " · " + displayTime(row.requiredUnsigned("created_at")) + " · " + row.required("version").take(8) +
                    if (row.requiredFlag("current")) " · " + str(R.string.text_current_version_b3f03bb) else ""
                Section {
                    SelectionRow(path, index in files || index in required, !working && index !in required && row.requiredFlag("local"), summary) { on ->
                        files = if (on) files + index else files - index
                    }
                }
            }
            if (working) item {
                DetailCard {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(android.text.format.Formatter.formatFileSize(androidx.compose.ui.platform.LocalContext.current, copied),
                        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            item {
                Button(enabled = !working && (selected.isNotEmpty() || files.isNotEmpty()), modifier = Modifier.fillMaxWidth(), onClick = {
                    working = true
                    client.clearNotice()
                    client.perform({ it.localImportCommit(reviewed.required("id"), selected.sorted(), files.sorted(), "root") }, key = "local-import:commit", quiet = true, userInitiated = true, onError = { code ->
                        working = false
                        copied = 0
                        if (alive) {
                            client.showError(code)
                            if (code == "EXPIRED" || code == "VERSION_CONFLICT") {
                                client.perform({ it.localImportCancel(reviewed.required("id")) }, key = "local-import:cancel", quiet = true) {}
                                plan = null; files = emptySet()
                            }
                        }
                    }) {
                        plan = null
                        working = false
                        client.vault.changed()
                        client.library.changed()
                        if (alive) client.navigation.back()
                    }
                }) { Text(str(R.string.text_import_1a200b3)) }
            }
        }
    }
}
