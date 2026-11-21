package net.dmsg.android.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.draw.clip
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import net.dmsg.android.core.*
import net.dmsg.android.platform.BoundedImages

@Composable internal fun FileThumbnail(client: ClientController, file: LibraryFile) {
    var bitmap by remember(file.id, file.version) { mutableStateOf<Bitmap?>(null) }
    val operation = remember(file.id, file.version) { "file:thumbnail:" + requestId() }
    LaunchedEffect(file.id, file.version, file.local) {
        if (file.mime.startsWith("image/")) client.perform({ core ->
            val value = core.filePreview(file.id, file.version).decode().objectValue()
            value.text("content_base64url")?.takeIf { value.text("kind") == "image" }?.let { BoundedImages.thumbnail(it, 160) }
        }, key = operation, quiet = true) { bitmap = it }
    }
    DisposableEffect(operation) { onDispose { client.cancelOperation(operation); bitmap = null } }
    if (bitmap != null) Image(bitmap!!.asImageBitmap(), null, Modifier.size(40.dp).clip(MaterialTheme.shapes.small), contentScale = ContentScale.Crop)
    else TonalIcon(if (file.mime.startsWith("image/")) Icons.Outlined.Image else Icons.Outlined.Description, MaterialTheme.colorScheme.secondaryContainer)
}
