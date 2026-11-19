package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class FileLinksTest {
    private val link = """{"share_id":"share","source":{"type":"file_version","file":"file","version":"v1"},"file":"file","version":"v1","name":"Report","size":9007199254740993,"url":"https://dmsg.net/s/share#fixture","expires_at":18446744073709551615,"max_downloads":5,"password":false,"show_profile":false,"state":"active","downloads":3,"last_checked_at":null}"""
    @Test fun vaultLinkManagementKeepsTheSourceItemAcrossRevisions() {
        val row = Json.parseToJsonElement(link).objectValue().toMutableMap()
        row["source"] = Json.parseToJsonElement("""{"type":"vault_snapshot","item_id":"original","item_revision":"previous"}""")
        val parsed = parseFileLink(kotlinx.serialization.json.JsonObject(row))
        assertEquals("vault_snapshot", parsed.source)
        assertEquals("original", parsed.vaultItem)
    }

    @Test fun disclosureIsOffByDefaultAndMissingContractFieldsAreNotInvented() {
        assertFalse(FileLinkDraft().showProfile)
        val parsed = parseFileLink(Json.parseToJsonElement(link))
        assertEquals(9007199254740993uL, parsed.size)
        assertEquals(ULong.MAX_VALUE, parsed.expiresAt)
        assertFalse(parsed.showProfile)
        assertFalse(parsed.copy(state = "unknown_future_state").active)
        assertTrue(runCatching { parseFileLink(Json.parseToJsonElement(link.replace("\"show_profile\":false,", ""))) }.isFailure)
    }
    @Test fun uncertainCreationKeepsItsOriginalIdAndCannotPretendToBeComplete() {
        val raw = """{"id":"original","file":"file","version":"v1","name":"Report","mime":"text/plain","size":12,"backup":null,"expires_in_ms":604800000,"max_downloads":5,"password":true,"show_profile":false,"stage":"checking","created_at":1,"updated_at":2,"error":"EXECUTION_UNKNOWN","can_discard":false,"link":null}"""
        val job = parseFileLinkIntent(Json.parseToJsonElement(raw))
        assertEquals("original", job.id); assertEquals("v1", job.version)
        assertTrue(job.password); assertFalse(job.canDiscard); assertNull(job.link)
        assertTrue(runCatching { parseFileLinkIntent(Json.parseToJsonElement(raw.replace("\"checking\"", "\"complete\""))) }.isFailure)
    }
}
