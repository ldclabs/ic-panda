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
    @Test fun tomorrowMorningUsesConfiguredTimezoneAtEightAm() {
        val now = java.time.Instant.parse("2026-11-26T15:30:00Z")
        val utc = tomorrowMorning("UTC", now)
        val tokyo = tomorrowMorning("Asia/Tokyo", now)
        assertEquals(java.time.Instant.parse("2026-11-27T08:00:00Z").toEpochMilli().toULong(), utc)
        // 15:30Z is 00:30 on Nov 27 in Asia/Tokyo, so next calendar day 08:00 is Nov 28 08:00+09:00 = Nov 27 23:00Z.
        assertEquals(java.time.Instant.parse("2026-11-27T23:00:00Z").toEpochMilli().toULong(), tokyo)
    }
}
