package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class AccountModelsTest {
    @Test fun onlyActiveAdministratorsWithRootManageCanChangeSecurity() {
        val device = AccountDevice("d", "Administrator", listOf("RootManage"), 1u, null, null, true)
        assertTrue(device.managesAccount)
        assertFalse(device.copy(revokedAt = 2u).managesAccount)
        assertFalse(device.copy(capabilities = listOf("VaultUnlock")).managesAccount)
        assertFalse(device.copy(role = "Member").managesAccount)
    }

    @Test fun recoveryMustBeEnabledAndRespectBothBoundsAndTheRequestingDevice() {
        val value = RecoveryStatus("account", true, false, true, 86_400_000u, RecoveryPending("id", 10u, 20u, true))
        assertFalse(value.canComplete(9u)); assertTrue(value.canComplete(10u)); assertFalse(value.canComplete(20u))
        assertFalse(value.copy(enabled = false).canComplete(15u))
        assertFalse(value.copy(pending = value.pending!!.copy(current = false)).canComplete(15u))
        assertFalse(value.canRequest(19u)); assertTrue(value.canRequest(20u))
        assertFalse(value.copy(member = true).canRequest(30u))
        assertTrue(value.copy(member = true, pending = null).canComplete(30u))
    }

    @Test fun incompleteSecurityPolicyIsNotTreatedAsDisabledRecovery() {
        assertThrows(IllegalStateException::class.java) { parseSecurity(Json.parseToJsonElement("""{"logins":[],"recovery_delay_ms":259200000,"recovery_updating":false}""")) }
        val parsed = parseSecurity(Json.parseToJsonElement("""{"logins":[],"recovery_delay_ms":18446744073709551615,"recovery_enabled":true,"recovery_updating":true}"""))
        assertEquals(ULong.MAX_VALUE, parsed.recoveryDelayMs)
        assertTrue(parsed.recoveryEnabled); assertTrue(parsed.recoveryUpdating)
    }
}
