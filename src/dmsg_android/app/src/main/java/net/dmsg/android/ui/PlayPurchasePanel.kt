package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

private val purchaseWorking = setOf(PurchasePhase.LOADING, PurchasePhase.PREPARING, PurchasePhase.CONFIRMING, PurchasePhase.VERIFYING)

@Composable fun PlayPurchasePanel(client: ClientController) {
    val view by client.purchases.state.collectAsStateWithLifecycle()
    val plan by client.membership.plan.state.collectAsStateWithLifecycle()
    val activity = LocalActivity.current
    LaunchedEffect(client) { client.purchases.start() }
    DisposableEffect(client) { onDispose { client.purchases.cancelPreparation() } }
    PlayPurchaseContent(view, plan.valueOrNull(), activity != null,
        onSelect = client.purchases::select, onRetry = client.purchases::load,
        onRestore = client.purchases::restore, onClose = client.purchases::dismiss,
        onBuy = { activity?.let(client.purchases::buy) }, onManage = { activity?.let(client.purchases::manage) },
        onRefreshMembership = { client.membership.load(true) })
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable internal fun PlayPurchaseContent(view: PurchaseView, plan: MembershipPlan?, hasActivity: Boolean,
    onSelect: (PlayOffer) -> Unit, onRetry: () -> Unit, onRestore: () -> Unit, onClose: () -> Unit,
    onBuy: () -> Unit, onManage: () -> Unit, onRefreshMembership: () -> Unit) {
    var historyLimit by rememberSaveable { mutableIntStateOf(20) }
    val history = view.receipts.filter { it.pending || it.confirmationPending || it.state in setOf("Grace", "OnHold", "Paused", "Canceled", "Refunded") || it.nextProduct != null || it.error != null }
    Column(Modifier.fillMaxWidth().testTag("play-purchases"), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SectionTitle(str(R.string.text_google_play_50042fd))
        if (view.phase == PurchasePhase.UNAVAILABLE) {
            Text(str(R.string.text_purchases_are_unavailable_on_this_installation_you_can_still_view_fde228b))
        } else {
            if (view.selected == null) PurchaseProgress(view)
            if (view.unavailable > 0) Text(str(R.string.text_some_products_could_not_be_loaded_retry_to_check_their_current_av_cd30584), style = MaterialTheme.typography.bodySmall)
            view.offers.forEach { offer ->
                OutlinedCard(onClick = { onSelect(offer) }, enabled = view.phase !in setOf(PurchasePhase.PREPARING, PurchasePhase.CONFIRMING), modifier = Modifier.fillMaxWidth().testTag("play-offer-${offer.selection.id}")) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(offerTitle(offer), style = MaterialTheme.typography.titleMedium)
                        Text(offer.price + if (offer.selection.kind == PlayKind.SUBSCRIPTION) str(R.string.text_subscription_bbc9925) else str(R.string.text_one_time_purchase_4140825), style = MaterialTheme.typography.bodyLarge)
                        Text(str(R.string.text_review_price_and_terms_73f2eb8), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                    }
                }
            }
            if (view.offers.isEmpty() && view.phase !in purchaseWorking) Text(str(R.string.text_no_products_are_available_yet_9e7f4a4))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = onRetry, enabled = view.phase !in purchaseWorking, modifier = Modifier.testTag("play-retry")) { Text(str(R.string.text_retry_products_ea141ea)) }
                TextButton(onClick = onRestore, enabled = view.phase !in purchaseWorking, modifier = Modifier.testTag("play-restore")) { Text(str(R.string.text_restore_purchases_396c523)) }
            }
        }
        if (plan?.term?.method == "GooglePlay" || view.receipts.any { it.kind == PlayKind.SUBSCRIPTION }) {
            TextButton(onClick = { onManage() }, modifier = Modifier.testTag("play-manage"), enabled = hasActivity) { Text(str(R.string.text_manage_in_google_play_1469550)) }
            Text(str(R.string.text_change_renewal_payment_method_or_cancellation_in_the_google_accou_6ab74d5), style = MaterialTheme.typography.bodySmall)
        }
        history.take(historyLimit).forEach { receipt ->
            DetailCard {
                Text(view.offers.find { it.selection.product == receipt.product }?.title ?: str(R.string.text_previous_purchase_14e279e), style = MaterialTheme.typography.titleSmall)
                Text(receiptState(receipt), style = MaterialTheme.typography.bodyMedium)
                if (receipt.environment == "Test") Text(str(R.string.text_test_purchase_fa667fa), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                receipt.expires?.let { Text(str(R.string.text_current_term_802ec1b) + displayTime(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                if (receipt.nextProduct != null) Text(str(R.string.text_a_change_is_scheduled_for_the_next_term_current_benefits_remain_u_3f898e4), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        if (historyLimit < history.size) TextButton(onClick = { historyLimit += 20 }) { Text(str(R.string.text_show_more_purchases_650e0c2)) }
    }
    view.selected?.let { offer ->
        ModalBottomSheet(onDismissRequest = onClose, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
            Column(Modifier.fillMaxWidth().heightIn(max = 680.dp).verticalScroll(rememberScrollState()).padding(horizontal = pageMargin()).padding(bottom = pageMargin()).testTag("play-review"), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text(offerTitle(offer), style = MaterialTheme.typography.headlineSmall)
                LabelValue(str(R.string.text_account_4ab56e9), view.beneficiary ?: str(R.string.text_not_verified_e9586ea), code = view.beneficiary != null)
                if (offer.description.isNotBlank()) Text(offer.description, style = MaterialTheme.typography.bodyMedium)
                offer.phases.forEachIndexed { index, phase ->
                    LabelValue(if (offer.selection.kind == PlayKind.STORAGE) str(R.string.text_pay_once_84cad17) else if (phase.recurring) str(R.string.text_each_year_a4c5bef) else str(R.string.text_introductory_phase_f6477c6) + if (offer.phases.size > 1) " ${index + 1}" else "", phase.price)
                    if (!phase.recurring && offer.selection.kind == PlayKind.SUBSCRIPTION) Text(billingPeriod(phase.period, phase.cycles), style = MaterialTheme.typography.bodySmall)
                }
                if (offer.selection.kind == PlayKind.SUBSCRIPTION) Text(str(R.string.text_renews_automatically_until_canceled_in_google_play_upgrades_start_18d0413))
                else {
                    Text(str(R.string.text_this_storage_purchase_ends_with_the_current_subscription_term_and_4b9b65b))
                    offer.selection.storageBytes?.let { LabelValue(str(R.string.text_additional_storage_bf2a67d), fileSize(it)) }
                    plan?.term?.endsAt?.let { LabelValue(str(R.string.text_storage_term_ends_9ba72a5), displayTime(it)) }
                }
                PurchaseProgress(view)
                val storageReady = offer.selection.kind != PlayKind.STORAGE || offer.selection.storageBytes != null && plan?.let { !it.needsRefresh(System.currentTimeMillis().toULong()) && it.term?.method == "GooglePlay" } == true
                Button(onClick = { onBuy() }, enabled = hasActivity && view.beneficiary != null && view.canBuy && view.pending == 0 && view.phase !in purchaseWorking && storageReady, modifier = Modifier.fillMaxWidth().testTag("play-buy")) { Text(str(R.string.text_continue_with_google_play_c7da724)) }
                if (!storageReady) TextButton(onClick = { onRefreshMembership() }) { Text(str(R.string.text_refresh_membership_5fe7c8a)) }
                if (view.pending > 0 || view.phase == PurchasePhase.FAILED || view.phase == PurchasePhase.DELIVERED) TextButton(onClick = onRestore, enabled = view.phase !in purchaseWorking, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_check_the_original_purchase_0283791)) }
                TextButton(onClick = onClose, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_close_6cf4a77)) }
            }
        }
    }
}

private fun offerTitle(offer: PlayOffer) = offer.selection.plan?.let { "dMsg $it" } ?: offer.title

@Composable private fun PurchaseProgress(view: PurchaseView) {
    Column(Modifier.semantics { liveRegion = LiveRegionMode.Polite }.testTag("play-progress"), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        val label = when(view.phase) {
            PurchasePhase.LOADING -> str(R.string.text_loading_google_play_products_bc5d131)
            PurchasePhase.PREPARING -> str(R.string.text_1_preparing_your_purchase_5586f5a)
            PurchasePhase.CONFIRMING -> str(R.string.text_2_confirm_in_google_play_0714d1b)
            PurchasePhase.VERIFYING -> str(R.string.text_3_checking_delivery_f91776b)
            PurchasePhase.PENDING -> if (view.receipts.any { it.state == "Pending" }) str(R.string.text_payment_is_still_pending_in_google_play_db721a6) else str(R.string.text_your_purchase_is_being_verified_finish_restoring_it_before_buying_8803db3)
            PurchasePhase.DELIVERED -> str(R.string.text_purchase_recovered_membership_is_being_refreshed_9ba2023)
            PurchasePhase.IDLE -> if (view.cancelled && view.pending == 0) str(R.string.text_cancelled_bf41d7d) else null
            else -> null
        }
        label?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
        if (view.phase in purchaseWorking) LinearProgressIndicator(Modifier.fillMaxWidth())
        view.error?.let { Text(purchaseError(it), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
    }
}
private fun purchaseError(code: String) = when(code) {
    "PLAY_PENDING" -> str(R.string.text_an_earlier_purchase_still_needs_verification_restore_it_before_pu_81da8a4)
    "PLAY_RECOVERED" -> str(R.string.text_an_earlier_purchase_was_recovered_review_your_updated_membership_ca03138)
    "PLAY_CURRENT_PLAN" -> str(R.string.text_you_already_have_this_plan_manage_renewal_in_google_play_19b2cff)
    "PLAY_RESTORE_REQUIRED", "PLAY_ALREADY_OWNED" -> str(R.string.text_restore_the_existing_purchase_using_the_google_account_that_bough_5154042)
    "PLAY_PLAN_CHANGED" -> str(R.string.text_your_subscription_term_changed_refresh_membership_and_review_it_a_a24389d)
    "EXTERNAL_PLAN" -> str(R.string.text_your_current_plan_is_managed_through_another_payment_channel_mana_45fae38)
    "SUBSCRIPTION_REQUIRED" -> str(R.string.text_storage_purchases_require_an_active_google_play_subscription_in_t_30c697b)
    "SUBSCRIPTION_SUSPENDED" -> str(R.string.text_resolve_the_paused_subscription_or_payment_issue_in_google_play_f_49e40bc)
    "ACCOUNT_MISMATCH" -> str(R.string.text_this_purchase_belongs_to_a_different_dmsg_account_switch_to_its_o_affb973)
    "PAYMENT_STORAGE_UNAVAILABLE" -> str(R.string.text_the_saved_purchase_could_not_be_opened_keep_the_original_google_a_ab68078)
    "GOOGLE_UNAVAILABLE", "PLAY_PRODUCT_UNAVAILABLE", "PLAY_SERVICE_UNAVAILABLE" -> str(R.string.text_google_play_is_unavailable_check_your_connection_and_retry_do_not_f17918b)
    else -> errorText(code)
}
private fun receiptState(receipt: PlayReceipt) = when {
    receipt.error == "account_mismatch" -> purchaseError("ACCOUNT_MISMATCH")
    receipt.error != null -> str(R.string.text_verification_needs_another_attempt_your_original_purchase_is_reta_189a7ce)
    receipt.state == "Pending" -> str(R.string.text_awaiting_payment_in_google_play_070c16e)
    receipt.state == "PendingCanceled" -> str(R.string.text_pending_payment_canceled_b8acdce)
    receipt.state == "Grace" -> str(R.string.text_benefits_continue_during_the_payment_grace_period_update_your_pay_f7d9c2b)
    receipt.state in setOf("Paused", "OnHold") -> str(R.string.text_subscription_paused_or_payment_on_hold_manage_it_in_google_play_53497c3)
    receipt.state == "Canceled" -> str(R.string.text_renewal_canceled_current_benefits_continue_until_the_term_ends_4884ceb)
    receipt.state == "Refunded" -> str(R.string.text_purchase_refunded_or_revoked_7006a30)
    receipt.state == "Expired" -> str(R.string.text_subscription_term_ended_2cc045a)
    receipt.confirmationPending -> str(R.string.text_delivered_google_play_confirmation_is_still_being_checked_c7ef66e)
    receipt.pending -> str(R.string.text_verifying_delivery_d2669c3)
    else -> str(R.string.text_purchase_verified_6b1370a)
}
private fun billingPeriod(period: String, cycles: Int): String {
    val match = Regex("P([0-9]+)([DWMY])").matchEntire(period) ?: return str(R.string.text_offer_duration_shown_by_google_play_84bc5ff)
    val amount = match.groupValues[1].toIntOrNull()?.takeIf { it > 0 } ?: return str(R.string.text_offer_duration_shown_by_google_play_84bc5ff)
    val unit = when(match.groupValues[2]) { "D" -> android.icu.util.MeasureUnit.DAY; "W" -> android.icu.util.MeasureUnit.WEEK; "M" -> android.icu.util.MeasureUnit.MONTH; else -> android.icu.util.MeasureUnit.YEAR }
    val duration = android.icu.text.MeasureFormat.getInstance(net.dmsg.android.platform.AppLanguage.locale(), android.icu.text.MeasureFormat.FormatWidth.WIDE)
        .format(android.icu.util.Measure(amount, unit))
    return duration + if (cycles > 1) " × " + net.dmsg.android.platform.localizedNumber(cycles) else ""
}
