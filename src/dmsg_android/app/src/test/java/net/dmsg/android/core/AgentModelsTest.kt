package net.dmsg.android.core

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import org.junit.Assert.*
import org.junit.Test

class AgentModelsTest {
    @Test fun hostedQuotaUsesTheAccountTotalEvenWithoutLocalAgents() {
        assertTrue(parseManagedAgents(Json.parseToJsonElement("""{"agents":[]}""")).isEmpty())
        assertEquals(HostedAgentQuota(2u, 3u), parseHostedAgentQuota(Json.parseToJsonElement("""{"used":2,"limit":3}""")))
        assertNull(parseHostedAgentQuota(Json.parseToJsonElement("""{"used":2,"limit":null}""")).limit)
    }

    @Test fun delegationAuthorityHasExplicitBoundsAndNoImplicitWildcard() {
        assertTrue(AgentAuthority("read\nwrite", "https://example.test\ndid:agent:sample").valid)
        for (scope in listOf("", "*", "read:*", "with space", "a".repeat(65), "read\nread", (1..9).joinToString("\n") { "scope$it" })) assertFalse(scope, AgentAuthority(scope, "https://example.test").valid)
        for (audience in listOf("", "http://example.test", "https://user@example.test", "https://example.test/", "https://example.test/a", "https://example.test?x", "https://example.test#x", "https://example.test\nhttps://example.test", (1..5).joinToString("\n") { "https://host$it.test" })) assertFalse(audience, AgentAuthority("read", audience).valid)
        assertFalse(AgentDelegationDraft(name = "controller").validController)
        assertTrue(AgentDelegationDraft(name = "controller", unrestricted = true).validController)
        assertFalse(AgentDelegationDraft(name = "\ncontroller", unrestricted = true).validController)
        assertTrue(AgentPermissionsDraft("vault.list", "366").valid)
        assertFalse(AgentPermissionsDraft("vault.list", "367").valid)
        assertFalse(AgentPermissionsDraft("vault.list", "0").valid)
        val permissions = (1..16).joinToString("\n") { "files.read:folder-$it" }
        assertTrue(AgentPermissionsDraft(permissions).valid)
        assertFalse(AgentPermissionsDraft(permissions + "\nvault.list").valid)
        assertFalse(validAgentLabel("Agent"))
    }

    @Test fun disabledPrincipalDiffersFromUnavailableAndUnpublishedState() {
        assertNull(parseAgentPrincipal(JsonNull))
        assertTrue(runCatching { parseAgentPrincipal(Json.parseToJsonElement("{}")) }.isFailure)
        val raw = """{"principal_id":"did:agent:principal","controllers":[],"version":9007199254740993,"published_version":9007199254740992,"published":false}"""
        val value = parseAgentPrincipal(Json.parseToJsonElement(raw))!!
        assertEquals(9007199254740993uL, value.version)
        assertFalse(value.published)
        assertTrue(runCatching { parseAgentPrincipal(Json.parseToJsonElement(raw.replace("\"published\":false", "\"published\":true"))) }.isFailure)
        assertTrue(runCatching { parseAgentPrincipal(Json.parseToJsonElement(raw.replace("\"controllers\":[]", "\"controllers\":[{}]"))) }.isFailure)
    }

    @Test fun incompleteCredentialsAreNeverInventedAsActive() {
        val raw = """[{"id":"credential","subject":"did:agent:subject","scopes":["read"],"audiences":["https://example.test"],"status":"suspended","not_before":12,"expires_at":9007199254740993}]"""
        val value = parseAgentCredentials(Json.parseToJsonElement(raw)).single()
        assertEquals("suspended", value.status)
        assertEquals(12uL, value.notBefore)
        assertEquals(9007199254740993uL, value.expiresAt)
        assertTrue(runCatching { parseAgentCredentials(Json.parseToJsonElement(raw.replace("\"status\":\"suspended\",", ""))) }.isFailure)
        assertTrue(runCatching { parseAgentCredentials(Json.parseToJsonElement("{}")) }.isFailure)
    }
}
