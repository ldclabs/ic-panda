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

    @Test fun unreadCountsAlignWithCoreNotificationRules() {
        val letters = listOf(
            Letter("1", "peer", "hi", read = false, archived = false, state = "visible", createdAt = 1u),
            Letter("2", "me", "self", read = false, archived = false, state = "visible", createdAt = 2u),
            Letter("3", "blocked", "spam", read = false, archived = false, state = "visible", createdAt = 3u),
            Letter("4", "peer", "aborted", read = false, archived = false, state = "aborted", createdAt = 4u),
            Letter("5", "peer", "read", read = true, archived = false, state = "visible", createdAt = 5u),
            Letter("6", "peer", "archived", read = false, archived = true, state = "visible", createdAt = 6u),
        )
        assertEquals(1, unreadLetterCount(letters, account = "me", blocked = setOf("blocked")))
        assertEquals(2, unreadLetterCount(letters, account = "me", blocked = emptySet()))

        fun channel(id: String, unread: ULong, muted: Boolean = false, status: String = "active") = net.dmsg.core.ChannelItem(
            id = id, name = id, unread = unread, draft = "", preview = "", lastAt = 1u,
            muted = muted, status = status, canPublish = true, canInvite = false, pendingSends = 0u, readThrough = 0u, news = false,
        )
        val channels = listOf(
            channel("active", 2u),
            channel("muted", 5u, muted = true),
            channel("archived", 3u, status = "archived"),
            channel("left", 4u, status = "left"),
        )
        assertEquals(2, unreadConversationCount(channels))
    }
}
