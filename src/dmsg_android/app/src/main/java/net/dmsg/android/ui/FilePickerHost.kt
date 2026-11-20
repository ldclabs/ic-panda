package net.dmsg.android.ui

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContract
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import net.dmsg.android.core.ClientController
import net.dmsg.android.core.LibraryFile

internal class FilePickers(val documents: () -> Unit, val photos: (remainingAttachments: Int?) -> Unit, val camera: () -> Unit, val export: (LibraryFile) -> Unit, val profilePhoto: () -> Unit)
internal val LocalFilePickers = staticCompositionLocalOf<FilePickers> { error("File picker host missing") }

private class ExportDocument : ActivityResultContract<LibraryFile, Uri?>() {
    override fun createIntent(context: Context, input: LibraryFile) = Intent(Intent.ACTION_CREATE_DOCUMENT)
        .addCategory(Intent.CATEGORY_OPENABLE).setType(input.mime.ifBlank { "application/octet-stream" }).putExtra(Intent.EXTRA_TITLE, input.name)
    override fun parseResult(resultCode: Int, intent: Intent?): Uri? = intent?.data?.takeIf { resultCode == Activity.RESULT_OK }
}

/** Registered outside private screens: the privacy cover must not unregister an OS result. */
@Composable internal fun rememberFilePickers(client: ClientController): FilePickers {
    val context = LocalContext.current
    val documents = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments(), client.transfers::importResults)
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(), client.transfers::importResults)
    val onePhoto = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia(), client.transfers::importResult)
    val profilePhoto = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia(), client.profile::photoResult)
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture(), client.transfers::captureResult)
    val export = rememberLauncherForActivityResult(remember { ExportDocument() }, client.transfers::exportResult)
    fun capture() { client.transfers.beginCapture()?.let { uri ->
        runCatching { camera.launch(uri) }.onFailure { client.transfers.captureResult(false); client.showError("CAMERA_UNAVAILABLE") }
    } }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) {
            val stamp = client.transfers.pendingImportStamp()
            if (stamp != null) client.deliverExternalResult("camera-launch", stamp, discard = { client.transfers.importResult(null) }) { capture() }
        } else { client.transfers.importResult(null); client.showError("CAMERA_PERMISSION_DENIED") }
    }
    return FilePickers(
        documents = { runCatching { documents.launch(arrayOf("*/*")) }.onFailure { client.transfers.importResult(null); client.showError("FILE_UNAVAILABLE") } },
        photos = { remaining -> runCatching {
            val request = PickVisualMediaRequest.Builder().setMediaType(ActivityResultContracts.PickVisualMedia.ImageAndVideo)
            if (remaining == 1) onePhoto.launch(request.build())
            else {
                if (remaining != null) request.setMaxItems(remaining)
                photos.launch(request.build())
            }
        }.onFailure { client.transfers.importResult(null); client.showError("FILE_UNAVAILABLE") } },
        camera = {
            if (ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) capture()
            else permission.launch(Manifest.permission.CAMERA)
        },
        profilePhoto = { if (client.profile.beginPhoto()) runCatching { profilePhoto.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }.onFailure { client.profile.photoResult(null); client.showError("FILE_UNAVAILABLE") } },
        export = { file -> runCatching { export.launch(file) }.onFailure { client.transfers.exportResult(null); client.showError("FILE_UNAVAILABLE") } },
    )
}
