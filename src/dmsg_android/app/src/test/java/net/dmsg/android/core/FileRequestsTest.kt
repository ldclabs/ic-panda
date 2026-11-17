package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class FileRequestsTest {
    @Test fun batchValidationCountsUtf8BytesAndKeepsIndependentNames() {
        assertEquals(listOf("Ada", "Lee"), RequestDraft(names = " Ada \n\n Lee ").labels)
        assertFalse(RequestDraft(title = "中".repeat(171)).valid)
        assertTrue(RequestDraft(title = "中".repeat(170)).valid)
        assertFalse(RequestDraft(title = "Documents", names = (1..51).joinToString("\n")).valid)
    }

    @Test fun unknownOrWithdrawnUploadsCannotBeSavedAndAmountsAreExact() {
        val row = """{"upload_id":"original","request_id":"request","state":"unreadable","name":null,"mime":null,"size":null,"sender":null,"replaces":null,"stored_at":1800000000000,"bytes":9007199254740993,"saved_file":null}"""
        val file = parseReceivedFile(Json.parseToJsonElement(row))
        assertFalse(file.canSave); assertTrue(file.canDelete)
        assertEquals(9007199254740993uL, file.bytes)
        assertFalse(file.copy(state = "withdrawn").canDelete)
        assertFalse(file.copy(state = "unknown").canSave)
        assertTrue(runCatching { parseReceivedFile(Json.parseToJsonElement(row.replace("\"unreadable\"", "\"saved\""))) }.isFailure)
        assertTrue(runCatching { parseReceivedFile(Json.parseToJsonElement(row.replace("9007199254740993", "\"9007199254740993\""))) }.isFailure)
    }

    @Test fun completedCreationMustNameItsOriginalRequest() {
        val row = """{"id":"original","batch":"batch","title":"Documents","instructions":"","label":"Ada","expires_in_ms":1209600000,"max_total_bytes":null,"stage":"checking","can_discard":false,"created_at":1,"error":"EXECUTION_UNKNOWN","link":null}"""
        val intent = parseFileRequestIntent(Json.parseToJsonElement(row))
        assertEquals("original", intent.id); assertNull(intent.maxTotal); assertNull(intent.link)
        assertFalse(intent.canDiscard)
        assertTrue(runCatching { parseFileRequestIntent(Json.parseToJsonElement(row.replace("\"checking\"", "\"complete\""))) }.isFailure)
    }
}
