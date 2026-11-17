package net.dmsg.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class MembershipModelsTest {
    @Test fun expiredOrUnverifiableLeasesNeverBecomeFreeByDefault() {
        val value = parseMembershipPlan(Json.parseToJsonElement("""{"plan":"Max","source":"Active","eligibility":"Eligible","lease_until_ms":1000,"limits":{"storage_bytes":9007199254740993,"active_channels":200,"monthly_execution_units":200,"active_devices":10},"current":{"contract":"abababababababababababababababababababababababababababababababab","ends_at_ms":5000,"method":"AppStore","store_environment":"Sandbox"}}"""))
        assertEquals(9007199254740993uL, value.limits.storage)
        assertFalse(value.needsRefresh(999u)); assertTrue(value.needsRefresh(1000u))
        assertTrue(value.copy(eligibility = "Unverifiable").needsRefresh(1u))
        assertTrue(value.copy(source = "Unknown").needsRefresh(1u))
        assertTrue(runCatching { parseMembershipPlan(Json.parseToJsonElement("""{"plan":"Free","source":"Free"}""")) }.isFailure)
        assertNull(parseMembershipCache(Json.parseToJsonElement("""{"state":"missing","data":null,"observed_at_ms":null,"stale":true}""")))
    }
    @Test fun usageCanFailIndependentlyAndCountsCannotWrapOrRound() {
        val value = parseMembershipUsage(Json.parseToJsonElement("""{"cloud":{"committed":18446744073709551614,"reserved":5,"storage_limit":18446744073709551615,"active_channels":null,"channel_limit":null},"execution":null,"cloud_error":null,"execution_error":"AUTH_REQUIRED"}"""))
        assertEquals(0uL, value.cloud!!.remaining); assertNull(value.execution); assertEquals("AUTH_REQUIRED", value.executionError)
        assertNull(value.cloud.copy(limit = null).remaining)
        assertEquals(10uL, value.cloud.copy(committed = 30u, reserved = 10u, limit = 50u).remaining)
        assertTrue(runCatching { parseMembershipUsage(Json.parseToJsonElement("""{"cloud":null,"execution":null}""")) }.isFailure)
        assertTrue(runCatching { parseMembershipUsage(Json.parseToJsonElement("""{"cloud":null,"cloud_error":"UNAVAILABLE","execution":{"month":202613,"allowed":20,"charged":1,"remaining":19,"valid_until_ms":1000},"execution_error":null}""")) }.isFailure)
    }
    @Test fun catalogDoesNotInventMissingWalletQuotasOrNativePrices() {
        val plans = membershipTiers.map { tier -> Json.parseToJsonElement("""{"plan_id":"$tier","price_cents":987654321,"limits":{"storage_bytes":100,"active_channels":2,"monthly_execution_units":5,"active_devices":2,"backup_max_file_bytes":100,"file_request_links":0,"team_wallets":null}}""") }
        val value = parseMembershipCatalog(buildJsonObject { put("plans", JsonArray(plans.reversed())); put("wallet_quotas_error", "UNAVAILABLE") })
        assertEquals(membershipTiers, value.plans.map { it.tier }); assertTrue(value.plans.all { it.wallets == null })
        assertTrue(runCatching { parseMembershipCatalog(buildJsonObject { put("plans", JsonArray(plans.dropLast(1))) }) }.isFailure)
        assertTrue(runCatching { parseMembershipCatalog(buildJsonObject { put("plans", JsonArray(plans.dropLast(1) + plans.first())) }) }.isFailure)
    }
}
