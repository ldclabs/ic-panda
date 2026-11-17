package net.dmsg.android.platform

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.File
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

class IncomingShareStoreTest {
    @get:Rule val temporary = TemporaryFolder()
    private class TestBox(seed: Byte = 7) : IncomingSealer {
        private val key = SecretKeySpec(ByteArray(32) { seed }, "AES")
        override fun seal(scope: String, bytes: ByteArray): ByteArray {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key); updateAAD(scope.toByteArray()) }
            return cipher.iv + cipher.doFinal(bytes)
        }
        override fun open(scope: String, bytes: ByteArray): ByteArray = Cipher.getInstance("AES/GCM/NoPadding").run {
            init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, bytes.copyOfRange(0, 12))); updateAAD(scope.toByteArray()); doFinal(bytes, 12, bytes.size - 12)
        }
    }
    private val account = "040g2081040g2081040g"
    private val source = CipherTransferStore.digest("sample provider".toByteArray())

    @Test fun intakeStaysEncryptedAndAClaimedImportRetainsItsAccountFolderAndReceipt() {
        val root = temporary.newFolder()
        val data = "Private sample phrase stored only as ciphertext.".toByteArray()
        val store = IncomingShareStore(root, TestBox())
        val entry = store.stage(ByteArrayInputStream(data), "Sample draft.txt", "text/plain", data.size.toLong(), source) { true }!!
        for (file in root.walkTopDown().filter(File::isFile)) {
            assertFalse(file.name, file.readBytes().toString(Charsets.ISO_8859_1).contains("Private sample phrase"))
            assertFalse(file.name, file.readBytes().toString(Charsets.ISO_8859_1).contains("Sample draft.txt"))
        }
        val reopened = IncomingShareStore(root, TestBox())
        assertEquals(entry, reopened.list().single())
        assertTrue(reopened.pendingReview())
        reopened.acknowledgeReview()
        assertFalse(reopened.pendingReview())
        val claimed = reopened.claim(entry.id, account, "chosen-folder")
        assertEquals(claimed, reopened.claim(entry.id, account, "different-folder"))
        assertTrue(runCatching { reopened.claim(entry.id, "081040g2081040g20810", "root") }.isFailure)
        val target = File(temporary.newFolder(), "import.txt")
        reopened.materialize(claimed, target) { true }
        assertArrayEquals(data, target.readBytes())
        reopened.remove(entry.id)
        assertNull(reopened.stage(ByteArrayInputStream(data), "Sample draft.txt", "text/plain", data.size.toLong(), source) { true })
        assertTrue(reopened.list().isEmpty())
    }

    @Test fun partialOrAlteredSourceCannotBecomeAnImportableFile() {
        val root = temporary.newFolder()
        val store = IncomingShareStore(root, TestBox())
        val data = ByteArray(IncomingShareStore.PART + 3) { (it % 251).toByte() }
        assertTrue(runCatching { store.stage(ByteArrayInputStream(data), "changed.dat", "application/octet-stream", 12, source) { true } }.isFailure)
        assertTrue(store.list().isEmpty())
        val item = store.stage(ByteArrayInputStream(data), "../file.dat", "application/octet-stream", data.size.toLong(), source) { true }!!
        assertFalse(item.name.contains('/'))
        assertEquals(2, item.parts)
        val claimed = store.claim(item.id, account, "root")
        File(root, item.id + "/1").writeBytes(ByteArray(31))
        val destination = File(temporary.newFolder(), "partial")
        assertTrue(runCatching { store.materialize(claimed, destination) { true } }.isFailure)
        assertFalse(destination.exists())
        assertTrue(runCatching { IncomingShareStore(root, TestBox(8)).list() }.isFailure)
    }

    @Test fun aBlockedProviderDoesNotHoldTheQueueLockOrResurrectADiscardedCopy() {
        val store = IncomingShareStore(temporary.newFolder(), TestBox())
        val entered = java.util.concurrent.CountDownLatch(1)
        val release = java.util.concurrent.CountDownLatch(1)
        val pool = java.util.concurrent.Executors.newFixedThreadPool(2)
        val input = object : java.io.InputStream() {
            private var supplied = false
            override fun read(): Int = -1
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
                if (supplied) return -1
                entered.countDown()
                check(release.await(5, java.util.concurrent.TimeUnit.SECONDS))
                buffer[offset] = 'x'.code.toByte(); supplied = true
                return 1
            }
        }
        val writer = pool.submit(java.util.concurrent.Callable { runCatching { store.stage(input, "blocked.txt", "text/plain", 1, source) { true } } })
        try {
            assertTrue(entered.await(2, java.util.concurrent.TimeUnit.SECONDS))
            val reader = pool.submit(java.util.concurrent.Callable {
                val entry = store.list().single()
                assertFalse(entry.complete)
                store.remove(entry.id)
            })
            reader.get(2, java.util.concurrent.TimeUnit.SECONDS)
            release.countDown()
            assertTrue(writer.get(2, java.util.concurrent.TimeUnit.SECONDS).isFailure)
            assertTrue(store.list().isEmpty())
        } finally { release.countDown(); pool.shutdownNow() }
    }

    @Test fun providerAndQueueLimitsAreAppliedBeforeSavingAnotherSource() {
        val store = IncomingShareStore(temporary.newFolder(), TestBox())
        assertTrue(runCatching { store.stage(ByteArrayInputStream(byteArrayOf()), "large", "application/octet-stream", IncomingShareStore.MAX_FILE + 1, source) { true } }.isFailure)
        repeat(10) {
            val data = "sample $it".toByteArray()
            store.stage(ByteArrayInputStream(data), "$it.txt", "text/plain", data.size.toLong(), CipherTransferStore.digest(data)) { true }
        }
        assertEquals(10, store.list().size)
        assertTrue(runCatching { store.stage(ByteArrayInputStream(byteArrayOf()), "another", "text/plain", 0, source) { true } }.isFailure)
        assertEquals(10, store.list().size)
    }
}
