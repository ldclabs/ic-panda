package net.dmsg.android.core

import kotlinx.serialization.json.Json
import net.dmsg.core.ComposerAttachment
import net.dmsg.core.MessageItem
import net.dmsg.core.MessagePage
import org.junit.Assert.*
import org.junit.Test

class MessageComposerTest {
    @Test fun aDelayedQueueResultKeepsNewTextAndNewAttachments() {
        val first = ComposerAttachment("one", "v1", "Original.txt")
        val second = ComposerAttachment("two", "v2", "New.txt")
        val queued = MessageComposer("Original text", listOf(first), ComposerReply(1u, "digest"))
        val current = queued.copy(text = "Typed while queueing", attachments = listOf(first, second))
        val result = remainingComposer(queued, current)
        assertEquals(current.text, result.text); assertEquals(listOf(second), result.attachments); assertNull(result.reply)
        assertEquals(MessageComposer(attachments = listOf(second)), remainingComposer(queued, current.copy(text = queued.text)))
        val ledger = DraftLedger()
        val authored = ledger.edit("account", "channel", result)
        assertEquals(listOf(second), authored.native().attachments)
    }

    @Test fun refreshedMessagesReplaceTheCoveredRangeAndObsoleteSearchHits() {
        fun message(n: Int) = MessageItem(n.toString(), n.toULong(), "author", "message", 0u, null, null, null, false, "digest", null, false, false, false, false)
        val loaded = MessageTimeline("channel", "", (2..5).map(::message), true, 2u)
        val refreshed = MessagePage(1u, listOf(message(3), message(5).copy(text = "edited"), message(6)), true, 3u)
        val joined = loaded.withPage(refreshed, older = false)
        assertEquals(listOf("2", "3", "5", "6"), joined.items.map { it.id })
        assertEquals("edited", joined.items[2].text)
        assertEquals(2uL, joined.before)
        val gap = MessagePage(1u, listOf(message(9)), true, 9u)
        assertEquals(9uL, joined.withPage(gap, older = false).before)
        assertEquals(gap.items, joined.withPage(gap, older = false).items)
        val search = loaded.copy(query = "message").withPage(MessagePage(1u, emptyList(), false, null), older = false)
        assertTrue(search.items.isEmpty())
        assertFalse(search.hasMore)
    }
    @Test fun persistedReplyUsesExactUnsignedSequenceAndMissingSelectionsFail() {
        val value = """[{"channel":"channel","draft":{"text":"draft","attachments":[{"file":"file","version":"v1","name":"Review.txt"}],"reply_to":{"seq":9007199254740993,"digest":"digest"}}}]"""
        val draft = parseMessageComposers(Json.parseToJsonElement(value)).getValue("channel")
        assertEquals(9007199254740993uL, draft.reply?.sequence)
        assertEquals("v1", draft.attachments.single().version)
        assertTrue(runCatching { parseMessageComposers(Json.parseToJsonElement("""[{"channel":"channel","draft":{"text":"draft"}}}]""")) }.isFailure)
    }
    @Test fun messageLimitCountsUtf8AndFilesAreNotImplicitlySent() {
        assertTrue(MessageComposer("中".repeat(5333)).sendable)
        assertFalse(MessageComposer("中".repeat(5334)).sendable)
        assertFalse(MessageComposer().sendable)
        assertTrue(MessageComposer(attachments = listOf(ComposerAttachment("id", "v1", "a.txt"))).sendable)
    }
}
