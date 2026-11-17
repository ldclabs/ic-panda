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
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import net.dmsg.android.core.ClientController

/** All sources enter the same bounded, session-owned staging and encryption path. */
@Composable fun FileIntakeButton(client: ClientController, folder: String = "root", channel: String? = null,
    enabled: Boolean = true, extended: Boolean = false, chooseExisting: (() -> Unit)? = null) {
    val context = LocalContext.current
    var expanded by remember { mutableStateOf(false) }
    val pickers = LocalFilePickers.current
    fun prepare(): Boolean { expanded = false; return client.transfers.beginImport(folder, channel) }
    Box {
        if (extended) ExtendedFloatingActionButton(onClick = { expanded = true }, modifier = Modifier.semantics { contentDescription = str(R.string.text_import_files_2fd22c6) }, icon = { Icon(Icons.Outlined.Add, null) }, text = { Text(str(R.string.text_import_1a200b3)) })
        else IconButton(onClick = { expanded = true }, enabled = enabled) { Icon(Icons.Outlined.Add, str(R.string.text_add_attachment_d348d7f)) }
        DropdownMenu(expanded, { expanded = false }) {
            chooseExisting?.let { choose -> DropdownMenuItem(text = { Text(str(R.string.text_attach_from_files_d9fc92f)) }, leadingIcon = { Icon(Icons.Outlined.Folder, null) }, onClick = { expanded = false; choose() }) }
            DropdownMenuItem(text = { Text(str(R.string.text_photos_and_videos_d968b5a)) }, leadingIcon = { Icon(Icons.Outlined.PhotoLibrary, null) }, onClick = {
                if (prepare()) pickers.photos()
            })
            DropdownMenuItem(text = { Text(str(R.string.text_choose_documents_2d5a07b)) }, leadingIcon = { Icon(Icons.Outlined.InsertDriveFile, null) }, onClick = {
                if (prepare()) pickers.documents()
            })
            DropdownMenuItem(text = { Text(str(R.string.text_take_a_photo_9626890)) }, leadingIcon = { Icon(Icons.Outlined.CameraAlt, null) }, onClick = {
                if (prepare()) pickers.camera()
            })
            DropdownMenuItem(text = { Text(str(R.string.text_paste_an_image_ee42335)) }, leadingIcon = { Icon(Icons.Outlined.ContentPaste, null) }, onClick = {
                expanded = false
                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                val clip = clipboard.primaryClip
                val uri = clip?.takeIf { it.description.hasMimeType("image/*") && it.itemCount > 0 }?.getItemAt(0)?.uri
                if (uri == null) client.showError("NO_IMAGE_IN_CLIPBOARD")
                else if (client.transfers.beginImport(folder, channel)) client.transfers.importResult(uri)
            })
        }
    }
}
