package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class NoticeDeliveryTest {
    private val id = "ab".repeat(32)
    private fun plan(category: String = "message", kind: String = "channel", count: String = "null") = """{"system_id":"$id","delivery_id":"${"cd".repeat(32)}","account":"040g2081040g2081040g","category":"$category","target":{"kind":"$kind","id":"$id"},"covered":[{"id":"$id","version":"9007199254740993","stage":1}],"count":$count,"sound":false,"in_app":true,"update":false}"""
    @Test fun unknownCountsUseGenericCopyAndVerifiedLargeCountsKeepTheirDigits() {
        assertTrue(parseNoticeDelivery(Json.parseToJsonElement(plan())).generic)
        assertFalse(parseNoticeDelivery(Json.parseToJsonElement(plan("security", "recovery"))).generic)
        val counted = parseNoticeDelivery(Json.parseToJsonElement(plan(count = "18446744073709551615")))
        assertEquals(ULong.MAX_VALUE, counted.count); assertEquals(9007199254740993uL, counted.covered.single().version)
        assertFalse(counted.generic)
    }
    @Test fun categoryMismatchAndAmbiguousCoverageCannotBecomeASystemNotification() {
        assertThrows(IllegalStateException::class.java) { parseNoticeDelivery(Json.parseToJsonElement(plan("security", "channel"))) }
        assertThrows(IllegalStateException::class.java) { parseNoticeDelivery(Json.parseToJsonElement(plan().replace("9007199254740993", "01"))) }
        assertThrows(IllegalStateException::class.java) { parseNoticeDelivery(Json.parseToJsonElement(plan().replace("\"sound\":false,", ""))) }
        assertThrows(IllegalStateException::class.java) { parseNoticeBatch(Json.parseToJsonElement("""{"plans":[],"remove":["https://example.com"],"next_due":null}""")) }
    }
}
