package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class SecretToolsTest {
    @Test fun unavailableAttachmentsRetainTheirIdentityAndCannotEnterSnapshots() {
        val result = parseSecretAttachments(Json.parseToJsonElement("""{"item_id":"item","revision":"rev","device_local":true,"attachments_sync_separately":true,"files":[{"id":"missing","available":false,"error":"NOT_FOUND"}]}"""))
        assertEquals("missing", result.files.single().id)
        assertFalse(result.files.single().selectable)
        assertEquals("NOT_FOUND", result.files.single().error)
    }

    @Test fun findingsKeepExactObservationTimeAndRejectIncompleteReports() {
        val source = """{"findings":[{"id":"one","rule":"weak","severity":"high","subject":"item","revision":"r","field":"password","observed_at":18446744073709551615,"evidence":"local","action":"review","dismissed":false}],"rotations":[],"observed_at":9007199254740993,"device_local":true}"""
        val report = parseSecretHealth(Json.parseToJsonElement(source))
        assertEquals(ULong.MAX_VALUE, report.findings.single().observedAt)
        assertEquals(9007199254740993uL, report.observedAt)
        assertTrue(runCatching { parseSecretHealth(Json.parseToJsonElement(source.replace("\"dismissed\":false", "\"other\":false"))) }.isFailure)
        assertTrue(runCatching { parseSecretHealth(Json.parseToJsonElement(source.replace("\"device_local\":true", "\"device_local\":false"))) }.isFailure)
    }

    @Test fun onlyOpaqueFieldIdsAreSavedInNavigation() {
        val navigation = WorkspaceNavigation()
        navigation.activate("account")
        navigation.push(Route("secret-reference", "item", field = "password"))
        val restored = WorkspaceNavigation()
        restored.restore(navigation.save()); restored.activate("account")
        assertEquals(navigation.state.value, restored.state.value)
    }
}
