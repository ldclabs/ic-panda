package net.dmsg.android.core
import org.junit.Assert.*
import org.junit.Test
class DraftLedgerTest {
    @Test fun oldSaveCannotAcknowledgeNewTyping() {
        val ledger = DraftLedger()
        val first = ledger.edit("a", "channel", "one")
        val second = ledger.edit("a", "channel", "one two")
        ledger.saved(first)
        assertEquals(listOf(second), ledger.pending("a"))
        ledger.saved(second)
        assertTrue(ledger.pending("a").isEmpty())
    }
    @Test fun accountTransitionDoesNotClearAnotherAccountsDraft() {
        val ledger = DraftLedger()
        ledger.edit("a","same","secret a")
        val b = ledger.edit("b","same","secret b")
        ledger.clear("a")
        assertEquals(listOf(b),ledger.pending("b"))
        assertTrue(ledger.pending("a").isEmpty())
    }
}
