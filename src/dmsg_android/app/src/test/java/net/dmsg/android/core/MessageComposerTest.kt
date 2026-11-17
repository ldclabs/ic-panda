package net.dmsg.android.core

import kotlinx.serialization.json.Json
import net.dmsg.core.ComposerAttachment
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
        val ledger = DraftLedger()
        val authored = ledger.edit("account", "channel", result)
        assertEquals(listOf(second), authored.native().attachments)
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
