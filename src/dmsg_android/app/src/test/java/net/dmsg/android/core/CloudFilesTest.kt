package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class CloudFilesTest {
    @Test fun unknownInventoryAndUnknownEntitlementsAreNotEmptySuccessOrZeroQuota() {
        val unknown = parseCloudInventory(Json.parseToJsonElement("[]"), Json.parseToJsonElement("""{"complete":false,"observed_at_ms":null}"""))
        assertFalse(unknown.complete)
        val limit = parseBackupCapacity(Json.parseToJsonElement("""{"backup_max_file_bytes":null,"storage_limit":null,"committed":9007199254740993,"reserved":7}"""))
        assertNull(limit.fileLimit); assertNull(limit.remaining)
        assertEquals(9007199254740993uL, limit.committed)
        assertTrue(runCatching { parseCloudInventory(Json.parseToJsonElement("[]"), Json.parseToJsonElement("""{"complete":true}""")) }.isFailure)
        assertTrue(runCatching { parseFileJobs(Json.parseToJsonElement("""[{"upload_id":"one","stage":"done"}]"""), "download") }.isFailure)
        assertTrue(runCatching { parseCloudInventory(Json.parseToJsonElement("""[{"upload_id":"one","path":[],"device":"device","generation":1,"bytes":8,"needs_root":false,"local":true}]"""), Json.parseToJsonElement("""{"complete":true,"observed_at_ms":1}""")) }.isFailure)
    }
    @Test fun admissionSkipsAndTransferBackoffRetainTheirMeaning() {
        assertFalse(parseBackupAdmission(Json.parseToJsonElement("""{"started":[],"skipped":[{"reason":"too_large"}]}""")).accepted)
        assertTrue(parseBackupAdmission(Json.parseToJsonElement("""{"started":[],"skipped":[{"reason":"in_progress"}]}""")).accepted)
        val job = parseFileJobs(Json.parseToJsonElement("""[{"upload_id":"original","name":null,"stage":"verifying","done":2,"total":2,"bytes":9007199254740993,"total_bytes":9007199254740993,"error":"UNAVAILABLE","retry_at":100}]"""), "download").single()
        assertFalse(job.due(99u)); assertTrue(job.due(100u))
        assertFalse(job.copy(stage = "done", error = null).due(200u))
        assertFalse(job.copy(retryAt = null).due(200u))
        assertEquals(9007199254740993uL, job.bytes)
    }
    @Test fun availableSpaceNeverWrapsUnsignedCounters() {
        assertEquals(0uL, BackupCapacity(1u, 10u, ULong.MAX_VALUE, 2u, null).remaining)
        assertEquals(0uL, BackupCapacity(1u, 10u, 5u, ULong.MAX_VALUE, null).remaining)
        assertEquals(3uL, BackupCapacity(1u, 10u, 5u, 2u, null).remaining)
    }
}
