package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*

data class AccountDeletionJob(val id: String, val state: String, val requestedAt: ULong?, val lastError: String?)
data class AccountDeletionStatus(val deletion: AccountDeletionJob?)
data class AccountDeletionChannel(val id: String, val name: String)
data class AccountDeletionPreview(val digest: String, val account: String, val pendingSync: ULong, val pendingOperation: Boolean, val channels: List<AccountDeletionChannel>, val wallets: List<WalletSummary>, val walletsAvailable: Boolean) {
    val requiresWalletReview get() = !walletsAvailable || wallets.isNotEmpty()
    fun canRequest(confirmation: String, walletReview: Boolean) = validAccount(account) && confirmation == account && !pendingOperation && channels.isEmpty() && (!requiresWalletReview || walletReview)
}
data class AccountDeletionDraft(val confirmation: String = "", val walletsReviewed: Boolean = false)
fun parseAccountDeletionJob(value: JsonElement) = value.objectValue().let { AccountDeletionJob(it.required("id"), it.required("state"), it.unsigned("requested_at"), it.text("last_error")) }
fun parseAccountDeletionStatus(value: JsonElement): AccountDeletionStatus {
    val row = value.objectValue(); check("deletion" in row) { "INTEGRITY_FAILED" }
    return AccountDeletionStatus(row["deletion"]?.takeIf { it != JsonNull }?.let(::parseAccountDeletionJob))
}
fun parseAccountDeletionPreview(value: JsonElement): AccountDeletionPreview {
    val row = value.objectValue(); val details = row["details"]!!.objectValue()
    val digest = row.required("digest"); check(digest.length == 64 && digest.all { it in '0'..'9' || it in 'a'..'f' }) { "INTEGRITY_FAILED" }
    return AccountDeletionPreview(digest, details.required("account"), details.requiredUnsigned("pending_sync"), details.requiredFlag("pending_account_operation"), details.rows("owned_channels").map { AccountDeletionChannel(it.required("id"), it.required("name")) }, parseWallets(details["wallets"]!!), details.requiredFlag("wallets_available"))
}
/** A known service job is authoritative across restarts; a transport failure never means deleted. */
class AccountDeletionCoordinator(private val client: ClientController) {
    val status = ReadStore<AccountDeletionStatus>()
    val preview = ReadStore<AccountDeletionPreview>()
    private val draftValue = MutableStateFlow(AccountDeletionDraft())
    val draft = draftValue.asStateFlow()
    private val errorValue = MutableStateFlow<String?>(null)
    val error = errorValue.asStateFlow()
    private var request: String? = null

    fun edit(value: AccountDeletionDraft) { client.activity(); draftValue.value = value }
    fun load() {
        val account = client.state.value.session?.account ?: return
        client.account.devices()
        val loaded = status.begin()
        client.perform({ parseAccountDeletionStatus(it.accountDeletionStatus().decode()) }, key = "account-deletion:status", replace = true, quiet = true, onError = { status.fail(loaded, it) }) { value ->
            status.accept(loaded, value)
            if (value.deletion != null) { request = value.deletion.id; preview.reset() }
            else {
                val review = preview.begin()
                client.perform({ parseAccountDeletionPreview(it.accountDeletionPreview().decode()) }, key = "account-deletion:preview", replace = true, quiet = true, onError = { preview.fail(review, it) }) {
                    check(it.account == account) { "INTEGRITY_FAILED" }; preview.accept(review, it)
                }
            }
        }
    }
    fun ready(): Boolean {
        val review = preview.value() ?: return false
        return preview.state.value is ReadState.Ready && status.state.value is ReadState.Ready && status.value()?.deletion == null &&
            client.account.devices.value()?.canManage == true && client.account.devices.state.value is ReadState.Ready &&
            review.account == client.state.value.session?.account && review.canRequest(draftValue.value.confirmation, draftValue.value.walletsReviewed)
    }
    fun submit() {
        if (!ready() || client.isWorking("account-deletion:request")) return
        val review = preview.value() ?: return
        val id = request ?: requestId().also { request = it }
        val reviewedWallets = draftValue.value.walletsReviewed
        errorValue.value = null
        client.perform({ parseAccountDeletionJob(it.requestAccountDeletion(review.digest, reviewedWallets, id).decode()) }, key = "account-deletion:request", quiet = true, onError = {
            errorValue.value = it
            load() // The service keeps one account deletion job even if the HTTP reply was lost.
        }) { result ->
            val loaded = status.begin(); status.accept(loaded, AccountDeletionStatus(result))
            request = result.id; preview.reset(); draftValue.value = AccountDeletionDraft()
        }
    }
    fun clear(retainDraft: Boolean = false) {
        status.reset(); preview.reset()
        if (!retainDraft) { draftValue.value = AccountDeletionDraft(); errorValue.value = null; request = null }
    }
}
