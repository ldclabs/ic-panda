package net.dmsg.android.platform

import android.app.Activity
import android.content.Context
import com.android.billingclient.api.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import net.dmsg.android.core.*
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** One application-owned listener. It persists receipts even when the old UI is gone. */
class GooglePlayBilling(private val context: Context, private val store: PlayPurchaseStore) : PlayBillingGateway {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val mutable = MutableStateFlow(PlayBillingStatus())
    override val status = mutable.asStateFlow()
    override val available get() = GooglePush(context).available && runCatching { context.packageManager.getInstallSourceInfo(context.packageName).installingPackageName == "com.android.vending" }.getOrDefault(false)
    private val connection = Mutex()
    private val known = mutableMapOf<String, PlayKind>()
    private data class NativeOffer(val details: ProductDetails, val token: String, val offer: PlayOffer)
    private val offers = mutableMapOf<String, NativeOffer>()
    @Volatile private var owned = emptyList<PlayPurchase>()
    private var launchOwner: PlayAccount? = null
    private var launchSelection: PlaySelection? = null
    private val billing by lazy {
        BillingClient.newBuilder(context.applicationContext).setListener { result, purchases ->
            val originalOwner = launchOwner
            val originalSelection = launchSelection
            scope.launch {
                try {
                    if (result.responseCode == BillingClient.BillingResponseCode.OK && purchases != null) {
                        var knownCount = 0
                        val converted = purchases.flatMap { p ->
                            val product = p.products.singleOrNull() ?: error("PLAY_UNSUPPORTED_PURCHASE")
                            val kind = known[product] ?: originalSelection?.takeIf { it.product == product }?.kind
                            if (kind == null) emptyList() else { knownCount++; convert(p, kind) }
                        }.sortedBy { it.pending }.distinctBy { it.id }
                        withContext(Dispatchers.IO) { converted.forEach { store.enqueue(it, originalOwner) } }
                        owned = owned.filterNot { old -> converted.any { it.id == old.id } } + converted
                        publish(false, if (converted.any { it.pending }) "pending" else "received", originalOwner?.binding ?: converted.singleOrNull()?.binding)
                        if (knownCount != purchases.size) restore()
                    } else publish(false, billingEvent(result.responseCode), originalOwner?.binding)
                } catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Throwable) { publish(false, "PAYMENT_STORAGE_UNAVAILABLE", originalOwner?.binding) }
                finally { launchOwner = null; launchSelection = null }
            }
        }.enablePendingPurchases(PendingPurchasesParams.newBuilder().enableOneTimeProducts().build()).enableAutoServiceReconnection().build()
    }
    private fun publish(launching: Boolean, event: String?, binding: String? = null) { mutable.value = PlayBillingStatus(mutable.value.revision + 1, launching, event, binding) }
    private fun billingEvent(code: Int): String = when (code) {
        BillingClient.BillingResponseCode.OK -> "ready"
        BillingClient.BillingResponseCode.USER_CANCELED -> "cancelled"
        BillingClient.BillingResponseCode.ITEM_ALREADY_OWNED -> "PLAY_ALREADY_OWNED"
        BillingClient.BillingResponseCode.ITEM_UNAVAILABLE -> "PLAY_PRODUCT_UNAVAILABLE"
        BillingClient.BillingResponseCode.BILLING_UNAVAILABLE -> "GOOGLE_UNAVAILABLE"
        else -> "PLAY_SERVICE_UNAVAILABLE"
    }
    private suspend fun ready() = withContext(Dispatchers.Main.immediate) {
        check(available) { "GOOGLE_UNAVAILABLE" }
        connection.withLock {
            if (!billing.isReady) withTimeout(30_000) {
                suspendCancellableCoroutine<Unit> { continuation ->
                    billing.startConnection(object : BillingClientStateListener {
                        override fun onBillingSetupFinished(result: BillingResult) { if (continuation.isActive) {
                            if (result.responseCode == BillingClient.BillingResponseCode.OK) continuation.resume(Unit)
                            else continuation.resumeWithException(IllegalStateException(billingEvent(result.responseCode)))
                        } }
                        override fun onBillingServiceDisconnected() { if (continuation.isActive) continuation.resumeWithException(IllegalStateException("PLAY_SERVICE_UNAVAILABLE")) }
                    })
                }
            }
        }
    }
    override suspend fun products(selections: List<PlaySelection>): PlayProductQuery = withContext(Dispatchers.Main.immediate) {
        check(selections.size <= 128) { "INTEGRITY_FAILED" }
        selections.forEach { selected -> check(known[selected.product] == null || known[selected.product] == selected.kind) { "INTEGRITY_FAILED" }; known[selected.product] = selected.kind }
        ready()
        val found = mutableListOf<PlayOffer>()
        val desired = selections.filter { it.enabled }
        offers.clear()
        for (kind in PlayKind.entries) {
            val selected = desired.filter { it.kind == kind }
            for (batch in selected.map { it.product }.distinct().chunked(20)) {
                val params = QueryProductDetailsParams.newBuilder().setProductList(batch.map { QueryProductDetailsParams.Product.newBuilder().setProductId(it).setProductType(kind.sdk()).build() }).build()
                val result = withTimeout(30_000) { suspendCancellableCoroutine<QueryProductDetailsResult> { continuation ->
                    billing.queryProductDetailsAsync(params) { response, value -> if (continuation.isActive) {
                        if (response.responseCode == BillingClient.BillingResponseCode.OK) continuation.resume(value) else continuation.resumeWithException(IllegalStateException(billingEvent(response.responseCode)))
                    } }
                } }
                result.productDetailsList.forEach { details ->
                    selected.filter { it.product == details.productId }.forEach { selection ->
                        val native = offer(details, selection) ?: return@forEach
                        offers[selection.id] = native; found.add(native.offer)
                    }
                }
            }
        }
        PlayProductQuery(found, desired.map { it.id }.toSet() - found.map { it.selection.id }.toSet())
    }
    private fun offer(details: ProductDetails, selection: PlaySelection): NativeOffer? {
        val phases: List<PlayPricePhase>; val token: String
        if (selection.kind == PlayKind.SUBSCRIPTION) {
            val value = details.subscriptionOfferDetails?.singleOrNull { it.basePlanId == selection.option && it.offerId == selection.offer } ?: return null
            val prices = value.pricingPhases.pricingPhaseList
            if (value.installmentPlanDetails != null || prices.isEmpty() || prices.last().billingPeriod != "P1Y" || prices.last().recurrenceMode != ProductDetails.RecurrenceMode.INFINITE_RECURRING) return null
            phases = prices.map { PlayPricePhase(it.formattedPrice, it.billingPeriod, it.billingCycleCount, it.recurrenceMode == ProductDetails.RecurrenceMode.INFINITE_RECURRING) }
            token = value.offerToken
        } else {
            val value = details.oneTimePurchaseOfferDetailsList?.singleOrNull { it.purchaseOptionId == selection.option && it.offerId == selection.offer } ?: return null
            if (value.rentalDetails != null || value.preorderDetails != null) return null
            phases = listOf(PlayPricePhase(value.formattedPrice, "", 1, false)); token = value.offerToken ?: return null
        }
        if (token.isBlank() || phases.any { it.price.length !in 1..128 || it.period.length > 32 } || details.name.length > 2000 || details.description.length > 4000) return null
        return NativeOffer(details, token, PlayOffer(selection, details.name, details.description, phases, System.currentTimeMillis()))
    }
    private fun convert(purchase: Purchase, kind: PlayKind): List<PlayPurchase> {
        check(purchase.packageName == context.packageName && purchase.products.size == 1 && purchase.quantity == 1 && purchase.purchaseState in setOf(Purchase.PurchaseState.PURCHASED, Purchase.PurchaseState.PENDING)) { "PLAY_UNSUPPORTED_PURCHASE" }
        val current = PlayPurchase(purchase.purchaseToken, purchase.products.single(), kind, purchase.accountIdentifiers?.obfuscatedAccountId, purchase.accountIdentifiers?.obfuscatedProfileId, purchase.purchaseState == Purchase.PurchaseState.PENDING, purchase.purchaseTime, purchase.orderId).also(PlayPurchase::validate)
        val pending = purchase.pendingPurchaseUpdate ?: return listOf(current)
        check(kind == PlayKind.SUBSCRIPTION && pending.products.size == 1 && pending.purchaseToken != current.token) { "PLAY_UNSUPPORTED_PURCHASE" }
        val update = PlayPurchase(pending.purchaseToken, pending.products.single(), kind, current.binding, current.profile, true, current.purchasedAt, null).also(PlayPurchase::validate)
        return listOf(current, update)
    }
    override suspend fun restore(force: Boolean): List<PlayPurchase> = withContext(Dispatchers.Main.immediate) {
        ready()
        val purchases = mutableListOf<PlayPurchase>()
        for (kind in PlayKind.entries) {
            val params = QueryPurchasesParams.newBuilder().setProductType(kind.sdk()).build()
            val values = withTimeout(30_000) { suspendCancellableCoroutine<List<Purchase>> { continuation ->
                billing.queryPurchasesAsync(params) { result, values -> if (continuation.isActive) {
                    if (result.responseCode == BillingClient.BillingResponseCode.OK) continuation.resume(values) else continuation.resumeWithException(IllegalStateException(billingEvent(result.responseCode)))
                } }
            } }
            val converted = values.flatMap { convert(it, kind) }.sortedBy { it.pending }.distinctBy { it.id }
            withContext(Dispatchers.IO) { converted.forEach { store.enqueue(it, force = force) } }
            purchases.addAll(converted)
        }
        owned = purchases.toList()
        launchOwner = null; launchSelection = null
        publish(false, "restored")
        purchases
    }
    override fun currentPurchases(): List<PlayPurchase> = owned
    override fun launch(activity: Activity, offer: PlayOffer, owner: PlayAccount, baseContract: String?, replacement: PlayReplacement?): String? {
        if (!available || !billing.isReady || mutable.value.launching) return "GOOGLE_UNAVAILABLE"
        val native = offers[offer.selection.id]?.takeIf { it.offer == offer && System.currentTimeMillis() - offer.fetchedAt in 0..300_000 } ?: return "PLAY_PRODUCT_UNAVAILABLE"
        owner.validate()
        if (offer.selection.kind == PlayKind.STORAGE && (baseContract == null || !playHash(baseContract))) return "SUBSCRIPTION_REQUIRED"
        val product = BillingFlowParams.ProductDetailsParams.newBuilder().setProductDetails(native.details).setOfferToken(native.token)
        if (replacement != null) {
            check(offer.selection.kind == PlayKind.SUBSCRIPTION && replacement.purchase.kind == PlayKind.SUBSCRIPTION && !replacement.purchase.pending && (replacement.purchase.binding == null || replacement.purchase.binding == owner.binding)) { "ACCOUNT_MISMATCH" }
            product.setSubscriptionProductReplacementParams(BillingFlowParams.ProductDetailsParams.SubscriptionProductReplacementParams.newBuilder()
                .setOldProductId(replacement.purchase.product).setReplacementMode(if (replacement.deferred) BillingFlowParams.ProductDetailsParams.SubscriptionProductReplacementParams.ReplacementMode.DEFERRED else BillingFlowParams.ProductDetailsParams.SubscriptionProductReplacementParams.ReplacementMode.CHARGE_FULL_PRICE).build())
        }
        val params = BillingFlowParams.newBuilder().setProductDetailsParamsList(listOf(product.build())).setObfuscatedAccountId(owner.binding)
        baseContract?.let(params::setObfuscatedProfileId)
        replacement?.let { params.setSubscriptionUpdateParams(BillingFlowParams.SubscriptionUpdateParams.newBuilder().setOldPurchaseToken(it.purchase.token).build()) }
        launchOwner = owner; launchSelection = offer.selection
        publish(true, "confirming", owner.binding)
        val result = billing.launchBillingFlow(activity, params.build())
        if (result.responseCode != BillingClient.BillingResponseCode.OK) { launchOwner = null; launchSelection = null; publish(false, billingEvent(result.responseCode)); return billingEvent(result.responseCode) }
        return null
    }
    private fun PlayKind.sdk() = if (this == PlayKind.SUBSCRIPTION) BillingClient.ProductType.SUBS else BillingClient.ProductType.INAPP
}
