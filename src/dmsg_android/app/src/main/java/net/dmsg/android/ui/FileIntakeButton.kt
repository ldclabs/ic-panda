package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import net.dmsg.android.core.ClientController

/** All sources enter the same bounded, session-owned staging and encryption path. */
@Composable fun FileIntakeButton(client: ClientController, folder: String = "root", channel: String? = null,
    enabled: Boolean = true, chooseExisting: (() -> Unit)? = null, fab: Boolean = false, expanded: Boolean = true) {
    val context = LocalContext.current
    var menu by remember { mutableStateOf(false) }
    val pickers = LocalFilePickers.current
    fun prepare(): Boolean { menu = false; return client.transfers.beginImport(folder, channel) }
    Box {
        if (fab) ExtendedFloatingActionButton(onClick = { menu = true }, expanded = expanded,
            icon = { Icon(Icons.Outlined.Add, str(R.string.text_import_files_2fd22c6)) },
            text = { Text(str(R.string.text_import_files_2fd22c6)) })
        else IconButton(onClick = { menu = true }, enabled = enabled) {
            Icon(Icons.Outlined.Add, str(if (channel == null) R.string.text_import_files_2fd22c6 else R.string.text_add_attachment_d348d7f))
        }
        DropdownMenu(menu, { menu = false }) {
            chooseExisting?.let { choose -> DropdownMenuItem(text = { Text(str(R.string.text_attach_from_files_d9fc92f)) }, leadingIcon = { Icon(Icons.Outlined.Folder, null) }, onClick = { menu = false; choose() }) }
            DropdownMenuItem(text = { Text(str(R.string.text_photos_and_videos_d968b5a)) }, leadingIcon = { Icon(Icons.Outlined.PhotoLibrary, null) }, onClick = {
                if (prepare()) pickers.photos(channel?.let { 10 - client.messages.composer(it).attachments.size })
            })
            DropdownMenuItem(text = { Text(str(R.string.text_choose_documents_2d5a07b)) }, leadingIcon = { Icon(Icons.Outlined.InsertDriveFile, null) }, onClick = {
                if (prepare()) pickers.documents()
            })
            DropdownMenuItem(text = { Text(str(R.string.text_take_a_photo_9626890)) }, leadingIcon = { Icon(Icons.Outlined.CameraAlt, null) }, onClick = {
                if (prepare()) pickers.camera()
            })
            DropdownMenuItem(text = { Text(str(R.string.text_paste_an_image_ee42335)) }, leadingIcon = { Icon(Icons.Outlined.ContentPaste, null) }, onClick = {
                menu = false
                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                val clip = clipboard.primaryClip
                val uri = clip?.takeIf { it.description.hasMimeType("image/*") && it.itemCount > 0 }?.getItemAt(0)?.uri
                if (uri == null) client.showError("NO_IMAGE_IN_CLIPBOARD")
                else if (client.transfers.beginImport(folder, channel)) client.transfers.importResult(uri)
            })
        }
    }
}
