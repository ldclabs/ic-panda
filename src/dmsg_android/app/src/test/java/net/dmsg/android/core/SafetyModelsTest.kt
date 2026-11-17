package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class SafetyModelsTest {
    @Test fun missingReceivingRulesAreNotReportedAsClosedAndPaidRulesStayDistinct() {
        assertNull(parseReceivingSnapshot(Json.parseToJsonElement("""{"state":"missing","data":null,"observed_at_ms":null,"stale":true}""")))
        val absent = parseReceivingSnapshot(Json.parseToJsonElement("""{"state":"absent","data":null,"observed_at_ms":10,"stale":false}"""))!!
        assertEquals("closed", absent.mode); assertNull(absent.capacity)
        val paid = parseReceivingSnapshot(Json.parseToJsonElement("""{"state":"present","data":{"mode":"paid","allow":[],"capacity":9007199254740993},"observed_at_ms":10,"stale":true}"""))!!
        assertEquals("paid", paid.mode); assertEquals(9007199254740993uL, paid.capacity)
        assertFalse(ReceivingDraft("paid").valid)
        assertFalse(ReceivingDraft("contacts", listOf("040g2081040g2081040g", "040g2081040g2081040g")).valid)
    }
    @Test fun reportReviewPreservesDisclosedSelectionAndUtf8Budget() {
        val target = ReportTarget("account", "040g2081040g2081040g", "040g2081040g2081040g")
        assertTrue(ReportDraft(target, note = "中".repeat(666)).valid)
        assertFalse(ReportDraft(target, note = "中".repeat(667)).valid)
        val review = parseReportReview(Json.parseToJsonElement("""{"id":"original","state":"pending","created_at":1,"payload":{"kind":"account","target_account":"040g2081040g2081040g","resource":"040g2081040g2081040g","reason":"spam","note":"Reviewed","disclosed_text":"Exactly selected"}}"""))
        assertEquals("original", review.report.id); assertEquals("Exactly selected", review.disclosedText)
    }
}
