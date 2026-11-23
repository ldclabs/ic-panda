package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import android.app.Activity
import android.content.res.Resources
import android.content.ContextWrapper
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Translate
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.platform.LocalWindowInfo
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

/** Android recreates the Activity for a new locale. Return to the setting that opened the picker. */
@Composable internal fun AccountLanguageRow() {
    var open by remember { mutableStateOf(false) }
    var returnLocale by rememberSaveable { mutableStateOf<String?>(null) }
    val locale = LocalConfiguration.current.locales[0].toLanguageTag()
    val windowFocused = LocalWindowInfo.current.isWindowFocused
    val input = LocalInputModeManager.current
    val focus = remember { FocusRequester() }
    val scroll = remember { BringIntoViewRequester() }
    var height by remember { mutableIntStateOf(0) }

    Box(Modifier.fillMaxWidth().focusRequester(focus).bringIntoViewRequester(scroll).onSizeChanged { height = it.height }) {
        ListRow(str(R.string.app_language), glyph = R.drawable.glyph_language) { open = true }
    }
    if (open) LanguagePicker(onSelected = { tag ->
        returnLocale = tag.ifEmpty { Resources.getSystem().configuration.locales[0].toLanguageTag() }
    }) { open = false }
    LaunchedEffect(returnLocale, locale, windowFocused, open, height) {
        if (returnLocale == locale && !open && windowFocused && height > 0) {
            withFrameNanos { }
            if (input.inputMode == InputMode.Keyboard) focus.requestFocus()
            scroll.bringIntoView(Rect(0f, 0f, 1f, height.toFloat()))
            returnLocale = null
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable internal fun LanguagePicker(onSelected: (String) -> Unit = {}, close: () -> Unit) {
    val context = LocalContext.current
    val selected = remember(LocalConfiguration.current) { AppLanguage.selected(context) }
    var query by remember { mutableStateOf("") }
    val choices = listOf(LanguageChoice("", str(R.string.follow_system_language))) + AppLanguage.supported.map { (tag, name) ->
        LanguageChoice(tag, name, Locale.forLanguageTag(tag).getDisplayName(AppLanguage.locale()).takeUnless { it.equals(name, true) })
    }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        SyncSystemBars()
        Surface(Modifier.fillMaxSize().testTag("language-picker"), color = MaterialTheme.colorScheme.page) {
            Column(Modifier.fillMaxSize().imePadding()) {
                TopAppBar(
                    title = { Text(str(R.string.app_language)) },
                    navigationIcon = { IconButton(onClick = close) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, str(R.string.text_back_5db5cac)) } },
                    colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.page),
                )
                WorkspaceSearchField(query, { query = it }, str(R.string.language_search), Modifier.padding(horizontal = pageMargin()).testTag("language-search"))
                LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(horizontal = pageMargin(), vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    val visible = choices.filter { query.isBlank() || it.tag.contains(query, true) || it.name.contains(query, true) || it.localizedName?.contains(query, true) == true }
                    if (visible.isEmpty()) item { EmptyState(str(R.string.no_matches), icon = Icons.Outlined.Search) }
                    items(visible, key = { it.tag }) { (tag, name, localizedName) ->
                        Section {
                            ListItem(
                                headlineContent = { Text(name) },
                                supportingContent = if (localizedName == null) null else ({ Text(localizedName) }),
                                leadingContent = { RadioButton(selected == tag, onClick = null) },
                                modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp).testTag("language-" + tag.ifEmpty { "system" })
                                    .selectable(selected == tag, role = Role.RadioButton) {
                                        var owner = context
                                        while (owner is ContextWrapper && owner !is Activity) owner = owner.baseContext
                                        (owner as? Activity)?.let { activity -> onSelected(tag); close(); AppLanguage.choose(activity, tag) }
                                    },
                            )
                        }
                    }
                }
            }
        }
    }
}
