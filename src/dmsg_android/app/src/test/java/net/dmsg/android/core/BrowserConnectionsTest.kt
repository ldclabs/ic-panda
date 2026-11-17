package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class BrowserConnectionsTest {
    private val account = "040g2081040g2081040g"
    private val request = "ab".repeat(32)
    @Test fun onlyExactConnectDestinationsAndUniqueBoundedIdentifiersAreAccepted() {
        val link = ConnectLink("request", account, request)
        assertEquals(link, ConnectLink.parse(link.url()))
        assertEquals(link, ConnectLink.parse("dmsg://connect/request?account=$account&request=$request"))
        assertEquals(ConnectLink("pair", null, null), ConnectLink.parse("https://connect.dmsg.net/app/pair"))
        for (url in listOf("http://connect.dmsg.net/app/pair", "https://connect.dmsg.net:443/app/pair", "https://user@connect.dmsg.net/app/pair", "https://connect.dmsg.net.evil.test/app/pair", "https://connect.dmsg.net/app/pair#secret", "https://connect.dmsg.net/app/pair?request=$request", "https://connect.dmsg.net/app/request?request=$request", link.url() + "&account=$account", link.url() + "&approve=true", link.url().replace("/app/request", "/app/%72equest"), link.url().replace(request, request.uppercase()))) assertNull(url, ConnectLink.parse(url))
    }
    @Test fun invitationSecretsOnlyOpenInTheConfiguredBridge() {
        assertTrue(trustedBrowserInvitation("https://connect.dmsg.net/v1/pair#AQ_ab-CD", "https://connect.dmsg.net"))
        for (url in listOf("https://connect.dmsg.net.evil.test/v1/pair#abc", "https://user@connect.dmsg.net/v1/pair#abc", "https://connect.dmsg.net/v1/pair?next=evil#abc", "https://connect.dmsg.net/v1/pair#", "https://connect.dmsg.net/else#abc", "javascript:alert(1)")) assertFalse(trustedBrowserInvitation(url, "https://connect.dmsg.net"))
    }
    @Test fun browserExpirationAndMissingStateRemainExplicit() {
        val values = parseBrowserPairings(Json.parseToJsonElement("""[{"pairing_id":"original","label":null,"state":"revoked","expires_at":9007199254740993}]"""))
        assertEquals(9007199254740993uL, values.single().expiresAt); assertEquals("revoked", values.single().state)
        assertTrue(runCatching { parseBrowserPairings(Json.parseToJsonElement("""[{"pairing_id":"missing-state"}]""")) }.isFailure)
    }
}
