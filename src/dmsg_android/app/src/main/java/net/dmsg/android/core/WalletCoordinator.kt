package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.core.*

class WalletCoordinator(private val client: ClientController) {
    val wallets = ReadStore<List<WalletSummary>> { it.isEmpty() }
    val pending = ReadStore<List<WalletPending>> { it.isEmpty() }
    val details = ReadStore<WalletDetails>()
    val history = ReadStore<WalletProposalPage> { it.proposals.isEmpty() }
    val balances = ReadStore<WalletBalances>()
    val assets = ReadStore<List<WalletAsset>> { it.isEmpty() }
    val proposal = ReadStore<WalletProposal>()
    val cosign = ReadStore<WalletCosignStatus>()
    val localKeys = ReadStore<List<LocalWalletKey>> { it.isEmpty() }
    val native = ReadStore<NativeWalletKeys>()
    val bindings = ReadStore<List<String>> { it.isEmpty() }
    val move = ReadStore<NativeWalletMove>()
    private var moveTarget: Triple<String, UInt, Boolean>? = null
    private val controlsValue = MutableStateFlow<Map<String, WalletControlDraft>>(emptyMap())
    val controls = controlsValue.asStateFlow()
    private var selected: String? = null
    private var selectedProposal: Pair<String, ULong>? = null
    private val transferValue = MutableStateFlow<Map<String, WalletTransferDraft>>(emptyMap())
    val transfers = transferValue.asStateFlow()
    private val previewsValue = MutableStateFlow<Map<String, WalletTransferPreview>>(emptyMap())
    val previews = previewsValue.asStateFlow()
    private val submissionsValue = MutableStateFlow<Map<String, WalletSubmission>>(emptyMap())
    val submissions = submissionsValue.asStateFlow()
    private val feeDecimals = mutableMapOf<String, Int>()
    private val errorsValue = MutableStateFlow<Map<String, String>>(emptyMap())
    val errors = errorsValue.asStateFlow()
    fun feePrecision(wallet: String): Int? = feeDecimals[wallet]

    fun initializeControls(value: WalletDetails) {
        if (value.id !in controlsValue.value) controlsValue.value += value.id to WalletControlDraft(approvers = value.policy.approvers.toString(), administrators = value.policy.administrators.toString())
    }
    fun editControls(wallet: String, value: WalletControlDraft) { client.activity(); controlsValue.value += wallet to value }
    fun newSubmission(key: String) { if (submissionsValue.value[key]?.result != null) submissionsValue.value -= key }
    fun change(wallet: String, change: WalletChange) = submit("controls:" + wallet, wallet) { core, request -> core.walletChange(wallet, change, request) }
    fun freeze(wallet: String) = client.perform({ it.freezeWallet(wallet) }, key = "wallet:freeze:" + wallet) { select(wallet, true) }
    fun keys() {
        val request = localKeys.begin()
        client.perform({ parseLocalWalletKeys(it.walletCosigners().decode()) }, key = "wallet:keys", replace = true, quiet = true, onError = { localKeys.fail(request, it) }) { localKeys.accept(request, it) }
    }
    fun newKey(label: String) = client.perform({ it.newWalletCosigner(label.trim()) }, key = "wallet:new-key") { keys() }
    fun testKey(label: String) = client.perform({ it.walletCosignerTest(label) }, key = "wallet:test-key") { client.notice("KEY_VERIFIED") }
    fun registerKey(wallet: String, label: String) = submit("register-key:" + label, wallet) { core, request -> core.registerWalletCosigner(wallet, label, request) }
    fun deviceKeys(wallet: String) {
        select(wallet); keys()
        val request = native.begin()
        client.perform({ parseNativeWalletKeys(it.walletNativeStatus(wallet).decode()) }, key = "wallet:native", replace = true, quiet = true, onError = { native.fail(request, it) }) { native.accept(request, it) }
        val bound = bindings.begin()
        client.perform({ it.walletBindings(wallet).decode().objectValue().strings("unbind") }, key = "wallet:bindings", replace = true, quiet = true, onError = { bindings.fail(bound, it) }) { bindings.accept(bound, it) }
    }
    fun nativeKey(wallet: String, action: String, biometry: Boolean, index: UInt? = null) = client.perform({ core -> when (action) {
        "create" -> core.walletNativeCreate(biometry)
        "add" -> core.walletNativeProtectorAdd(biometry)
        "remove" -> core.walletNativeProtectorRemove(index ?: error("INVALID_INPUT"))
        else -> error("INVALID_INPUT")
    } }, key = "wallet:native-change", onError = { deviceKeys(wallet) }) { deviceKeys(wallet) }
    fun replaceKey(wallet: String, cosigner: UInt, label: String) = submit("replace-key:" + wallet + ":" + cosigner, wallet) { core, request -> core.replaceWalletCosigner(wallet, cosigner, label, request) }
    fun unbind(wallet: String, devices: List<String>) = client.approvals.request("wallet:unbind:" + wallet) { it.walletUnbind(wallet, devices.take(4)) }
    fun move(wallet: String, from: UInt, l2: Boolean) {
        val target = Triple(wallet, from, l2)
        if (moveTarget != target) { move.reset(); moveTarget = target }
        val request = move.begin()
        client.perform({ parseNativeWalletMove(it.walletNcMigratePlan(wallet, from, l2).decode()) }, key = "wallet:move", replace = true, quiet = true, onError = { move.fail(request, it) }) { move.accept(request, it) }
    }
    fun moveNext(wallet: String, from: UInt, l2: Boolean) = client.perform({ it.walletNcMigrateStart(wallet, from, l2) }, key = "wallet:move-next", onError = { move(wallet, from, l2) }) { move(wallet, from, l2) }
    fun cosignHere(wallet: String, id: ULong, plan: UInt) = client.approvals.request("wallet:cosign:" + wallet + ":" + id + ":" + plan) { it.walletCosignRequest(wallet, id, plan) }
    fun importPacket(wallet: String, id: ULong, plan: UInt, packet: String) = client.perform({ it.walletCosignImport(wallet, id, plan, packet) }, key = "wallet:import-packet") { cosign(wallet, id, plan) }
    fun executeCosigned(wallet: String, id: ULong, plan: UInt) = client.perform({ core -> client.operation { core.walletCosignExecute(wallet, id, plan, it) } }, key = "wallet:execute:" + wallet + ":" + id, onError = { proposal(wallet, id) }) { proposal(wallet, id) }

    private fun <T> cached(store: ReadStore<T>, kind: String, id: String, force: Boolean, decode: (JsonElement) -> T) {
        val request = store.begin()
        fun accept(value: JsonElement) {
            val row = value.objectValue()
            if (row.required("state") == "missing") { check(row["data"] == JsonNull); return }
            check(row.required("state") == "present") { "INTEGRITY_FAILED" }
            row.requiredUnsigned("observed_at_ms")
            store.accept(request, decode(row["data"]!!))
            if (row.requiredFlag("stale")) store.fail(request, "STALE")
        }
        fun refresh() = client.perform({ core -> client.operation { core.refreshDisplay(kind, id, force, it) }.decode() }, key = "wallet:display:" + kind, replace = true, quiet = true, onError = { store.fail(request, it) }, apply = ::accept)
        client.perform({ it.cachedDisplay(kind, id).decode() }, key = "wallet:cached:" + kind, replace = true, quiet = true, onError = { refresh() }) { accept(it); refresh() }
    }
    fun list(force: Boolean = false) { cached(wallets, "wallets", "", force, ::parseWallets); pending() }
    fun pending() {
        val request = pending.begin()
        client.perform({ parseWalletPending(it.pendingWalletProposals().decode()) }, key = "wallet:pending", replace = true, quiet = true, onError = { pending.fail(request, it) }) { pending.accept(request, it) }
    }
    fun select(wallet: String, force: Boolean = false) {
        if (selected != wallet) { selected = wallet; details.reset(); history.reset(); balances.reset(); assets.reset() }
        cached(details, "wallet", wallet, force) { parseWalletDetails(it).also { value -> check(value.id == wallet) { "INTEGRITY_FAILED" } } }
        cached(history, "history", wallet, force, ::parseWalletProposalPage)
    }
    fun balances(wallet: String) = cached(balances, "balances", wallet, true) { parseWalletBalances(it).also { value -> check(value.wallet == wallet) { "INTEGRITY_FAILED" } } }
    fun transfer(wallet: String) {
        select(wallet)
        if (wallet !in transferValue.value) transferValue.value += wallet to WalletTransferDraft(wallet)
        val request = assets.begin()
        client.perform({ parseWalletAssets(it.walletAssets(wallet).decode()) }, key = "wallet:assets", replace = true, quiet = true, onError = { assets.fail(request, it) }) { assets.accept(request, it) }
    }
    fun edit(value: WalletTransferDraft) {
        if (submissionsValue.value["transfer:" + value.wallet]?.attempted == true) return
        client.activity()
        val previous = transferValue.value[value.wallet]
        transferValue.value += value.wallet to if (previous?.asset != value.asset) value.copy(maxFee = "") else value
        if (previous?.asset != value.asset) feeDecimals.remove(value.wallet)
        previewsValue.value -= value.wallet; errorsValue.value -= "transfer:" + value.wallet
    }
    fun input(value: WalletTransferDraft, maxFee: String? = null, request: String = ""): WalletTransfer? {
        val asset = assets.value()?.find { it.key == value.asset && it.error == null && it.decimals != null } ?: return null
        val amount = AtomicAmount.parse(value.amount, asset.decimals!!) ?: return null
        if (amount == "0" || value.recipient.trim().isEmpty() || details.value()?.takeIf { it.id == value.wallet }?.addresses?.none { it.id == value.from && it.verified && !it.archived && it.chain == asset.chain } != false) return null
        val fee = maxFee ?: if (value.maxFee.isBlank()) "" else feeDecimals[value.wallet]?.let { AtomicAmount.parse(value.maxFee, it) } ?: return null
        return WalletTransfer(value.wallet, value.from, value.asset, value.recipient.trim(), amount, fee, null, request)
    }
    fun preview(wallet: String) {
        val draft = transferValue.value[wallet] ?: return
        val input = input(draft) ?: return
        previewsValue.value -= wallet; errorsValue.value -= "transfer:" + wallet
        client.perform({ parseWalletTransferPreview(it.previewWalletTransfer(input).decode()) }, key = "wallet:preview:" + wallet, replace = true, quiet = true, onError = { errorsValue.value += ("transfer:" + wallet) to it }) { preview ->
            if (transferValue.value[wallet] == draft) { feeDecimals[wallet] = preview.feeDecimals; previewsValue.value += wallet to preview }
        }
    }
    fun submitTransfer(wallet: String) {
        val key = "transfer:" + wallet
        val submission = submissionsValue.value[key] ?: WalletSubmission()
        if (submission.attempted) { submit(key, wallet) { _, _ -> error("EXECUTION_UNKNOWN") }; return }
        val draft = transferValue.value[wallet] ?: return
        val preview = previewsValue.value[wallet] ?: return
        val now = System.currentTimeMillis().toULong()
        if (!preview.canSubmit(now)) { previewsValue.value -= wallet; errorsValue.value += key to "PREVIEW_EXPIRED"; return }
        val input = input(draft, preview.maxFee) ?: return
        submit(key, wallet) { core, request -> core.walletTransfer(input.copy(requestId = request)) }
    }
    fun submit(key: String, wallet: String, create: suspend (MobileEngine, String) -> ObjectSnapshot) {
        val original = submissionsValue.value[key] ?: WalletSubmission()
        if (original.result != null || client.isWorking("wallet:submit:" + key)) return
        submissionsValue.value += key to original.copy(attempted = true, error = null)
        client.perform({ core ->
            if (original.attempted) core.resumeWalletProposal(original.request).decode().objectValue()
            else create(core, original.request).decode().objectValue()
        }, key = "wallet:submit:" + key, quiet = true, onError = { reason ->
            errorsValue.value += key to reason
            client.perform({ it.walletProposalAttempt(original.request) }, key = "wallet:journal:" + original.request, quiet = true, onError = { submissionsValue.value += key to original.copy(attempted = true, error = reason); pending() }) { journal ->
                submissionsValue.value += key to original.failed(journal, reason); pending()
            }
        }) { result ->
            check(result.required("wallet") == wallet) { "INTEGRITY_FAILED" }
            submissionsValue.value += key to original.copy(attempted = true, result = result)
            errorsValue.value -= key; pending()
            client.navigation.push(Route("wallet-proposal", wallet, sequence = result.requiredUnsigned("id")))
        }
    }
    fun resume(entry: WalletPending) {
        val key = "pending:" + entry.id
        submissionsValue.value += key to WalletSubmission(entry.id, attempted = true)
        submit(key, entry.wallet) { _, _ -> error("EXECUTION_UNKNOWN") }
    }
    fun join(wallet: String) = client.approvals.request("wallet:join:" + wallet) { it.walletJoin(wallet.trim(), emptyList(), false) }
    fun proposal(wallet: String, id: ULong) {
        if (selectedProposal != (wallet to id)) { proposal.reset(); cosign.reset(); selectedProposal = wallet to id }
        val request = proposal.begin()
        client.perform({ parseWalletProposal(it.walletProposal(wallet, id).decode()) }, key = "wallet:proposal", replace = true, quiet = true, onError = { proposal.fail(request, it) }) { value ->
            check(value.id == id) { "INTEGRITY_FAILED" }; proposal.accept(request, value)
            cosign(wallet, id, value.plans)
        }
    }
    fun decide(wallet: String, id: ULong, approve: Boolean) = client.approvals.request("wallet:decision:" + wallet + ":" + id + ":" + approve) { it.walletDecision(wallet, id, approve) }
    fun run(wallet: String, id: ULong, action: String) {
        client.perform({ core -> when (action) {
            "execute" -> client.operation { core.executeWallet(wallet, id, it) }
            "reconcile" -> client.operation { core.reconcileWallet(wallet, id, it) }
            "broadcast" -> client.operation { core.broadcastWallet(wallet, id, it) }
            "cancel" -> core.cancelWalletProposal(wallet, id)
            else -> error("INVALID_INPUT")
        }.decode() }, key = "wallet:execute:" + wallet + ":" + id, onError = { proposal(wallet, id) }) { result ->
            val row = result.objectValue()
            if (row.text("type") != null && row.text("digest") != null) client.approvals.open(row.required("id"))
            proposal(wallet, id)
        }
    }
    fun cosign(wallet: String, id: ULong, plan: UInt) {
        val request = cosign.begin()
        client.perform({ parseWalletCosign(it.walletCosignStatus(wallet, id, plan).decode()) }, key = "wallet:cosign", replace = true, quiet = true, onError = { cosign.fail(request, it) }) { cosign.accept(request, it) }
    }
    fun clear(retainForms: Boolean = false) {
        wallets.reset(); pending.reset(); details.reset(); history.reset(); balances.reset(); assets.reset(); proposal.reset(); cosign.reset(); localKeys.reset(); native.reset(); bindings.reset(); move.reset(); moveTarget = null; selected = null; selectedProposal = null
        previewsValue.value = emptyMap()
        if (!retainForms) { transferValue.value = emptyMap(); submissionsValue.value = emptyMap(); controlsValue.value = emptyMap(); feeDecimals.clear(); errorsValue.value = emptyMap() }
    }
}
data class WalletCosignStatus(val collected: ULong, val required: ULong, val ready: Boolean, val packet: String, val canCosign: Boolean?, val stale: List<UInt>)
fun parseWalletCosign(value: JsonElement) = value.objectValue().let { row ->
    val stale = (row["stale"] as? JsonArray)?.map { it.jsonPrimitive.content.toUIntOrNull() ?: error("INTEGRITY_FAILED") }.orEmpty()
    WalletCosignStatus(row.requiredUnsigned("collected"), row.requiredUnsigned("required"), row.requiredFlag("ready"), row.required("packet"), row.optionalFlag("can_cosign"), stale)
}
