package net.dmsg.android.core

import android.app.Activity
import android.app.Application
import android.content.Intent
import android.net.Uri
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.serialization.json.*
import net.dmsg.android.DMsgApplication
import net.dmsg.android.platform.*
import net.dmsg.core.*
import java.security.SecureRandom

enum class PurchasePhase { IDLE, LOADING, PREPARING, CONFIRMING, VERIFYING, PENDING, DELIVERED, UNAVAILABLE, FAILED }
data class PurchaseView(val phase: PurchasePhase = PurchasePhase.IDLE, val offers: List<PlayOffer> = emptyList(), val unavailable: Int = 0, val receipts: List<PlayReceipt> = emptyList(), val pending: Int = 0, val canBuy: Boolean = false, val selected: PlayOffer? = null, val error: String? = null, val beneficiary: String? = null, val cancelled: Boolean = false)

/** Screen epochs own presentation; the independent payment ledger owns receipts. */
class PlayPurchaseCoordinator(private val client: ClientController, private val application: Application, private val scope: CoroutineScope) {
    private val platform = GooglePush(application)
    private val billing: PlayBillingGateway get() = (application as DMsgApplication).playBilling
    private val store get() = (application as DMsgApplication).playPurchases
    private val mutable = MutableStateFlow(PurchaseView())
    val state = mutable.asStateFlow()
    private var catalogue: PlayCatalog? = null
    private var active = false
    private var observing: Job? = null
    private var wake: Job? = null
    private var again = false
    private var reviewRevision = 0L
    private val eligible get() = active && client.state.value.unlocked && !client.state.value.concealed

    fun start() {
        if (active || !client.state.value.unlocked || client.state.value.concealed) return
        active = true
        if (!platform.available || !billing.available) { mutable.value = PurchaseView(PurchasePhase.UNAVAILABLE); return }
        observing = scope.launch {
            billing.status.drop(1).collect { status ->
                if (!eligible || status.ownerBinding != null && status.ownerBinding != catalogue?.account?.binding) return@collect
                when (status.event) {
                    "received", "pending", "cancelled" -> {
                        mutable.value = mutable.value.copy(cancelled = status.event == "cancelled", canBuy = false)
                        refresh()
                    }
                    "PLAY_ALREADY_OWNED" -> refresh(queryStore = true)
                    "PAYMENT_STORAGE_UNAVAILABLE", "PLAY_SERVICE_UNAVAILABLE", "PLAY_PRODUCT_UNAVAILABLE" -> mutable.value = mutable.value.copy(phase = PurchasePhase.FAILED, error = status.event, canBuy = false)
                }
            }
        }
        load()
    }
    fun stop() {
        active = false; observing?.cancel(); observing = null; wake?.cancel(); wake = null
        client.cancelOperation(SYNC); client.cancelOperation(CHECKOUT)
        catalogue = null; again = false; reviewRevision++; mutable.value = PurchaseView()
    }
    fun clear() = stop()
    fun load() {
        if (!eligible) return
        if (!platform.available || !billing.available) { mutable.value = PurchaseView(PurchasePhase.UNAVAILABLE); return }
        if (client.isWorking(SYNC) || client.isWorking(CHECKOUT)) { again = true; return }
        val account = client.state.value.session?.account ?: return
        mutable.value = mutable.value.copy(phase = PurchasePhase.LOADING, error = null, canBuy = false)
        client.perform({ core ->
            val catalogue = parsePlayCatalog(core.googlePlayProducts().decode()) ?: error("GOOGLE_UNAVAILABLE")
            check(catalogue.account.account == account && catalogue.packageName == application.packageName) { "ACCOUNT_MISMATCH" }
            checkConfiguration(core, catalogue)
            store.bind(catalogue.account)
            val products = billing.products(catalogue.products)
            billing.restore()
            Loaded(catalogue, products, reconcile(core, catalogue.account, false, false))
        }, key = SYNC, quiet = true, onError = ::failed) { result ->
            catalogue = result.catalogue
            val selected = mutable.value.selected?.let { old -> result.products.offers.find { it.selection.id == old.selection.id } }
            if (selected != mutable.value.selected) reviewRevision++
            mutable.value = mutable.value.copy(beneficiary = result.catalogue.account.account, offers = result.products.offers, unavailable = result.products.unavailable.size, selected = selected)
            accept(result.reconciliation)
        }
    }
    fun restore() = refresh(force = true, recoverUnknown = true, queryStore = true)
    fun refresh(force: Boolean = false, recoverUnknown: Boolean = false, queryStore: Boolean = false) {
        if (!eligible) return
        val original = catalogue ?: return load()
        if (client.isWorking(SYNC) || client.isWorking(CHECKOUT)) { again = true; return }
        mutable.value = mutable.value.copy(phase = PurchasePhase.VERIFYING, error = null, canBuy = false)
        client.perform({ core ->
            if (queryStore) billing.restore(force)
            reconcile(core, original.account, force, recoverUnknown)
        }, key = SYNC, quiet = true, onError = ::failed, apply = ::accept)
    }
    private data class Reconciliation(val receipts: List<PlayReceipt>, val pending: Int, val canBuy: Boolean, val completed: Boolean)
    private data class Loaded(val catalogue: PlayCatalog, val products: PlayProductQuery, val reconciliation: Reconciliation)
    private suspend fun readReceipts(core: MobileEngine): List<PlayReceipt> {
        val result = mutableListOf<PlayReceipt>(); val cursors = mutableSetOf<String>(); var cursor: String? = null
        repeat(200) {
            val page = parsePlayReceipts(core.androidPurchases(cursor).decode())
            check(page.items.none { item -> result.any { it.tokenHash == item.tokenHash } }) { "INTEGRITY_FAILED" }
            result += page.items
            if (page.end) return result
            cursor = checkNotNull(page.next)
            check(cursors.add(cursor!!)) { "INTEGRITY_FAILED" }
        }
        error("INTEGRITY_FAILED")
    }
    private suspend fun reconcile(core: MobileEngine, owner: PlayAccount, force: Boolean, recoverUnknown: Boolean): Reconciliation {
        val before = store.pending(owner, recoverUnknown).size
        var receipts = readReceipts(core)
        store.reconcile(owner, receipts)
        val now = System.currentTimeMillis()
        val pending = store.pending(owner, recoverUnknown).filter { force || it.submittedAt == 0L || now - it.submittedAt >= 60_000 }.take(4)
        for (item in pending) {
            val details = AndroidPurchase(core.notificationInstallation(), item.purchase.token, item.purchase.kind.wire, item.purchase.product)
            val challenge = core.androidPurchaseChallenge(details).decode().objectValue()
            val proof = integrity(challenge)
            store.submitted(item.purchase.id, owner, System.currentTimeMillis())
            val accepted = core.androidSubmitPurchase(details, proof, requestId()).decode().objectValue()
            check(accepted.requiredFlag("accepted") && accepted.required("token_hash") == item.purchase.id) { "INTEGRITY_FAILED" }
        }
        if (pending.isNotEmpty()) { receipts = readReceipts(core); store.reconcile(owner, receipts) }
        val retained = store.pending(owner, recoverUnknown).size
        val outstanding = receipts.count { it.pending && it.error != "account_mismatch" || it.confirmationPending }
        return Reconciliation(receipts, maxOf(retained, outstanding), store.canLaunch(owner) && outstanding == 0 && !billing.status.value.launching, before > retained)
    }
    private suspend fun checkConfiguration(core: MobileEngine, catalogue: PlayCatalog) {
        val config = platform.configuration ?: error("GOOGLE_UNAVAILABLE")
        val remote = core.androidConfiguration().decode().objectValue()
        check(remote.requiredFlag("available") && remote.required("package_name") == config.packageName && remote.required("project_id") == config.projectId && remote.required("cloud_project_number") == config.projectNumber.toString() && remote.required("environment") == config.environment && catalogue.packageName == config.packageName) { "GOOGLE_UNAVAILABLE" }
        check(remote["play"]?.objectValue()?.requiredFlag("enabled") == true) { "GOOGLE_UNAVAILABLE" }
    }
    private suspend fun integrity(challenge: JsonObject): AndroidIntegrityProof {
        val id = challenge.required("challenge_id"); val hash = challenge.required("request_hash"); val expires = challenge.requiredUnsigned("expires_at")
        val now = System.currentTimeMillis().toULong()
        check(playHash(id) && hash.matches(Regex("[A-Za-z0-9_-]{43}")) && expires > now && expires <= now + 310_000uL) { "INTEGRITY_FAILED" }
        val token = platform.integrity(hash)
        check(System.currentTimeMillis().toULong() < expires) { "EXPIRED" }
        return AndroidIntegrityProof(id, token)
    }
    private fun accept(result: Reconciliation) {
        val changed = mutable.value.receipts != result.receipts
        val returnedCancellation = catalogue?.account?.binding?.let(billing::takeCancellation) == true
        val confirming = billing.status.value.launching
        mutable.value = mutable.value.copy(phase = when {
            result.pending > 0 -> PurchasePhase.PENDING
            result.completed -> PurchasePhase.DELIVERED
            confirming -> PurchasePhase.CONFIRMING
            else -> PurchasePhase.IDLE
        }, receipts = result.receipts, pending = result.pending, canBuy = result.canBuy && !confirming,
            cancelled = result.pending == 0 && !result.completed && (returnedCancellation || mutable.value.cancelled), error = null)
        if (result.completed || changed && result.receipts.any { it.delivered || it.state in setOf("Refunded", "Expired", "OnHold", "Paused") }) client.membership.load(true)
        schedule(result.pending > 0)
        deferred()
    }
    private fun failed(code: String) {
        mutable.value = mutable.value.copy(phase = if (code == "GOOGLE_UNAVAILABLE") PurchasePhase.UNAVAILABLE else if (code == "PLAY_RECOVERED") PurchasePhase.DELIVERED else PurchasePhase.FAILED, error = code, canBuy = false)
        if (code == "PLAY_RECOVERED") client.membership.load(true)
        schedule(mutable.value.pending > 0)
        deferred()
    }
    private fun deferred() {
        if (again) { again = false; scope.launch { yield(); if (eligible) refresh() } }
    }
    private fun schedule(pending: Boolean) {
        wake?.cancel()
        if (eligible && pending) wake = scope.launch { delay(15_000); if (eligible) refresh() }
    }
    fun select(offer: PlayOffer) {
        if (!eligible || mutable.value.phase in setOf(PurchasePhase.PREPARING, PurchasePhase.CONFIRMING)) return
        reviewRevision++; mutable.value = mutable.value.copy(selected = offer, cancelled = false, error = null)
    }
    fun cancelPreparation() { if (mutable.value.phase == PurchasePhase.PREPARING) { reviewRevision++; client.cancelOperation(CHECKOUT); mutable.value = mutable.value.copy(phase = PurchasePhase.IDLE); scope.launch { yield(); refresh() } } }
    fun dismiss() { cancelPreparation(); reviewRevision++; mutable.value = mutable.value.copy(selected = null) }
    private data class Checkout(val owner: PlayAccount, val base: String?, val replacement: PlayReplacement?, val expiry: ULong, val reviewedTerm: MembershipTerm?)
    fun buy(activity: Activity) {
        val catalogue = catalogue ?: return
        val offer = mutable.value.selected ?: return
        if (!eligible || !mutable.value.canBuy || mutable.value.pending > 0 || client.isWorking(CHECKOUT) || client.isWorking(SYNC) || billing.status.value.launching) return
        if (mutable.value.beneficiary != catalogue.account.account || client.state.value.session?.account != catalogue.account.account) { failed("ACCOUNT_MISMATCH"); return }
        val review = reviewRevision
        val reviewedTerm = if (offer.selection.kind == PlayKind.STORAGE) client.membership.plan.value()?.term else null
        mutable.value = mutable.value.copy(phase = PurchasePhase.PREPARING, cancelled = false, canBuy = false, error = null)
        client.perform({ core ->
            billing.restore()
            val outstanding = reconcile(core, catalogue.account, false, false)
            check(!outstanding.completed) { "PLAY_RECOVERED" }
            check(outstanding.canBuy) { "PLAY_PENDING" }
            val selection = offer.selection
            val preflight = core.googlePlayPreflight(selection.product, selection.option, selection.offer, application.packageName).decode().objectValue()
            check(preflight.requiredFlag("eligible")) { when(preflight.text("reason")) { "external_plan" -> "EXTERNAL_PLAN"; "subscription_required" -> "SUBSCRIPTION_REQUIRED"; "subscription_suspended" -> "SUBSCRIPTION_SUSPENDED"; else -> "GOOGLE_UNAVAILABLE" } }
            check(preflight.required("kind") == selection.kind.wire) { "INTEGRITY_FAILED" }
            val current = preflight["current"]?.takeIf { it != JsonNull }?.objectValue()
            val base = preflight.text("base_contract")
            if (selection.kind == PlayKind.STORAGE) check(reviewedTerm != null && reviewedTerm.contract == base && current?.unsigned("ends_at_ms") == reviewedTerm.endsAt) { "PLAY_PLAN_CHANGED" }
            val replacement = if (selection.kind == PlayKind.SUBSCRIPTION && current != null) {
                val oldHash = current.required("store_token_hash"); val oldTier = current.required("plan")
                check(oldTier != selection.plan) { "PLAY_CURRENT_PLAN" }
                val old = billing.currentPurchases().singleOrNull { it.id == oldHash && it.kind == PlayKind.SUBSCRIPTION } ?: error("PLAY_RESTORE_REQUIRED")
                val tiers = listOf("Free", "Plus", "Pro", "Max")
                check(oldTier in tiers && selection.plan in tiers) { "INTEGRITY_FAILED" }
                PlayReplacement(old, tiers.indexOf(selection.plan) < tiers.indexOf(oldTier))
            } else null
            val details = AndroidPurchasePreparation(core.notificationInstallation(), selection.product, selection.option, selection.offer, selection.kind.wire, base)
            val proof = integrity(core.androidPreparationChallenge(details).decode().objectValue())
            val prepared = core.androidPreparePurchase(details, proof, requestId()).decode().objectValue()
            check(prepared.requiredFlag("prepared")) { "INTEGRITY_FAILED" }
            Checkout(catalogue.account, base, replacement, prepared.requiredUnsigned("expires_at"), reviewedTerm)
        }, key = CHECKOUT, quiet = true, onError = ::failed) { prepared ->
            if (!eligible || review != reviewRevision || mutable.value.selected != offer || client.state.value.session?.account != prepared.owner.account || mutable.value.beneficiary != prepared.owner.account) return@perform
            if (prepared.reviewedTerm != null && client.membership.plan.value()?.term != prepared.reviewedTerm) { failed("PLAY_PLAN_CHANGED"); return@perform }
            if (System.currentTimeMillis().toULong() >= prepared.expiry) { failed("EXPIRED"); return@perform }
            client.beginExternalInteraction()
            val error = runCatching { billing.launch(activity, offer, prepared.owner, prepared.base, prepared.replacement) }.getOrElse { client.errorCode(it) }
            if (error == null) mutable.value = mutable.value.copy(phase = PurchasePhase.CONFIRMING, canBuy = false)
            else if (error == "cancelled") { mutable.value = mutable.value.copy(phase = PurchasePhase.IDLE, cancelled = true, canBuy = false); again = true }
            else failed(error)
            deferred()
        }
    }
    fun manage(activity: Activity) {
        client.beginExternalInteraction()
        val url = Uri.Builder().scheme("https").authority("play.google.com").path("/store/account/subscriptions").apply { catalogue?.packageName?.let { appendQueryParameter("package", it) } }.build()
        runCatching { activity.startActivity(Intent(Intent.ACTION_VIEW,url)) }.onFailure { failed("GOOGLE_UNAVAILABLE") }
    }
    private fun requestId() = ByteArray(32).also { SecureRandom().nextBytes(it) }.joinToString("") { "%02x".format(it) }
    companion object { private const val SYNC = "play:sync"; private const val CHECKOUT = "play:checkout" }
}
