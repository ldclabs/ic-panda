package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import net.dmsg.android.platform.WorkspaceEntries

@Composable fun WorkspaceEntriesScreen() {
    val context = LocalContext.current
    var unavailable by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Text(str(R.string.text_open_the_last_selected_account_s_workspace_from_your_home_screen_843a87d))
        Notice(str(R.string.text_widgets_show_navigation_labels_only_they_do_not_show_private_cont_d04c901))
        Button(onClick = { unavailable = !WorkspaceEntries.pinWidget(context) }) { Text(str(R.string.text_add_workspace_widget_1c671c9)) }
        SectionTitle(str(R.string.text_pinned_shortcuts_7185837))
        WorkspaceEntries.labels.forEach { (destination, label) ->
            OutlinedButton(onClick = { unavailable = !WorkspaceEntries.pinShortcut(context, destination) }, modifier = Modifier.fillMaxWidth()) { Text(str(label)) }
        }
        if (unavailable) Notice(str(R.string.text_this_launcher_does_not_support_pinning_here_open_its_widget_or_ap_75a45ce))
    }
}
