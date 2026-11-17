package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class LetterModelsTest {
    @Test fun limitsCountUtf8BytesAndCanonicalAccountAndInvitation() {
        val draft = LetterDraft(recipient = "040g2081040g2081040g", text = "信".repeat(2000))
        assertEquals(6000, draft.bytes)
        assertTrue(draft.valid)
        assertFalse(draft.copy(text = draft.text + "🧪").valid)
        assertFalse(draft.copy(recipient = "040g2081040g20810401").valid)
        assertFalse(draft.copy(invitation = "AB".repeat(32)).valid)
        assertTrue(draft.copy(invitation = "ab".repeat(32)).valid)
    }

    @Test fun historicalUnknownBodyAndTimeRemainUnknown() {
        val row = parseSentLetters(Json.parseToJsonElement("""[{"id":"id","recipient":"account","text":null,"created_at":null,"state":"unknown","error":null,"pending":false,"editable":false}]""")).single()
        assertNull(row.text); assertNull(row.createdAt); assertEquals("unknown", row.state)
    }

    @Test fun draftTimestampDoesNotLoseUnsignedPrecision() {
        val row = parseLetterDrafts(Json.parseToJsonElement("""[{"id":"id","recipient":"partial","text":"draft","invitation":null,"updated_at":18446744073709551615}]""")).single()
        assertEquals(ULong.MAX_VALUE, row.updatedAt)
        assertEquals("partial", row.input().recipient)
    }

    @Test fun malformedDeliveryCannotBecomeSentOrEditable() {
        val malformed = """[{"id":"id","recipient":"account","state":"visible","pending":"false","editable":false}]"""
        assertThrows(IllegalStateException::class.java) { parseSentLetters(Json.parseToJsonElement(malformed)) }
    }
}
