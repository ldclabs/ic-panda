package net.dmsg.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import net.dmsg.android.core.publishedAvatarBytes
import net.dmsg.core.MobileEngine
import net.dmsg.core.MobileException
import net.dmsg.core.NoHandle
import net.dmsg.core.ObjectSnapshot
import net.dmsg.core.OperationToken
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ProfileAvatarTest {
    private val url = "https://example.test/avatar.png"
    private val image = byteArrayOf(1, 2, 3)

    @Test fun editorLoadsPublishedAvatarOnACleanCache() = runBlocking {
        val core = AvatarEngine(publishedUrl = url)
        val result = OperationToken().use { core.publishedAvatarBytes(url, it) }
        assertArrayEquals(image, result)
        assertEquals(1, core.refreshes)
        assertEquals(1, core.downloads)
    }

    @Test fun editorDoesNotRequestAnUnpublishedAvatar() = runBlocking {
        val core = AvatarEngine(publishedUrl = "https://example.test/previous.png")
        assertNull(OperationToken().use { core.publishedAvatarBytes(url, it) })
        assertEquals(1, core.refreshes)
        assertEquals(0, core.downloads)
    }

    @Test fun cachedAvatarSkipsDirectoryAndDownload() = runBlocking {
        val core = AvatarEngine(publishedUrl = url, cached = image)
        assertArrayEquals(image, OperationToken().use { core.publishedAvatarBytes(url, it) })
        assertEquals(0, core.documentReads)
        assertEquals(0, core.refreshes)
        assertEquals(0, core.downloads)
    }

    @Test fun laggingDirectoryConflictStillReadsConfirmedPublishedAvatar() = runBlocking {
        val core = AvatarEngine(publishedUrl = url, refreshConflict = true)
        assertArrayEquals(image, OperationToken().use { core.publishedAvatarBytes(url, it) })
        assertEquals(1, core.refreshes)
        assertEquals(1, core.downloads)
    }

    private inner class AvatarEngine(
        private val publishedUrl: String,
        private val cached: ByteArray? = null,
        private val refreshConflict: Boolean = false,
    ) : MobileEngine(NoHandle) {
        var documentReads = 0
        var refreshes = 0
        var downloads = 0

        override suspend fun cachedProfileAvatar(url: String): ByteArray? = cached

        override suspend fun cachedDisplay(kind: String, id: String): ObjectSnapshot {
            assertEquals("profile-document", kind)
            documentReads++
            return ObjectSnapshot(0u, """{"state":"missing","data":null}""")
        }

        override suspend fun refreshDisplay(kind: String, id: String, force: Boolean, operation: OperationToken): ObjectSnapshot {
            assertEquals("profile-document", kind)
            assertEquals("", id)
            assertTrue(force)
            refreshes++
            if (refreshConflict) throw MobileException.Failure("VERSION_CONFLICT")
            return ObjectSnapshot(0u, """{"state":"present","observed_at_ms":1,"stale":false,"data":{"avatar_url":"$publishedUrl","links":[],"updated_at":1}}""")
        }

        override suspend fun profileAvatar(url: String): ByteArray? {
            assertEquals(publishedUrl, url)
            assertEquals(1, refreshes)
            downloads++
            return image
        }
    }
}
