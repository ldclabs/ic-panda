package net.dmsg.android.core

import org.junit.Assert.*
import org.junit.Test

class RemoteNoticeTest {
    private val now = 1_800_000_000_000L
    private fun payload() = mapOf("version" to "1", "route" to "a".repeat(128), "registration" to "b".repeat(64), "event_id" to "c".repeat(64), "system_id" to "d".repeat(64), "category" to "message", "generic" to "message", "sound" to "true", "expires" to (now + 60_000).toString())
    @Test fun acceptsOnlyBoundedOpaqueRemotePlans() {
        assertNotNull(RemoteNotice.parse(payload(), now))
        val invalid = listOf(payload() + ("body" to "content"), payload() - "registration", payload() + ("route" to "local:" + "a".repeat(64)), payload() + ("route" to "https://dmsg.net"), payload() + ("registration" to "B".repeat(64)), payload() + ("generic" to "security"), payload() + ("sound" to "1"), payload() + ("version" to "2"), payload() + ("expires" to now.toString()), payload() + ("expires" to (now + 86_460_001).toString()), payload() + ("expires" to "0" + (now + 100).toString()))
        invalid.forEach { assertNull(RemoteNotice.parse(it, now)) }
    }
}
