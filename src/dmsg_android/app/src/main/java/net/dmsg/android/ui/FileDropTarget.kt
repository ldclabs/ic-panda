package net.dmsg.android.ui

import android.app.Activity
import android.content.ClipData
import android.content.ContextWrapper
import android.content.Intent
import androidx.compose.foundation.border
import androidx.compose.foundation.draganddrop.dragAndDropTarget
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draganddrop.DragAndDropEvent
import androidx.compose.ui.draganddrop.DragAndDropTarget
import androidx.compose.ui.draganddrop.toAndroidDragEvent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import net.dmsg.android.core.ClientController
import java.util.concurrent.atomic.AtomicBoolean

/** Drops only become the same bounded encrypted intake as Android shares. */
internal fun droppedFiles(clip: ClipData?): Intent? {
    if (clip == null || clip.itemCount !in 1..10) return null
    val uris = (0 until clip.itemCount).map { index ->
        val item = clip.getItemAt(index)
        if (item.intent != null) return null
        item.uri?.takeIf { it.scheme == "content" && it.authority != null && it.toString().length <= 4096 } ?: return null
    }.distinct()
    return Intent(Intent.ACTION_SEND_MULTIPLE).apply {
        type = "application/octet-stream"
        putParcelableArrayListExtra(Intent.EXTRA_STREAM, ArrayList(uris))
    }
}

@Composable internal fun Modifier.fileDropTarget(client: ClientController, enabled: Boolean): Modifier {
    val context = LocalContext.current
    val activity = remember(context) {
        var owner = context
        while (owner is ContextWrapper && owner !is Activity) owner = owner.baseContext
        owner as? Activity
    }
    val active by rememberUpdatedState(enabled)
    var hovering by remember { mutableStateOf(false) }
    val target = remember(client, activity) { object : DragAndDropTarget {
        override fun onEntered(event: DragAndDropEvent) { hovering = active }
        override fun onExited(event: DragAndDropEvent) { hovering = false }
        override fun onEnded(event: DragAndDropEvent) { hovering = false }
        override fun onDrop(event: DragAndDropEvent): Boolean {
            hovering = false
            if (!active || activity == null) return false
            val original = event.toAndroidDragEvent()
            val intent = droppedFiles(original.clipData) ?: return false
            val permissions = try { activity.requestDragAndDropPermissions(original) }
                catch (_: Exception) { client.showError("INVALID_INPUT"); return false }
            val released = AtomicBoolean()
            fun release() { if (released.compareAndSet(false, true)) runCatching { permissions?.release() } }
            return try {
                client.incoming.receive(intent, ::release).also { handled ->
                    if (handled) {
                        client.incoming.routeIfReady()
                        client.incoming.intake.value.error?.let(client::showError)
                    }
                }
            } catch (_: Exception) { release(); client.showError("INVALID_INPUT"); false }
        }
    } }
    return this.then(if (hovering && enabled) Modifier.border(2.dp, MaterialTheme.colorScheme.primary) else Modifier)
        .dragAndDropTarget(shouldStartDragAndDrop = { active && activity != null }, target = target)
}
