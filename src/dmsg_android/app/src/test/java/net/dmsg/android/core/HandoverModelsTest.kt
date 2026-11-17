package net.dmsg.android.core

import kotlinx.serialization.json.*
import net.dmsg.android.platform.TransferAddress
import org.junit.Assert.*
import org.junit.Test

class HandoverModelsTest {
    @Test fun addressesAreExplicitEndpointsNotUrlsAndSessionsAreExact() {
        assertEquals(TransferAddress("192.168.1.20", 52000u), TransferAddress.parse("192.168.1.20:52000"))
        assertEquals("[fe80::1%wlan0]:52000", TransferAddress.parse("[fe80::1%wlan0]:52000")?.text())
        assertEquals(TransferAddress("receiver.local", 1234u), TransferAddress.parse("receiver.local:1234"))
        for (value in listOf("https://receiver:20", "user@receiver:20", "receiver:0", "receiver:65536", "receiver:12/a", "receiver:12?x", "receiver:12#x", "fe80::1:20", ":20")) assertNull(value, TransferAddress.parse(value))
        assertFalse(DeviceTransferDraft("ab".repeat(32), "cd".repeat(32)).valid(null))
        assertFalse(DeviceTransferDraft("ab".repeat(16), "cd".repeat(32), "not an address").valid(null))
        assertTrue(DeviceTransferDraft("ab".repeat(16), "cd".repeat(32)).valid(null))
        assertTrue(DeviceTransferDraft("ab".repeat(16)).valid("ef".repeat(32)))
    }

    @Test fun aSkippedStepDoesNotBecomeEvidenceAndRevocationWaitsForAvailability() {
        fun make(steps: List<String>) = buildJsonArray { add(buildJsonObject {
            put("id", "ab".repeat(32)); put("side", "successor"); put("account", "040g2081040g2081040g")
            put("predecessor", "cd".repeat(32)); put("successor", "ef".repeat(32)); put("created_at", 1)
            put("cancelled_at", JsonNull); putJsonArray("available") { add("wallets") }
            putJsonArray("steps") { steps.forEach { step -> add(buildJsonObject {
                put("step", step); put("settled", step == "content")
                put("evidence", JsonNull); putJsonArray("skipped") { if (step == "content") add("library") }
            }) } }
        }) }
        val row = parseHandovers(make(Handover.STEPS.toList())).single()
        assertFalse(row.complete)
        assertTrue(row.steps.single { it.name == "content" }.left)
        assertFalse(row.steps.single { it.name == "content" }.completed)
        assertEquals(listOf("library"), row.leftBehind)
        assertFalse(row.canRun("revoke"))
        assertTrue(row.canRun("wallets"))
        assertFalse(row.copy(cancelledAt = 2u).canRun("wallets"))
        assertTrue(runCatching { parseHandovers(make(listOf("content"))) }.isFailure)
    }
}
