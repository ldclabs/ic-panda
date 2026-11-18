package net.dmsg.android.platform

import android.app.Activity
import kotlinx.coroutines.flow.StateFlow
import net.dmsg.android.core.*

data class PlayBillingStatus(val revision: Long = 0, val launching: Boolean = false, val event: String? = null, val ownerBinding: String? = null)
class PlayReplacement(val purchase: PlayPurchase, val deferred: Boolean)
interface PlayBillingGateway {
    val available: Boolean
    val status: StateFlow<PlayBillingStatus>
    fun takeCancellation(ownerBinding: String): Boolean = false
    suspend fun products(selections: List<PlaySelection>): PlayProductQuery
    suspend fun restore(force: Boolean = false): List<PlayPurchase>
    fun currentPurchases(): List<PlayPurchase>
    fun launch(activity: Activity, offer: PlayOffer, owner: PlayAccount, baseContract: String?, replacement: PlayReplacement?): String?
}
