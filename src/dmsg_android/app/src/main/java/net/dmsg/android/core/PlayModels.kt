package net.dmsg.android.core

import kotlinx.serialization.json.*
import java.security.MessageDigest

enum class PlayKind(val wire: String) { SUBSCRIPTION("subscription"), STORAGE("storage");
    companion object { fun parse(value: String) = entries.singleOrNull { it.wire == value } ?: error("INTEGRITY_FAILED") }
}
data class PlayAccount(val home: String, val account: String, val binding: String) {
    fun validate() {
        check(home.matches(Regex("[a-z0-9-]{3,64}")) && validAccount(account) && playHash(binding)) { "INTEGRITY_FAILED" }
        check(binding == playDigest("dmsg/google-play/account/1\n$home\n$account")) { "INTEGRITY_FAILED" }
    }
}
data class PlaySelection(val product: String, val option: String, val offer: String?, val kind: PlayKind, val plan: String?, val storageProduct: String?, val enabled: Boolean, val storageBytes: ULong? = null) {
    val id get() = "$product|$option|${offer.orEmpty()}"
}
data class PlayCatalog(val account: PlayAccount, val packageName: String, val environment: String, val acceptTest: Boolean, val products: List<PlaySelection>)
data class PlayPricePhase(val price: String, val period: String, val cycles: Int, val recurring: Boolean)
data class PlayOffer(val selection: PlaySelection, val title: String, val description: String, val phases: List<PlayPricePhase>, val fetchedAt: Long) {
    val price get() = phases.first().price
}
data class PlayProductQuery(val offers: List<PlayOffer>, val unavailable: Set<String>)
data class PlayReceipt(val tokenHash: String, val product: String, val kind: PlayKind, val environment: String?, val state: String, val expires: ULong?, val delivered: Boolean, val pending: Boolean, val confirmationPending: Boolean, val nextProduct: String?, val error: String?)
data class PlayReceiptPage(val items: List<PlayReceipt>, val next: String?, val end: Boolean)

/** Kept out of UI state and diagnostics. The raw receipt is only for verification. */
class PlayPurchase(val token: String, val product: String, val kind: PlayKind, val binding: String?, val profile: String?, val pending: Boolean, val purchasedAt: Long, val order: String?) {
    val id get() = playDigest(token)
    override fun toString() = "PlayPurchase(redacted)"
    fun validate() {
        check(token.isNotEmpty() && token.length <= 4096 && token.none(Char::isWhitespace) && playProductId(product) && (binding == null || playHash(binding)) && (profile == null || playHash(profile)) && purchasedAt >= 0 && (order == null || playProductId(order))) { "INTEGRITY_FAILED" }
    }
}
fun playHash(value: String) = value.matches(Regex("[a-f0-9]{64}"))
fun playProductId(value: String) = value.matches(Regex("[A-Za-z0-9._-]{1,200}"))
fun playDigest(value: String): String = MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }

fun parsePlayCatalog(value: JsonElement): PlayCatalog? {
    val root = value.objectValue()
    val account = PlayAccount(root.required("home"), root.required("account"), root.required("account_token")).also { it.validate() }
    val row = root["configuration"]?.takeIf { it != JsonNull }?.objectValue() ?: return null
    val storageProducts = root["storage_products"]?.arrayRows()?.associate { it.required("id").also { id -> check(playHash(id)) } to it.requiredUnsigned("bytes") }.orEmpty()
    val products = row.rows("products").map { product ->
        val kind = product["kind"]!!.objectValue()
        check(kind.size == 1) { "INTEGRITY_FAILED" }
        val plan = kind["Plan"]?.objectValue(); val storage = kind["Storage"]?.objectValue()
        check((plan != null) != (storage != null)) { "INTEGRITY_FAILED" }
        val tier = plan?.required("plan")?.also { check(it in setOf("Plus", "Pro", "Max")) { "INTEGRITY_FAILED" } }
        val hash = storage?.get("product")?.let { bytes ->
            check(bytes is JsonArray && bytes.size == 32) { "INTEGRITY_FAILED" }
            bytes.joinToString("") { item -> "%02x".format(item.jsonPrimitive.int.also { check(it in 0..255) { "INTEGRITY_FAILED" } }) }
        }
        PlaySelection(product.required("product_id"), plan?.required("base_plan_id") ?: storage!!.required("purchase_option_id"), product.text("offer_id"), if (plan != null) PlayKind.SUBSCRIPTION else PlayKind.STORAGE, tier, hash, product.requiredFlag("enabled"), hash?.let(storageProducts::get)).also {
            check(playProductId(it.product) && playProductId(it.option) && (it.offer == null || playProductId(it.offer))) { "INTEGRITY_FAILED" }
        }
    }
    check(products.size <= 128 && products.map { it.id }.distinct().size == products.size) { "INTEGRITY_FAILED" }
    return PlayCatalog(account, row.required("package_name"), row.required("environment").also { check(it in setOf("Production", "Test")) }, row.requiredFlag("accept_test"), products)
}

fun parsePlayReceipts(value: JsonElement): PlayReceiptPage {
    val root = value.objectValue()
    val items = root.rows("items").map { row ->
        PlayReceipt(row.required("token_hash"), row.required("product_id"), PlayKind.parse(row.required("kind")), row.text("environment"), row.required("state"), row.unsigned("expires_at_ms"), row.requiredFlag("delivered"), row.requiredFlag("pending"), row.requiredFlag("confirmation_pending"), row.text("next_product"), row.text("error")).also {
            check(playHash(it.tokenHash) && playProductId(it.product) && it.state in setOf("Verifying", "Pending", "Active", "Grace", "Canceled", "Paused", "OnHold", "Expired", "PendingCanceled", "Refunded") && (it.environment == null || it.environment in setOf("Production", "Test"))) { "INTEGRITY_FAILED" }
            check(!it.delivered || !it.pending) { "INTEGRITY_FAILED" }
        }
    }
    val next = root.text("next_cursor"); val end = root.requiredFlag("end")
    check(items.size <= 50 && items.map { it.tokenHash }.distinct().size == items.size && (end == (next == null)) && (next == null || playHash(next) && items.isNotEmpty())) { "INTEGRITY_FAILED" }
    return PlayReceiptPage(items, next, end)
}
