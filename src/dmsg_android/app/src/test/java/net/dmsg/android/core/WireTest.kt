package net.dmsg.android.core
import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class WireTest {
    @Test fun unsignedVersionsNeverPassThroughFloatingPoint() {
        val value = Json.parseToJsonElement("""{"version":18446744073709551615,"amount":"9007199254740993"}""").objectValue()
        assertEquals(ULong.MAX_VALUE, value.unsigned("version"))
        assertEquals("9007199254740993", value.required("amount"))
        assertNull(value.text("unread"))
    }
    @Test fun malformedOrNegativeVersionsFailInsteadOfBecomingZero() {
        for (input in listOf("-1", "1.5", "18446744073709551616", "true", "1e3", "\"+1\"", "\"1\"", "{}")) {
            val value = Json.parseToJsonElement("{\"version\":" + input + "}").objectValue()
            assertThrows(IllegalStateException::class.java) { value.unsigned("version") }
        }
    }

    @Test fun absentOrMalformedListsNeverBecomeSuccessfulEmptyResults() {
        for (input in listOf("{}", "{\"folders\":[],\"files\":null}", "{\"folders\":[],\"files\":[7]}")) {
            assertThrows(IllegalStateException::class.java) { parseFiles(input) }
        }
        assertTrue(parseFiles("{\"folders\":[],\"files\":[]}").second.isEmpty())
        assertThrows(IllegalStateException::class.java) { parseLetters("{}") }
        val wrong = Json.parseToJsonElement("{\"flag\":\"false\",\"text\":12}").objectValue()
        assertThrows(IllegalStateException::class.java) { wrong.flag("flag") }
        assertThrows(IllegalStateException::class.java) { wrong.required("text") }
    }

    @Test fun approvalDeadlinesUseTheActualContractFieldWithoutTruncation() {
        val rows = parseApprovals("""[{"id":"a","digest":"b","type":"account","state":"awaiting_user","source":"owner","risk":"high","expires_at":18446744073709551615,"mobile_supported":true}]""")
        assertEquals(ULong.MAX_VALUE, rows.single().expiresAt)
        assertThrows(IllegalStateException::class.java) { parseApprovals("""[{"id":"a","digest":"b","type":"account","state":"awaiting_user","source":"owner","risk":"high","expiresAt":1,"mobile_supported":true}]""") }
    }
}
