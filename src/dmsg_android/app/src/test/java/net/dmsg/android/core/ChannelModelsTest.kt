package net.dmsg.android.core

import java.util.Base64
import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class ChannelModelsTest {
    private fun details() = parseChannelDetails(Json.parseToJsonElement("""{"id":"channel","name":"Channel","left":false,"muted":false,"hidden":false,"muted_until":null,"notification_pending":false,"notification_conflict":false,"readable_epochs":[9007199254740993],"ledger":{"type":"distribution","status":"active","epoch":9007199254740993,"head":"head","members":{"owner":{"account":"owner","role":"owner","can_rotate":true,"joined_epoch":1,"history":[]},"admin":{"account":"admin","role":"admin","can_rotate":true,"joined_epoch":1,"history":[]},"member":{"account":"member","role":"member","can_rotate":false,"joined_epoch":3,"history":[[1,2]]}}}}"""))
    @Test fun rolesNeverGrantAnAdministratorOwnerPrivilegesAndHistoricalMembershipIsReadOnly() {
        val d = details()
        assertTrue(d.canRemove("owner", d.member("admin")!!))
        assertFalse(d.canRemove("admin", d.member("owner")!!))
        assertFalse(d.canRemove("admin", d.member("admin")!!))
        assertTrue(d.canRemove("admin", d.member("member")!!))
        assertFalse(d.canManage("member"))
        assertFalse(d.copy(status = "archived").canManage("owner"))
        assertFalse(d.copy(left = true).canManage("owner"))
        assertEquals(9007199254740993uL, d.epoch)
        assertEquals(listOf(1uL to 2uL), d.member("member")!!.history)
    }
    @Test fun externalInvitationsUseExactRoutesAndUnsignedDeadlines() {
        val id = "ab".repeat(32)
        val raw = """{"format":"dmsg-channel-invitation/1","channel":"$id","genesis":"$id","head":"$id","invitation":"$id","account":"040g2081040g2081040g","name":"Reviewed channel","expires_at":18446744073709551615}"""
        val encoded = Base64.getUrlEncoder().withoutPadding().encodeToString(raw.toByteArray())
        val link = "dmsg://channel/join#$encoded"
        assertEquals(ULong.MAX_VALUE, ChannelInvitation.parse(link)?.expiresAt)
        assertNull(ChannelInvitation.parse("dmsg://channel/join?approve=true#$encoded"))
        assertNull(ChannelInvitation.parse("dmsg://channel.evil/join#$encoded"))
        assertNull(ChannelInvitation.parse("$link="))
        assertNull(ChannelInvitation.parse(raw.replace(id, "invalid")))
    }
    @Test fun missingAuthorityFieldsAreNotTreatedAsEmptyMembers() {
        assertTrue(runCatching { parseChannelDetails(Json.parseToJsonElement("""{"id":"channel","name":"Channel"}""")) }.isFailure)
        assertTrue(runCatching { parseParticipants(Json.parseToJsonElement("""{"040g2081040g2081040g":{"name":"Mina"}}""")) }.isFailure)
        val names = parseParticipants(Json.parseToJsonElement("""{"040g2081040g2081040g":{"name":"Mina","updated_at":9007199254740993,"observed_at_ms":42},"040g2082040g2082040g":{"name":"Mina","updated_at":8,"observed_at_ms":42}}"""))
        assertEquals(2, names.size)
        assertEquals(9007199254740993uL, names.getValue("040g2081040g2081040g").updatedAt)
    }
}
