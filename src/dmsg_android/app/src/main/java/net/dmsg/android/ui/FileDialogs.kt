package net.dmsg.android.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.str

internal val List<LibraryFolder>.folderId get() = lastOrNull()?.id ?: "root"
internal val List<LibraryFolder>.folderLabel get() = (listOf(str(R.string.text_files_c79ddc2)) + map { it.name }).joinToString(" / ")
internal fun fileActionError(code: String) = if (code == "IDEMPOTENCY_CONFLICT") str(R.string.file_name_taken) else errorText(code)

@Composable internal fun FileFolderDialog(
    client: ClientController, title: String, close: () -> Unit,
    action: String = str(R.string.text_use_this_folder_4a67a24), initial: List<LibraryFolder> = emptyList(),
    exclude: Set<String> = emptySet(), unchanged: String? = null, busy: Boolean = false,
    review: @Composable (String) -> Unit = {}, choose: (List<LibraryFolder>) -> Unit
) {
    var path by remember { mutableStateOf(initial) }
    val folder = path.folderId
    val read by client.library.destinations.state.collectAsStateWithLifecycle()
    val listing = read.valueOrNull()?.takeIf { it.folder == folder }
    LaunchedEffect(folder) { client.library.destination(folder) }
    AlertDialog(onDismissRequest = { if (!busy) close() }, title = { Text(title) }, text = {
        Column(Modifier.heightIn(max = 400.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            review(folder)
            Text(path.folderLabel, style = MaterialTheme.typography.titleSmall)
            if (path.isNotEmpty()) TextButton(onClick = { path = path.dropLast(1) }, enabled = !busy) { Text(str(R.string.text_parent_folder_3af9479)) }
            ReadFeedback(read) { client.library.destination(folder) }
            listing?.folders.orEmpty().filterNot { it.id in exclude }.forEach { child ->
                TextButton(onClick = { path += child }, enabled = !busy) { Icon(Icons.Outlined.Folder, null); Spacer(Modifier.width(8.dp)); Text(child.name) }
            }
        }
    }, confirmButton = {
        TextButton(onClick = { choose(path) }, enabled = !busy && folder != unchanged && listing != null && (read is ReadState.Ready || read is ReadState.Empty)) { Text(action) }
    }, dismissButton = { TextButton(onClick = close, enabled = !busy) { Text(str(R.string.text_cancel_998b9c4)) } })
}

@Composable internal fun FileNameDialog(client: ClientController, parent: String, target: FileActionTarget?, close: () -> Unit, saved: () -> Unit) {
    val state by client.state.collectAsStateWithLifecycle()
    val busy = state.working(target?.let { "files:rename:" + it.id } ?: "files:create-folder")
    var name by remember(target?.id, parent) { mutableStateOf(target?.name.orEmpty()) }
    var failure by remember { mutableStateOf<String?>(null) }
    val clean = name.trim()
    val valid = clean.isNotEmpty() && clean != "." && clean != ".." && clean.codePointCount(0, clean.length) <= 255 &&
        clean.none { it == '/' || it == '\\' || it.isISOControl() }
    val canSave = valid && clean != target?.name && !busy
    fun save() {
        if (canSave) client.library.saveName(target?.id, parent, clean, { failure = it }, saved)
    }
    AlertDialog(onDismissRequest = { if (!busy) close() }, title = { Text(str(if (target == null) R.string.text_new_folder_3b1862d else R.string.text_rename_dd3a2f7)) }, text = {
        val message = when {
            name.isNotEmpty() && !valid -> str(R.string.file_name_rule)
            else -> failure?.let(::fileActionError)
        }
        OutlinedTextField(name, { name = it; failure = null }, Modifier.fillMaxWidth(), singleLine = true, enabled = !busy,
            label = { Text(str(R.string.text_name_dd4dc4c)) }, isError = message != null,
            supportingText = message?.let { { Text(it) } }, keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { save() }))
    }, confirmButton = { TextButton(onClick = ::save, enabled = canSave) { Text(str(if (target == null) R.string.text_create_7570cfc else R.string.text_save_ec8e6d5)) } },
        dismissButton = { TextButton(onClick = close, enabled = !busy) { Text(str(R.string.text_cancel_998b9c4)) } })
}
