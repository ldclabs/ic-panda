package net.dmsg.android

import android.content.ClipData
import android.content.ClipDescription
import android.content.Intent
import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import net.dmsg.android.core.incomingPayload
import net.dmsg.android.ui.droppedFiles
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class FileDropPayloadTest {
    @Test fun onlyBoundedContentUrisBecomeAnIntakeWithNoSenderCommands() {
        val uri = Uri.parse("content://external.files/sample")
        val files = ClipData(ClipDescription("File", arrayOf("text/plain")), ClipData.Item(uri))
        files.addItem(ClipData.Item(uri))
        val intent = checkNotNull(droppedFiles(files))
        assertEquals(listOf(uri), incomingPayload(intent)!!.uris)
        assertEquals(setOf(Intent.EXTRA_STREAM), intent.extras!!.keySet())
        assertNull(intent.component)
        assertNull(intent.data)
        assertNull(intent.clipData)
        assertNull(droppedFiles(ClipData.newPlainText("Text", "do not execute")))
        assertNull(droppedFiles(ClipData.newIntent("Command", Intent("net.dmsg.APPROVE"))))
        for (value in listOf("file:///data/private", "https://dmsg.net/a", "content:/no-authority")) {
            assertNull(droppedFiles(ClipData(ClipDescription("File", arrayOf("*/*")), ClipData.Item(Uri.parse(value)))))
        }
        repeat(9) { files.addItem(ClipData.Item(uri)) }
        assertNull(droppedFiles(files))
    }
}
