package net.dmsg.android.ui

import android.app.Activity
import android.content.ContextWrapper
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import net.dmsg.android.R
import net.dmsg.android.platform.AppLanguage
import net.dmsg.android.platform.str
import java.util.Locale

private data class LanguageChoice(val tag: String, val name: String, val localizedName: String? = null)

@Composable internal fun LanguagePicker(close: () -> Unit) {
    val context = LocalContext.current
    val selected = remember(LocalConfiguration.current) { AppLanguage.selected(context) }
    var query by remember { mutableStateOf("") }
    val choices = listOf(LanguageChoice("", str(R.string.follow_system_language))) + AppLanguage.supported.map { (tag, name) ->
        LanguageChoice(tag, name, Locale.forLanguageTag(tag).getDisplayName(AppLanguage.locale()).takeUnless { it.equals(name, true) })
    }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize().testTag("language-picker")) {
            Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
                Row(Modifier.fillMaxWidth()) {
                    IconButton(onClick = close) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, str(R.string.text_back_5db5cac)) }
                    Text(str(R.string.app_language), Modifier.padding(12.dp), style = MaterialTheme.typography.titleLarge)
                }
                OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth().padding(horizontal = 16.dp).testTag("language-search"), label = { Text(str(R.string.language_search)) }, singleLine = true)
                LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(vertical = 12.dp)) {
                    items(choices.filter { query.isBlank() || it.tag.contains(query, true) || it.name.contains(query, true) || it.localizedName?.contains(query, true) == true }, key = { it.tag }) { (tag, name, localizedName) ->
                        ListItem(
                            headlineContent = { Text(name) },
                            supportingContent = if (localizedName == null) null else ({ Text(localizedName) }),
                            leadingContent = { RadioButton(selected == tag, onClick = null) },
                            modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp).testTag("language-" + tag.ifEmpty { "system" })
                                .selectable(selected == tag, role = Role.RadioButton) {
                                    var owner = context
                                    while (owner is ContextWrapper && owner !is Activity) owner = owner.baseContext
                                    (owner as? Activity)?.let { activity -> close(); AppLanguage.choose(activity, tag) }
                                },
                        )
                    }
                }
            }
        }
    }
}
