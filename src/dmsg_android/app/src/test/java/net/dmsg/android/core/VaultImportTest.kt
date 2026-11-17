package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class VaultImportTest {
    private val preview = """{"format":"csv","digest":"source-digest","skipped":0,"incomplete":1,"items":[{"index":0,"type":"login","title":"Account","username":"reader","duplicate":false}]}"""

    @Test fun previewIsMetadataOnlyAndPreservesUnsupportedRecordCounts() {
        val result = parseVaultImport(Json.parseToJsonElement(preview))
        assertEquals("source-digest", result.digest)
        assertEquals(1uL, result.incomplete)
        assertEquals("reader", result.items.single().username)
        assertTrue(VaultImportForm("source").selected.isEmpty())
    }

    @Test fun duplicateOrOverflowedIndexesCannotSelectAnotherRecord() {
        assertTrue(runCatching { parseVaultImport(Json.parseToJsonElement(preview.replace("\"index\":0", "\"index\":4294967296"))) }.isFailure)
        val item = """{"index":0,"type":"login","title":"Other","username":"other","duplicate":false}"""
        assertTrue(runCatching { parseVaultImport(Json.parseToJsonElement(preview.replace("]}", ",$item]}"))) }.isFailure)
    }
}
