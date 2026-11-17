package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class ProfileModelsTest {
    @Test fun onlyNamedHttpsLinksAndBoundedUnicodeFieldsArePublished() {
        assertTrue(ProfileDraft("🧪".repeat(80), links = listOf(ProfileLink("Project", "https://dmsg.net/"))).valid)
        assertFalse(ProfileDraft("🧪".repeat(81)).valid)
        assertFalse(ProfileDraft(about = "中".repeat(2001)).valid)
        for (url in listOf("http://dmsg.net", "https://person:password@dmsg.net", "javascript:alert(1)", "/relative")) {
            assertFalse(ProfileDraft(links = listOf(ProfileLink("Project", url))).valid)
        }
        assertFalse(ProfileDraft(links = listOf(ProfileLink("", "https://dmsg.net"))).valid)
    }
    @Test fun profileAndPublicationKeepExactVersionsAndUnknownDataFails() {
        val value = parsePublicProfile(Json.parseToJsonElement("""{"name":null,"description":null,"avatar_url":null,"links":[],"updated_at":9007199254740993}"""))
        assertEquals(9007199254740993L, value.updatedAt)
        val pending = parseProfilePublication(Json.parseToJsonElement("""{"published":false,"pending":true,"updated_at":9007199254740993}"""))
        assertTrue(pending.pending); assertFalse(pending.published)
        assertTrue(runCatching { parseProfilePublication(Json.parseToJsonElement("""{"pending":false}""")) }.isFailure)
        assertTrue(runCatching { parsePublicProfile(Json.parseToJsonElement("""{"links":[],"updated_at":18446744073709551615}""")) }.isFailure)
    }
    @Test fun localRiskCountsAreNotZeroWhenMissingAndCacheTotalsCannotWrap() {
        val value = parseLocalContent(Json.parseToJsonElement("""{"versions":2,"bytes":9007199254740993,"unbacked_versions":1,"unbacked_bytes":8000,"pending_sync":12,"last_sync_ok_at":null}"""))
        assertEquals(9007199254740993uL, value.bytes); assertEquals(1uL, value.unbacked); assertNull(value.lastSync)
        assertTrue(runCatching { parseLocalContent(Json.parseToJsonElement("""{"versions":0}""")) }.isFailure)
        assertTrue(runCatching { parseDisplayUsage(Json.parseToJsonElement("""{"domains":[{"sealed_bytes":18446744073709551615},{"sealed_bytes":1}]}""")) }.isFailure)
    }
}
