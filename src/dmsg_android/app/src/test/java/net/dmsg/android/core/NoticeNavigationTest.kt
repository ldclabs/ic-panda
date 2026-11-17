package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class NoticeNavigationTest {
    @Test fun routesAcceptOnlyCurrentOpaqueLookupsAndNeverAnAccountOrAction() {
        assertNotNull(NoticeRoute.parse("ab".repeat(64)))
        assertNotNull(NoticeRoute.parse("local:" + "ab".repeat(32)))
        listOf(null, "ab".repeat(32), "local:" + "ab".repeat(64), "AB".repeat(64), "https://example.com", "ab".repeat(64) + "?approve=true", "local:../request").forEach { assertNull(it, NoticeRoute.parse(it)) }
    }
    @Test fun resolvedDestinationsKeepTheOriginalAccountAndRejectUnknownActions() {
        val account = "040g2081040g2081040g"
        val hash = "ab".repeat(32)
        val value = parseNoticeDestination(Json.parseToJsonElement("""{"account":"$account","target":{"kind":"approval","id":"$hash"},"ended":true}"""))
        assertEquals(account, value.account); assertTrue(value.ended); assertEquals(NoticeTarget("approval", hash), value.target)
        assertThrows(IllegalStateException::class.java) { parseNoticeDestination(Json.parseToJsonElement("""{"account":"$account","target":{"kind":"approve","id":"$hash"},"ended":false}""")) }
        assertThrows(IllegalStateException::class.java) { parseNoticeDestination(Json.parseToJsonElement("""{"account":"$account","target":{"kind":"approval","id":"$hash"}}""")) }
    }
}
