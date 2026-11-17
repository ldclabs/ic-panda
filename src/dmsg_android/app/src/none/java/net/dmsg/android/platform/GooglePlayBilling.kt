package net.dmsg.android.platform

import android.app.Activity
import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.android.core.*

/** The none distribution has no Billing SDK, storefront or payment dialog. */
class GooglePlayBilling(context: Context, store: PlayPurchaseStore) : PlayBillingGateway {
    override val available = false
    override val status = MutableStateFlow(PlayBillingStatus()).asStateFlow()
    override suspend fun products(selections: List<PlaySelection>) = PlayProductQuery(emptyList(), selections.filter { it.enabled }.map { it.id }.toSet())
    override suspend fun restore(force: Boolean): List<PlayPurchase> = emptyList()
    override fun currentPurchases(): List<PlayPurchase> = emptyList()
    override fun launch(activity: Activity, offer: PlayOffer, owner: PlayAccount, baseContract: String?, replacement: PlayReplacement?) = "GOOGLE_UNAVAILABLE"
}
