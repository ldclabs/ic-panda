package net.dmsg.android.platform

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import net.dmsg.android.R

/** Shown after the export key has been unlocked. Only display fields reach Compose. */
@Composable
internal fun PasswordPicker(modifier: Modifier, entries: List<ExportCredential>, recent: List<String>, host: String?,
                           busy: Boolean, onChoose: (ExportCredential) -> Unit, heading: @Composable () -> Unit) {
    var query by remember { mutableStateOf("") }
    val sorted = remember(entries, recent, host, query) {
        val order = recent.withIndex().associate { it.value to it.index }
        entries.filter { entry -> query.isBlank() || listOf(entry.title, entry.username, entry.origin).any { it.contains(query, true) } }
            .sortedWith(compareBy<ExportCredential> { order[it.itemId] ?: Int.MAX_VALUE }
                .thenByDescending { host != null && AutofillTrust.host(it.origin) == host }
                .thenBy(String.CASE_INSENSITIVE_ORDER) { it.title }.thenBy { it.username })
    }
    LazyColumn(modifier, contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                heading()
                OutlinedTextField(query, { query = it }, label = { Text(str(R.string.text_search_05fdb9c)) },
                    singleLine = true, modifier = Modifier.fillMaxWidth())
            }
        }
        items(sorted, key = { it.id }) { entry ->
            OutlinedButton(onClick = { onChoose(entry) }, enabled = !busy, modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.fillMaxWidth()) {
                    Text(entry.title)
                    if (entry.username.isNotEmpty()) Text(entry.username)
                    if (entry.origin.isNotEmpty()) Text(entry.origin)
                    if (entry.itemId in recent) Text(str(R.string.autofill_used))
                }
            }
        }
        if (sorted.isEmpty()) item {
            Text(str(if (entries.isEmpty()) R.string.autofill_no_items else R.string.no_matches), modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            if (query.isNotEmpty()) TextButton(onClick = { query = "" }) { Text(str(R.string.text_clear_search_3f4dec6)) }
        }
    }
}

@Composable
internal fun ConfirmLogin(entry: ExportCredential, app: String, host: String?, canRemember: Boolean,
                         dismiss: () -> Unit, fill: (Boolean) -> Unit) {
    AlertDialog(onDismissRequest = dismiss,
        title = { Text(str(R.string.autofill_confirm)) },
        text = {
            Column {
                Text(entry.title)
                Text(entry.username)
                Text(str(R.string.text_requesting_app_c06bb1c) + ": " + app)
                if (host != null) Text(str(R.string.text_website_claimed_by_app_ae298e3) + ": " + host)
            }
        },
        confirmButton = {
            Column {
                if (canRemember) TextButton(onClick = { fill(true) }) { Text(str(R.string.autofill_remember)) }
                TextButton(onClick = { fill(false) }) { Text(str(R.string.autofill_once)) }
            }
        },
        dismissButton = { TextButton(onClick = dismiss) { Text(str(R.string.text_cancel_998b9c4)) } })
}
