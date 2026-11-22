package net.dmsg.android.core

import kotlinx.serialization.json.*

val membershipTiers = listOf("Free", "Plus", "Pro", "Max")
data class PlanLimits(val storage: ULong, val channels: ULong, val executions: ULong, val devices: ULong, val agentIds: ULong? = null)
data class MembershipTerm(val endsAt: ULong, val method: String, val environment: String?, val contract: String? = null)
data class MembershipPlan(val tier: String, val source: String, val eligibility: String?, val lease: ULong?, val limits: PlanLimits, val term: MembershipTerm?) {
    fun needsRefresh(now: ULong) = source !in setOf("Active", "Free", "Expired") || eligibility == "Unverifiable" || lease?.let { it <= now } == true
}
data class PlanBenefit(val tier: String, val limits: PlanLimits, val backupFile: ULong, val requests: ULong, val wallets: ULong?)
data class MembershipCatalog(val plans: List<PlanBenefit>, val walletError: String?)
data class MembershipCloudUsage(val committed: ULong, val reserved: ULong, val limit: ULong?, val channels: ULong?, val channelLimit: ULong?) {
    val remaining: ULong? get() = limit?.let { if (committed >= it || reserved >= it - committed) 0uL else it - committed - reserved }
}
data class MembershipExecutionUsage(val month: UInt, val allowed: ULong, val charged: ULong, val remaining: ULong, val validUntil: ULong)
data class MembershipUsage(val cloud: MembershipCloudUsage?, val execution: MembershipExecutionUsage?, val cloudError: String?, val executionError: String?)
data class CachedMembership(val value: MembershipPlan, val stale: Boolean)
private fun JsonObject.planLimits() = PlanLimits(requiredUnsigned("storage_bytes"), requiredUnsigned("active_channels"), requiredUnsigned("monthly_execution_units"), requiredUnsigned("active_devices"), unsigned("agent_ids"))
fun parseMembershipPlan(value: JsonElement): MembershipPlan = value.objectValue().let { row ->
    val tier = row.required("plan"); check(tier in membershipTiers) { "INTEGRITY_FAILED" }
    MembershipPlan(tier, row.required("source"), row.text("eligibility"), row.unsigned("lease_until_ms"), row["limits"]!!.objectValue().planLimits(), row["current"]?.takeIf { it != JsonNull }?.objectValue()?.let { MembershipTerm(it.requiredUnsigned("ends_at_ms"), it.required("method"), it.text("store_environment"), it.required("contract").also { contract -> check(contract.matches(Regex("[a-f0-9]{64}"))) { "INTEGRITY_FAILED" } }) })
}
fun parseMembershipCache(value: JsonElement): CachedMembership? {
    val row = value.objectValue()
    if (row.required("state") == "missing") { check(row["data"] == JsonNull && row["observed_at_ms"] == JsonNull); return null }
    check(row.required("state") == "present") { "INTEGRITY_FAILED" }; row.requiredUnsigned("observed_at_ms")
    return CachedMembership(parseMembershipPlan(row["data"]!!), row.requiredFlag("stale"))
}
fun parseMembershipCatalog(value: JsonElement): MembershipCatalog {
    val row = value.objectValue()
    val plans = row.rows("plans").map { plan ->
        val limits = plan["limits"]!!.objectValue()
        PlanBenefit(plan.required("plan_id"), limits.planLimits(), limits.requiredUnsigned("backup_max_file_bytes"), limits.requiredUnsigned("file_request_links"), limits.unsigned("team_wallets"))
    }
    check(plans.size == membershipTiers.size && plans.map { it.tier }.toSet() == membershipTiers.toSet()) { "INTEGRITY_FAILED" }
    // Cash price_cents is deliberately not decoded as a native store price.
    return MembershipCatalog(plans.sortedBy { membershipTiers.indexOf(it.tier) }, row.text("wallet_quotas_error"))
}
fun parseMembershipUsage(value: JsonElement): MembershipUsage {
    val row = value.objectValue()
    val cloud = row["cloud"]?.takeIf { it != JsonNull }?.objectValue()?.let { MembershipCloudUsage(it.requiredUnsigned("committed"), it.requiredUnsigned("reserved"), it.unsigned("storage_limit"), it.unsigned("active_channels"), it.unsigned("channel_limit")) }
    val execution = row["execution"]?.takeIf { it != JsonNull }?.objectValue()?.let {
        val month = it.requiredUnsigned("month"); check(month <= UInt.MAX_VALUE.toULong() && month % 100u in 1uL..12uL) { "INTEGRITY_FAILED" }
        val allowed = it.requiredUnsigned("allowed"); val charged = it.requiredUnsigned("charged"); val remaining = it.requiredUnsigned("remaining")
        check(remaining == if (allowed > charged) allowed - charged else 0uL) { "INTEGRITY_FAILED" }
        MembershipExecutionUsage(month.toUInt(), allowed, charged, remaining, it.requiredUnsigned("valid_until_ms"))
    }
    val cloudError = row.text("cloud_error"); val executionError = row.text("execution_error")
    check((cloud != null) != (cloudError != null) && (execution != null) != (executionError != null)) { "INTEGRITY_FAILED" }
    return MembershipUsage(cloud, execution, cloudError, executionError)
}

/** The entitlement, catalogue and usage have independent failures and request ownership. */
class MembershipCoordinator(private val client: ClientController) {
    val plan = ReadStore<MembershipPlan>()
    val catalog = ReadStore<MembershipCatalog>()
    val usage = ReadStore<MembershipUsage>()
    fun loadPlan(force: Boolean = false) {
        val request = plan.begin()
        fun accept(value: CachedMembership) { plan.accept(request, value.value); if (value.stale) plan.fail(request, "MEMBERSHIP_STALE") }
        fun refresh() {
            client.perform({ core -> parseMembershipCache(client.operation { core.refreshDisplay("plan", "", force, it) }.decode()) ?: error("INTEGRITY_FAILED") }, key = "membership:plan", replace = true, quiet = true, onError = { plan.fail(request, it) }) { accept(it) }
        }
        client.perform({ parseMembershipCache(it.cachedDisplay("plan", "").decode()) }, key = "membership:cached", replace = true, quiet = true, onError = { refresh() }) { cached -> cached?.let(::accept); refresh() }
    }
    fun loadCatalog() {
        val request = catalog.begin()
        client.perform({ parseMembershipCatalog(it.planCatalog().decode()) }, key = "membership:catalog", replace = true, quiet = true, onError = { catalog.fail(request, it) }) { catalog.accept(request, it) }
    }
    fun loadUsage() {
        val request = usage.begin()
        // execution_usage restores an existing login before reading the private monthly ledger.
        client.perform({ parseMembershipUsage(it.accountUsage().decode()) }, key = "membership:usage", replace = true, quiet = true, onError = { usage.fail(request, it) }) { usage.accept(request, it) }
    }
    fun load(force: Boolean = false) { loadPlan(force); loadCatalog(); loadUsage() }
    fun clear() { plan.reset(); catalog.reset(); usage.reset() }
}
