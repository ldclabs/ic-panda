package net.dmsg.android.platform

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class CipherTransferStoreTest {
    @get:Rule val temporary = TemporaryFolder()
    private val account = "040g2081040g2081040g"
    private val id = "ab".repeat(32)
    private val bytes = "only sample ciphertext".toByteArray()
    private fun plan(parts: Int = 1): CipherPlan {
        val now = System.currentTimeMillis().toULong()
        return CipherPlan(id, account, "cd".repeat(32), "GET", "ef".repeat(32), now + 60_000u, (0 until parts).map {
            CipherPart(it.toString(), bytes.size.toLong(), CipherTransferStore.digest(bytes), "https://relay.example.test/mobile-transfer/v1/$account/$id/$it?token=" + "12".repeat(32))
        })
    }

    @Test fun planRejectsExpandedAuthorityAndUnboundedCiphertext() {
        val value = plan()
        value.valid(System.currentTimeMillis().toULong())
        val part = value.parts.single()
        for (url in listOf(part.url.replace("https:", "http:"), part.url + "&redirect=https://evil.test", part.url + "#x", part.url.replace("/$id/", "/../../"), part.url.replace("relay.example.test", "user@relay.example.test"))) {
            assertTrue(url, runCatching { value.copy(parts = listOf(part.copy(url = url))).valid() }.isFailure)
        }
        assertTrue(runCatching { value.copy(parts = listOf(part.copy(size = CipherPlan.MAX_PART + 1))).valid() }.isFailure)
        assertTrue(runCatching { value.copy(parts = listOf(part, part)).valid() }.isFailure)
        assertTrue(runCatching { value.copy(parts = listOf(part.copy(slot = "../key"))).valid() }.isFailure)
        assertTrue(runCatching { value.copy(expires = ULong.MAX_VALUE).valid(System.currentTimeMillis().toULong()) }.isFailure)
        val store = CipherTransferStore(temporary.newFolder(), "installation")
        store.start(value.copy(parts = emptyList()), System.currentTimeMillis().toULong())
        assertTrue(store.snapshot()!!.ready)
        assertEquals(0, store.snapshot()!!.completed.size)
    }

    @Test fun restartRetainsProgressButLateAttemptCannotCommitAndCompletionNeedsForegroundVerification() {
        val root = temporary.newFolder()
        val store = CipherTransferStore(root, "installation")
        val plan = plan(2)
        store.start(plan, System.currentTimeMillis().toULong())
        val original = store.retry(System.currentTimeMillis().toULong())
        assertTrue(store.receive(id, original.attempt, "0", bytes))
        store.pause()
        assertFalse(store.receive(id, original.attempt, "1", bytes))
        val reopened = CipherTransferStore(root, "installation")
        assertEquals(setOf("0"), reopened.snapshot()!!.completed)
        val next = reopened.retry(System.currentTimeMillis().toULong())
        assertFalse(store.complete(id, original.attempt, "1"))
        assertTrue(reopened.receive(id, next.attempt, "1", bytes))
        assertTrue(store.snapshot()!!.ready)
        assertFalse(store.fail(id, next.attempt, "late failure", true))
        assertTrue(File(root, "transfer.json").exists())
        assertArrayEquals(bytes, reopened.verifiedPart(plan, "0"))
        assertTrue(runCatching { CipherTransferStore(root, "different-installation").snapshot() }.isFailure)
        reopened.discard(plan)
        assertNull(store.snapshot())
    }

    @Test fun alteredBytesAndExpiredOrRepeatedFailureCannotBePromotedToSuccess() {
        val root = temporary.newFolder()
        val store = CipherTransferStore(root, "installation")
        val plan = plan()
        val now = System.currentTimeMillis().toULong()
        store.start(plan, now)
        val attempt = store.retry(now).attempt
        assertTrue(runCatching { store.receive(id, attempt, "0", bytes + 0) }.isFailure)
        assertTrue(store.snapshot()!!.completed.isEmpty())
        assertTrue(store.receive(id, attempt, "0", bytes))
        File(root, "$id/0").writeBytes(ByteArray(bytes.size))
        assertTrue(runCatching { store.verifiedPart(plan, "0") }.isFailure)
        store.discard(plan)
        store.start(plan, now)
        val retry = store.retry(now)
        repeat(3) { assertTrue(store.fail(id, retry.attempt, "offline", true)) }
        assertFalse(store.fail(id, retry.attempt, "offline", true))
        assertEquals("failed", store.snapshot()!!.phase)
        assertTrue(runCatching { store.retry(plan.expires) }.isFailure)
    }
}
