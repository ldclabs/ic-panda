package net.dmsg.android.platform

import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Key
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.ui.WorkspaceSearchField
import net.dmsg.android.ui.pageMargin

@Composable
internal fun PasswordPickerHeading(app: String, host: String?, cancel: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(str(R.string.text_choose_account_58cfffa), style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
            Text(app, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (host != null) Text(str(R.string.text_website_claimed_by_app_ae298e3) + ": " + host, style = MaterialTheme.typography.bodySmall)
        }
        IconButton(onClick = cancel) { Icon(Icons.Outlined.Close, str(R.string.text_cancel_998b9c4)) }
    }
}

/** Shown after the export key has been unlocked. Only display fields reach Compose. */
@Composable
internal fun PasswordPicker(modifier: Modifier, entries: List<ExportCredential>, recent: List<String>, host: String?,
                           busy: Boolean, onChoose: (ExportCredential) -> Unit, onSearch: () -> Unit = {}, heading: @Composable () -> Unit) {
    var query by remember { mutableStateOf("") }
    val focus = LocalFocusManager.current
    val largeText = LocalDensity.current.fontScale >= 1.5f
    val sorted = remember(entries, recent, host, query) {
        val order = recent.withIndex().associate { it.value to it.index }
        val terms = query.trim().lowercase().split(Regex("\\s+")).filter(String::isNotEmpty)
        entries.filter { entry ->
            if (terms.isEmpty()) true
            else {
                val fields = listOf(entry.title, entry.username, entry.origin) + entry.searchKeys
                terms.all { term -> fields.any { it.contains(term, ignoreCase = true) } }
            }
        }
            .sortedWith(compareBy<ExportCredential> { order[it.itemId] ?: Int.MAX_VALUE }
                .thenByDescending { host != null && AutofillTrust.host(it.origin) == host }
                .thenBy(String.CASE_INSENSITIVE_ORDER) { it.title }.thenBy { it.username })
    }
    Column(modifier) {
        Column(Modifier.fillMaxWidth().padding(horizontal = pageMargin(), vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            heading()
            WorkspaceSearchField(query, { query = it; onSearch() }, str(R.string.text_search_vault_f6e42b2))
        }
        LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(horizontal = pageMargin())) {
            items(sorted, key = { it.id }) { entry ->
                Row(Modifier.fillMaxWidth().clickable(enabled = !busy, role = Role.Button) {
                    focus.clearFocus()
                    onChoose(entry)
                }.heightIn(min = 56.dp).padding(horizontal = 4.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (!largeText) {
                        Surface(shape = MaterialTheme.shapes.small, color = MaterialTheme.colorScheme.secondaryContainer) {
                            Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
                                Text(entry.title.ifEmpty { entry.username }.take(1).uppercase(), style = MaterialTheme.typography.titleSmall,
                                    modifier = Modifier.clearAndSetSemantics {})
                            }
                        }
                    }
                    Column(Modifier.weight(1f)) {
                        val lines = if (largeText) Int.MAX_VALUE else 1
                        Text(entry.title, style = MaterialTheme.typography.titleMedium, maxLines = lines, overflow = TextOverflow.Ellipsis)
                        val site = entry.origin.removePrefix("https://")
                        val detail = listOf(entry.username, site.takeUnless { it == entry.title }.orEmpty()).filter(String::isNotEmpty).joinToString(" · ")
                        if (detail.isNotEmpty()) Text(detail, style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = lines, overflow = TextOverflow.Ellipsis)
                    }
                    if (entry.itemId in recent) Icon(Icons.Outlined.History, str(R.string.autofill_used),
                        Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                HorizontalDivider(Modifier.padding(start = if (largeText) 4.dp else 52.dp))
            }
            if (sorted.isEmpty()) item {
                Column(
                    Modifier.fillMaxWidth().padding(vertical = 32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Icon(
                        if (entries.isEmpty()) Icons.Outlined.Key else Icons.Outlined.Search,
                        null,
                        Modifier.size(32.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(
                        str(if (entries.isEmpty()) R.string.autofill_no_items else R.string.no_matches),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                    )
                    if (query.isNotEmpty()) TextButton(onClick = { query = "" }) { Text(str(R.string.text_clear_search_3f4dec6)) }
                }
            }
        }
    }
}

@Composable
internal fun ConfirmLogin(entry: ExportCredential, app: String, host: String?, canRemember: Boolean,
                         dismiss: () -> Unit, fill: (Boolean) -> Unit) {
    AlertDialog(onDismissRequest = dismiss,
        title = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(str(R.string.autofill_confirm), Modifier.weight(1f).semantics { heading() })
                IconButton(onClick = dismiss) { Icon(Icons.Outlined.Close, str(R.string.text_cancel_998b9c4)) }
            }
        },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(entry.title, style = MaterialTheme.typography.titleMedium)
                if (entry.username.isNotEmpty()) Text(entry.username)
                if (entry.origin.isNotEmpty()) Text(entry.origin, style = MaterialTheme.typography.bodySmall)
                HorizontalDivider(Modifier.padding(vertical = 6.dp))
                Text(str(R.string.text_requesting_app_c06bb1c) + ": " + app)
                if (host != null) Text(str(R.string.text_website_claimed_by_app_ae298e3) + ": " + host)
            }
        },
        confirmButton = {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Button(onClick = { fill(false) }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.autofill_once)) }
                if (canRemember) OutlinedButton(onClick = { fill(true) }, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.autofill_remember)) }
            }
        })
}
