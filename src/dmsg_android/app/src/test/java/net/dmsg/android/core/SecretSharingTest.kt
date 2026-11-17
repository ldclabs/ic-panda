package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class SecretSharingTest {
    private val importDetail = """{"selection":{"package_id":"package","digest":"digest","fields":["password"],"include_notes":false,"attachments":[1],"folder":"folder"},"preview":{"title":"Received login","notes":true,"fields":[{"id":"username","type":"text","label":"User","section":null},{"id":"password","type":"concealed","label":"Password","section":null}],"attachments":[{"name":"excluded.txt","mime":"text/plain","size":1,"sha256":"first"},{"name":"chosen.txt","mime":"text/plain","size":9007199254740993,"sha256":"second"}]},"files_local_only":true,"files_inherit_folder_grants":true}"""

    @Test fun reviewIncludesOnlyTheFixedSelectionAndKeepsUnsignedSizesExact() {
        val review = parseSecretReview("secret_package_import", Json.parseToJsonElement(importDetail))
        assertEquals(listOf("password"), review.fields.map { it.id })
        assertEquals(listOf("chosen.txt"), review.files.map { it.name })
        assertEquals(9007199254740993uL, review.files.single().size)
        assertFalse(review.notes)
        assertEquals("folder", review.destination)
    }

    @Test fun anInvalidOrDuplicateSelectionCannotEnableApproval() {
        listOf(importDetail.replace("\"attachments\":[1]", "\"attachments\":[4]"),
            importDetail.replace("\"fields\":[\"password\"]", "\"fields\":[\"password\",\"password\"]"),
            importDetail.replace("\"fields\":[\"password\"]", "\"fields\":[\"missing\"]"),
            importDetail.replace("\"files_local_only\":true", "\"files_local_only\":false")).forEach {
            assertTrue(runCatching { parseSecretReview("secret_package_import", Json.parseToJsonElement(it)) }.isFailure)
        }
    }

    @Test fun channelPackageRoutePreservesTheFullSequenceWithoutPayloads() {
        val navigation = WorkspaceNavigation()
        navigation.activate("account"); navigation.open("messages")
        navigation.push(Route("secret-import", channel = "channel", sequence = ULong.MAX_VALUE))
        val restored = WorkspaceNavigation()
        restored.restore(navigation.save()); restored.activate("account")
        assertEquals(navigation.state.value, restored.state.value)
    }
}
