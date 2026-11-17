package net.dmsg.android.core

import java.util.UUID
import net.dmsg.core.MessageDraft
import net.dmsg.core.MessageDraftStamp

/** Tracks the complete authored snapshot until its exact revision is acknowledged. */
class DraftLedger {
    data class Draft(val account: String, val channel: String, val value: MessageComposer, val revision: Long, val writer: String) {
        val text get() = value.text
        fun native() = MessageDraft(channel, value.text, MessageDraftStamp(writer, revision.toULong()), value.attachments, value.reply?.sequence, value.reply?.digest)
    }
    private val writer = UUID.randomUUID().toString().replace("-", "")
    private var revision = 0L
    private val values = mutableMapOf<Pair<String, String>, Draft>()
    @Synchronized fun edit(account: String, channel: String, text: String): Draft =
        edit(account, channel, MessageComposer(text))
    @Synchronized fun edit(account: String, channel: String, value: MessageComposer): Draft =
        Draft(account, channel, value, ++revision, writer).also { values[account to channel] = it }
    @Synchronized fun text(account: String, channel: String): String? = values[account to channel]?.text
    @Synchronized fun pending(account: String): List<Draft> = values.values.filter { it.account == account }
    @Synchronized fun current(draft: Draft): Boolean = values[draft.account to draft.channel] == draft
    @Synchronized fun saved(draft: Draft) { if (current(draft)) values.remove(draft.account to draft.channel) }
    @Synchronized fun clear(account: String) { values.keys.removeAll { it.first == account } }
}
