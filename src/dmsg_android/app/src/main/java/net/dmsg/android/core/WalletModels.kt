package net.dmsg.android.core

import kotlinx.serialization.json.*
import java.math.BigInteger
import java.math.BigDecimal
import java.text.DecimalFormatSymbols
import java.text.NumberFormat
import java.util.Locale

object AtomicAmount {
    private val maximum = BigInteger("340282366920938463463374607431768211455")
    fun parse(text: String, decimals: Int, separator: Char = DecimalFormatSymbols.getInstance(Locale.getDefault()).decimalSeparator): String? {
        if (decimals !in 0..38) return null
        val trimmed = text.trim().replace(separator, '.')
        if (trimmed.isEmpty() || trimmed.length > 80 || trimmed.endsWith('.')) return null
        val normalized = buildString {
            for (c in trimmed) {
                if (c == '.') append(c)
                else { val digit = Character.digit(c, 10); if (digit !in 0..9) return null; append(digit) }
            }
        }
        val parts = normalized.split('.')
        if (parts.size > 2 || parts.first().isEmpty()) return null
        val fraction = parts.getOrNull(1).orEmpty()
        if (fraction.length > decimals) return null
        val result = (parts.first() + fraction.padEnd(decimals, '0')).trimStart('0').ifEmpty { "0" }
        return result.takeIf { BigInteger(it) <= maximum }
    }
    fun canonical(value: String): String {
        check(value.isNotEmpty() && value.length <= 39 && value.all { it in '0'..'9' } && (value == "0" || !value.startsWith('0')) && BigInteger(value) <= maximum) { "INTEGRITY_FAILED" }
        return value
    }
    fun display(value: String, decimals: Int, locale: Locale = Locale.getDefault()): String {
        canonical(value); check(decimals in 0..38) { "INTEGRITY_FAILED" }
        return NumberFormat.getNumberInstance(locale).apply {
            isGroupingUsed = false
            minimumFractionDigits = 0
            maximumFractionDigits = decimals
        }.format(BigDecimal(BigInteger(value), decimals))
    }
}
data class WalletSummary(val id: String, val name: String)
data class WalletMember(val id: String, val label: String, val role: String, val status: String)
data class WalletAddress(val id: UInt, val text: String, val label: String, val chain: String, val verified: Boolean, val control: String, val archived: Boolean, val cosigner: UInt?)
data class WalletCosigner(val id: UInt, val label: String, val member: String, val curve: String, val publicKey: String, val hardware: Boolean, val local: Boolean?)
data class WalletPolicy(val approvers: UInt, val administrators: UInt, val details: JsonObject)
data class WalletDetails(val id: String, val name: String, val frozen: Boolean, val me: WalletMember?, val members: List<WalletMember>, val addresses: List<WalletAddress>, val cosigners: List<WalletCosigner>, val policy: WalletPolicy) {
    val canPropose get() = me?.status == "Active" && me.role !in setOf("Viewer", "Recovery")
    val canProposeMembers get() = me?.status == "Active" && me.role != "Viewer"
    val canDecide get() = me?.status == "Active" && me.role in setOf("Admin", "Approver")
}
data class WalletAsset(val key: String, val symbol: String?, val decimals: Int?, val error: String?) {
    val chain get() = when { key == "icp" || key.startsWith("icrc:") -> "Icp"; key == "btc" -> "Bitcoin"; key.startsWith("evm:") -> "Evm"; else -> "Solana" }
}
data class WalletBalance(val address: UInt, val text: String, val asset: WalletAsset, val atoms: String?, val error: String?)
data class WalletBalances(val wallet: String, val readAt: ULong, val values: List<WalletBalance>)
data class WalletProposal(val id: ULong, val status: String, val intent: JsonObject, val digest: String, val proposer: String, val approvals: UInt, val admin: Boolean, val delay: ULong, val created: ULong, val expires: ULong, val ready: ULong?, val plans: UInt, val failure: String?, val record: JsonObject)
data class WalletProposalPage(val proposals: List<WalletProposal>, val next: ULong?)
data class WalletPending(val id: String, val wallet: String, val intent: JsonObject)
data class WalletTransferDraft(val wallet: String, val from: UInt = 0u, val asset: String = "", val recipient: String = "", val amount: String = "", val maxFee: String = "")
data class WalletTransferPreview(val estimatedFee: String, val maxFee: String, val feeAsset: String, val feeDecimals: Int, val assetBalance: String, val nativeBalance: String, val reserve: String, val approvals: UInt, val admin: Boolean, val delay: ULong, val readAt: ULong, val blockers: List<String>) {
    fun canSubmit(now: ULong) = blockers.isEmpty() && readAt <= now && now - readAt <= 60_000uL
}
data class WalletSubmission(val request: String = requestId(), val attempted: Boolean = false, val result: JsonObject? = null, val error: String? = null) {
    fun failed(journal: String?, reason: String) = if (journal in setOf("absent", "rejected")) WalletSubmission(error = reason) else copy(attempted = true, error = reason)
}
private fun JsonObject.walletMember() = WalletMember(required("id"), text("label").orEmpty(), required("role"), required("status"))
fun parseWallets(value: JsonElement) = value.arrayRows().map { WalletSummary(it.required("wallet"), it.required("name")) }
fun parseWalletDetails(value: JsonElement): WalletDetails {
    val row = value.objectValue(); val policy = row["policy"]!!.objectValue()
    return WalletDetails(row.required("wallet"), row.required("name"), row.requiredFlag("frozen"), row["me"]?.takeIf { it != JsonNull }?.objectValue()?.walletMember(), row.rows("members").map { it.walletMember() },
        row.rows("addresses").map { address -> WalletAddress(address.requiredUInt("id"), address.required("text"), address.required("label"), address.required("chain"), address.requiredFlag("verified"), address.required("control"), address.flag("archived"), address["control_details"]?.takeIf { it != JsonNull }?.objectValue()?.requiredUInt("cosigner")) },
        row.rows("cosigners").map { WalletCosigner(it.requiredUInt("id"), it.required("label"), it.required("member"), it.required("curve"), it.required("public_key"), it.requiredFlag("hardware"), it.optionalFlag("local")) },
        WalletPolicy(policy.requiredUInt("approver_threshold"), policy.requiredUInt("admin_threshold"), policy))
}
fun parseWalletAsset(row: JsonObject): WalletAsset {
    val decimals = row.unsigned("decimals"); check(decimals == null || decimals <= 38uL) { "INTEGRITY_FAILED" }
    return WalletAsset(row.required("key"), row.text("symbol"), decimals?.toInt(), row.text("error"))
}
fun parseWalletAssets(value: JsonElement) = value.objectValue().rows("assets").map(::parseWalletAsset)
fun parseWalletBalances(value: JsonElement): WalletBalances {
    val row = value.objectValue()
    val values = row.rows("addresses").flatMap { address -> address.rows("balances").map { balance ->
        val result = balance["balance"]!!.objectValue(); check(result.keys == setOf("Ok") || result.keys == setOf("Err")) { "INTEGRITY_FAILED" }
        val metadata = balance["metadata"]?.takeIf { it != JsonNull }?.objectValue()
        WalletBalance(address.requiredUInt("address"), address.required("text"), metadata?.let(::parseWalletAsset) ?: WalletAsset(balance.required("key"), balance.text("asset"), null, "UNAVAILABLE"), result.text("Ok")?.let(AtomicAmount::canonical), result.text("Err"))
    } }
    return WalletBalances(row.required("wallet"), row.requiredUnsigned("read_at"), values)
}
fun parseWalletProposal(value: JsonElement): WalletProposal {
    val row = value.objectValue(); val requirement = row["requirement"]!!.objectValue()
    return WalletProposal(row.requiredUnsigned("id"), row.required("status"), row["intent"]!!.objectValue(), row.required("digest"), row.required("proposer"), requirement.requiredUInt("approvals"), requirement.requiredFlag("admin"), requirement.requiredUnsigned("delay_ms"), row.requiredUnsigned("created_at"), row.requiredUnsigned("expires_at"), row.unsigned("ready_at"), row.requiredUInt("plans"), row.text("failure"), row)
}
fun parseWalletProposalPage(value: JsonElement) = value.objectValue().let { WalletProposalPage(it.rows("proposals").map(::parseWalletProposal), it.unsigned("next")) }
fun parseWalletPending(value: JsonElement) = value.arrayRows().map { WalletPending(it.required("id"), it.required("wallet"), it["intent"]!!.objectValue()) }
fun parseWalletTransferPreview(value: JsonElement): WalletTransferPreview {
    val row = value.objectValue(); val requirement = row["requirement"]!!.objectValue(); val decimals = row.requiredUInt("fee_decimals")
    check(decimals <= 38u) { "INTEGRITY_FAILED" }
    return WalletTransferPreview(AtomicAmount.canonical(row.required("estimated_fee")), AtomicAmount.canonical(row.required("max_fee")), row.required("fee_asset"), decimals.toInt(), AtomicAmount.canonical(row.required("asset_balance")), AtomicAmount.canonical(row.required("native_balance")), AtomicAmount.canonical(row.required("additional_reserve")), requirement.requiredUInt("approvals"), requirement.requiredFlag("admin"), requirement.requiredUnsigned("delay_ms"), row.requiredUnsigned("read_at"), row.strings("blockers"))
}

data class WalletControlDraft(val chain: String = "Icp", val addressLabel: String = "", val member: String = "", val memberLabel: String = "", val role: String = "Viewer", val cosigners: Set<UInt> = emptySet(), val threshold: String = "1", val approvers: String = "1", val administrators: String = "1")
data class LocalWalletKey(val label: String, val publicKey: String, val hardware: Boolean, val provider: String?)
data class NativeWalletProtector(val index: UInt, val biometry: Boolean, val createdAt: ULong)
data class NativeWalletKeys(val created: Boolean, val protection: String?, val hardware: String?, val key: String?, val protectors: List<NativeWalletProtector>)
data class NativeWalletMove(val from: UInt, val source: String, val destination: String?, val chainKeyAfter: ULong, val next: String, val cosigner: UInt?, val archived: Boolean, val cooldown: ULong?, val pending: ULong?, val pendingState: String?, val sweep: WalletSweep?)
data class WalletSweep(val tip: ULong, val feeRate: ULong, val inputs: ULong, val total: String, val fee: String?, val amount: String?, val dust: Boolean, val empty: Boolean, val utxos: List<WalletUtxo>)
data class WalletUtxo(val txid: String, val vout: UInt, val value: String, val spendableAt: ULong?)
fun parseLocalWalletKeys(value: JsonElement) = value.arrayRows().map { LocalWalletKey(it.required("label"), it.required("public_key"), it.requiredFlag("hardware"), it.text("backend")) }
fun parseNativeWalletKeys(value: JsonElement) = value.objectValue().let { row ->
    NativeWalletKeys(row.requiredFlag("created"), row.text("protection"), row.text("hardware"), row["key"]?.takeIf { it != JsonNull }?.objectValue()?.required("public_key"), row.rows("protectors").map { NativeWalletProtector(it.requiredUInt("index"), it.requiredFlag("biometry_only"), it.requiredUnsigned("created_at")) })
}
fun parseNativeWalletMove(value: JsonElement): NativeWalletMove {
    val row = value.objectValue(); val from = row["from"]!!.objectValue()
    val pending = row["pending"]?.takeIf { it != JsonNull }?.objectValue()
    val sweep = row["sweep"]?.takeIf { it != JsonNull }?.objectValue()?.let {
        WalletSweep(it.requiredUnsigned("tip"), it.requiredUnsigned("fee_rate"), it.requiredUnsigned("inputs"), AtomicAmount.canonical(it.required("total")), it.text("fee")?.let(AtomicAmount::canonical), it.text("amount")?.let(AtomicAmount::canonical), it.requiredFlag("dust"), it.requiredFlag("empty"), it.rows("utxos").map { utxo -> WalletUtxo(utxo.required("txid"), utxo.requiredUInt("vout"), AtomicAmount.canonical(utxo.required("value")), utxo.unsigned("spendable_at")) })
    }
    val cosigner = row.unsigned("cosigner"); check(cosigner == null || cosigner <= UInt.MAX_VALUE.toULong()) { "INTEGRITY_FAILED" }
    return NativeWalletMove(from.requiredUInt("id"), from.required("text"), row["to"]?.takeIf { it != JsonNull }?.objectValue()?.required("text"), from.requiredUnsigned("chain_key_after"), row.required("next"), cosigner?.toUInt(), row.requiredFlag("archived"), row.unsigned("cooldown_until"), pending?.requiredUnsigned("proposal"), pending?.required("status"), sweep)
}
