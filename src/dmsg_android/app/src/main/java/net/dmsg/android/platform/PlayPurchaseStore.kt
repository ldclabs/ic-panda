package net.dmsg.android.platform

import kotlinx.serialization.json.*
import net.dmsg.android.core.*
import java.io.File

interface PurchaseSealer {
    fun seal(bytes: ByteArray): ByteArray
    fun open(bytes: ByteArray): ByteArray
}
class PendingPurchase(val purchase: PlayPurchase, val owner: PlayAccount?, val submittedAt: Long = 0, val rejectedFor: Set<String> = emptySet()) {
    override fun toString() = "PendingPurchase(redacted)"
}
private data class PaymentLedger(val accounts: List<PlayAccount> = emptyList(), val pending: List<PendingPurchase> = emptyList(), val delivered: Map<String, String> = emptyMap())

/** Separate from account roots. Billing callbacks persist here before updating any UI. */
class PlayPurchaseStore(private val root: File, private val cipher: PurchaseSealer) {
    private val file get() = File(root, "purchases.sealed")
    init { check(root.mkdirs() || root.isDirectory) }
    private fun read(): PaymentLedger {
        if (!file.exists()) return PaymentLedger()
        check(file.length() in 28..4L*1024*1024) { "PAYMENT_STORAGE_UNAVAILABLE" }
        val bytes = cipher.open(file.readBytes())
        try {
            val row = Json.parseToJsonElement(bytes.toString(Charsets.UTF_8)).objectValue()
            val accounts = row.rows("accounts").map { PlayAccount(it.required("home"), it.required("account"), it.required("binding")).also(PlayAccount::validate) }
            val pending = row.rows("pending").map { p ->
                val purchase = PlayPurchase(p.required("token"), p.required("product"), PlayKind.parse(p.required("kind")), p.text("binding"), p.text("profile"), p.requiredFlag("pending"), p.requiredUnsigned("time").toLong(), p.text("order")).also(PlayPurchase::validate)
                val binding = p.text("owner")
                val owner = binding?.let { value -> accounts.singleOrNull { it.binding == value } ?: error("PAYMENT_STORAGE_UNAVAILABLE") }
                check(owner == null || purchase.binding == null || owner.binding == purchase.binding) { "PAYMENT_STORAGE_UNAVAILABLE" }
                val rejected = p.strings("rejected").toSet().also { check(it.size <= 128 && it.all(::playHash)) { "PAYMENT_STORAGE_UNAVAILABLE" } }
                PendingPurchase(purchase, owner, p.requiredUnsigned("submitted").toLong().also { check(it >= 0) }, rejected)
            }
            val delivered = row.rows("delivered").associate { it.required("id") to it.required("binding") }
            check(accounts.size <= 128 && accounts.map { it.binding }.distinct().size == accounts.size && pending.size <= 256 && pending.map { it.purchase.id }.distinct().size == pending.size && delivered.size <= 512 && delivered.all { playHash(it.key) && playHash(it.value) }) { "PAYMENT_STORAGE_UNAVAILABLE" }
            return PaymentLedger(accounts, pending, delivered)
        } finally { bytes.fill(0) }
    }
    private fun save(value: PaymentLedger) {
        val bytes = buildJsonObject {
            putJsonArray("accounts") { value.accounts.forEach { add(buildJsonObject { put("home", it.home); put("account", it.account); put("binding", it.binding) }) } }
            putJsonArray("pending") { value.pending.forEach { item -> add(buildJsonObject {
                val p = item.purchase
                put("token", p.token); put("product", p.product); put("kind", p.kind.wire); put("binding", p.binding); put("profile", p.profile); put("pending", p.pending); put("time", p.purchasedAt); put("order", p.order); put("owner", item.owner?.binding); put("submitted", item.submittedAt)
                putJsonArray("rejected") { item.rejectedFor.forEach { add(it) } }
            }) } }
            putJsonArray("delivered") { value.delivered.forEach { (id, binding) -> add(buildJsonObject { put("id", id); put("binding", binding) }) } }
        }.toString().toByteArray()
        try { check(bytes.size <= 4*1024*1024-28) { "PAYMENT_STORAGE_UNAVAILABLE" }; PrivateFiles.atomicWrite(file, cipher.seal(bytes)) } finally { bytes.fill(0) }
    }
    fun bind(account: PlayAccount) = synchronized(lock) {
        account.validate()
        val old = read(); val existing = old.accounts.find { it.binding == account.binding }
        check(existing == null || existing == account) { "ACCOUNT_MISMATCH" }
        check(existing != null || old.accounts.size < 128) { "PAYMENT_STORAGE_UNAVAILABLE" }
        save(old.copy(accounts = if (existing != null) old.accounts else old.accounts + account, pending = old.pending.map { if (it.owner == null && it.purchase.binding == account.binding) PendingPurchase(it.purchase, account, it.submittedAt, it.rejectedFor) else it }))
    }
    fun enqueue(purchase: PlayPurchase, launchedBy: PlayAccount? = null, force: Boolean = false) = synchronized(lock) {
        purchase.validate(); launchedBy?.validate()
        val ledger = read(); val old = ledger.pending.find { it.purchase.id == purchase.id }
        if (!force && ledger.delivered[purchase.id] != null) return@synchronized
        check(old != null || ledger.pending.size < 256) { "PAYMENT_STORAGE_UNAVAILABLE" }
        val owner = old?.owner ?: purchase.binding?.let { binding -> ledger.accounts.find { it.binding == binding } }
            ?: launchedBy?.takeIf { purchase.binding == null || purchase.binding == it.binding }
        check(old == null || (old.purchase.kind == PlayKind.SUBSCRIPTION || old.purchase.product == purchase.product) && old.purchase.kind == purchase.kind && (old.purchase.binding == null || old.purchase.binding == purchase.binding)) { "ACCOUNT_MISMATCH" }
        check(owner == null || ledger.accounts.contains(owner)) { "ACCOUNT_MISMATCH" }
        save(ledger.copy(pending = ledger.pending.filterNot { it.purchase.id == purchase.id } + PendingPurchase(purchase, owner, if (force) 0 else old?.submittedAt ?: 0, old?.rejectedFor ?: emptySet()), delivered = if (force) ledger.delivered - purchase.id else ledger.delivered))
    }
    fun pending(account: PlayAccount, includeUnassigned: Boolean = false): List<PendingPurchase> = synchronized(lock) {
        account.validate(); read().pending.filter { account.binding !in it.rejectedFor && (it.owner == account || it.owner == null && it.purchase.binding == account.binding || includeUnassigned && it.owner == null && it.purchase.binding == null) }
    }
    fun submitted(id: String, account: PlayAccount, at: Long) = synchronized(lock) {
        val ledger = read()
        save(ledger.copy(pending = ledger.pending.map { item ->
            if (item.purchase.id != id) item else {
                check(item.owner == null || item.owner == account) { "ACCOUNT_MISMATCH" }
                check(at >= 0) { "INVALID_INPUT" }
                PendingPurchase(item.purchase, item.owner, maxOf(item.submittedAt, at), item.rejectedFor)
            }
        }))
    }
    fun reconcile(account: PlayAccount, receipts: List<PlayReceipt>) = synchronized(lock) {
        val ledger = read(); val delivered = ledger.delivered.toMutableMap()
        val pending = ledger.pending.mapNotNull { item ->
            val receipt = receipts.find { it.tokenHash == item.purchase.id } ?: return@mapNotNull item
            if (item.owner != null && item.owner != account) return@mapNotNull item
            if (receipt.error == "account_mismatch") return@mapNotNull PendingPurchase(item.purchase, if (item.purchase.binding == null) null else item.owner, item.submittedAt, item.rejectedFor + account.binding)
            if (receipt.state == "Verifying") return@mapNotNull item
            check((receipt.kind == PlayKind.SUBSCRIPTION || receipt.product == item.purchase.product) && receipt.kind == item.purchase.kind && (item.purchase.binding == null || item.purchase.binding == account.binding)) { "ACCOUNT_MISMATCH" }
            if ((receipt.delivered && !receipt.confirmationPending) || receipt.state in setOf("PendingCanceled", "Refunded", "Expired")) {
                delivered[item.purchase.id] = account.binding
                null
            } else PendingPurchase(item.purchase, account, item.submittedAt, item.rejectedFor)
        }
        save(ledger.copy(pending = pending, delivered = delivered.entries.toList().takeLast(512).associate { it.toPair() }))
    }
    fun canLaunch(account: PlayAccount): Boolean = synchronized(lock) { account.validate(); val value = read(); value.pending.size < 256 && value.pending.none { account.binding !in it.rejectedFor && (it.owner == account || it.owner == null && (it.purchase.binding == null || it.purchase.binding == account.binding)) } }
    companion object { private val lock = Any() }
}
