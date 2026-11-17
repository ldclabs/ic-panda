package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test
import java.time.Instant

class NotificationModelsTest {
    @Test fun quietHoursRespectOvernightAndBothFallBackHours() {
        val fall = NoticeQuietHours(60, 120, "America/New_York")
        assertEquals(Instant.parse("2026-11-01T07:00:00Z").toEpochMilli().toULong(), fall.until(Instant.parse("2026-11-01T06:30:00Z")))
        val overnight = NoticeQuietHours(1320, 480, "Asia/Shanghai")
        assertEquals(Instant.parse("2026-11-16T00:00:00Z").toEpochMilli().toULong(), overnight.until(Instant.parse("2026-11-15T18:00:00Z")))
        assertNull(overnight.until(Instant.parse("2026-11-16T02:00:00Z")))
        assertFalse(NoticeQuietHours(60, 60, "UTC").valid)
        assertFalse(NoticeQuietHours(60, 120, "Fake/Zone").valid)
        val spring = NoticeQuietHours(120, 240, "America/New_York")
        assertEquals(Instant.parse("2026-03-08T08:00:00Z").toEpochMilli().toULong(), spring.until(Instant.parse("2026-03-08T07:30:00Z")))
    }
    @Test fun reminderVersionsKeepExactCanonicalValuesAndRoutesAreClosed() {
        assertEquals(18446744073709551615uL, canonicalNoticeVersion("18446744073709551615"))
        listOf("01", "-1", "1.0", " 1", "18446744073709551616").forEach { assertTrue(runCatching { canonicalNoticeVersion(it) }.isFailure) }
        for (target in listOf("""{"kind":"url","id":"https://example.com"}""", """{"kind":"channel","id":"../path"}""", """{"kind":"summary","id":"urgent"}""")) assertTrue(runCatching { parseNoticeTarget(Json.parseToJsonElement(target)) }.isFailure)
        val wallet = parseNoticeTarget(Json.parseToJsonElement("""{"kind":"wallet","id":{"wallet":"aaaaa-aa","proposal":"9007199254740993"}}"""))
        assertEquals("9007199254740993", wallet.id)
        assertEquals(wallet, parseNoticeTarget(Json.parseToJsonElement(wallet.json())))
        assertEquals(ULong.MAX_VALUE, NoticeCounts(ULong.MAX_VALUE, 1u, true).total)
    }
    @Test fun preferenceEncodingPreservesAccountConsentAndExactRevision() {
        val global = NoticeGlobal(9007199254740993u, false, null, null, false, "all")
        assertEquals(global, parseNoticeGlobal(Json.parseToJsonElement(global.json()).objectValue()))
        val account = NoticeAccount(9u, false, noticeCategories.associateWith { NoticeCategory(true, it == "security") }, true, mapOf("did:agent:one" to false), 8u)
        assertEquals(account, parseNoticeAccount(Json.parseToJsonElement(account.json()).objectValue()))
        assertFalse(account.enabled)
        assertTrue(runCatching { parseNoticeAccount(Json.parseToJsonElement(account.copy(categories = account.categories - "security").json()).objectValue()) }.isFailure)
    }
}
