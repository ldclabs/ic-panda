package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ViewList
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str
import net.dmsg.android.platform.numericExpression

@Composable private fun membershipTime(): ULong {
    var now by remember { mutableStateOf(System.currentTimeMillis().toULong()) }
    LaunchedEffect(Unit) { while (true) { now = System.currentTimeMillis().toULong(); kotlinx.coroutines.delay(1000) } }
    return now
}

@Composable fun MembershipBadge(client: ClientController) {
    val plan by client.membership.plan.state.collectAsStateWithLifecycle()
    val now = membershipTime()
    LaunchedEffect(Unit) { if (plan is ReadState.Idle) client.membership.loadPlan() }
    val current = plan.valueOrNull()
    val known = plan is ReadState.Ready && current?.needsRefresh(now) == false
    FilledTonalButton(onClick = { client.navigation.push(Route("membership")) },
        contentPadding = PaddingValues(horizontal = 14.dp), modifier = Modifier.testTag("membership-badge")) {
        Icon(if (known) Icons.Outlined.Verified else Icons.Outlined.Info, null, Modifier.size(18.dp))
        Spacer(Modifier.width(6.dp))
        Text(if (known) current.tier else str(R.string.text_membership_ff9b3ff))
    }
}

@Composable fun MembershipScreen(client: ClientController, comparison: Boolean = false) {
    val membership = client.membership
    val plan by membership.plan.state.collectAsStateWithLifecycle()
    val catalog by membership.catalog.state.collectAsStateWithLifecycle()
    val usage by membership.usage.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val now = membershipTime()
    LaunchedEffect(state.epoch, comparison) {
        if (comparison) {
            if (catalog is ReadState.Idle) membership.loadCatalog()
            if (plan is ReadState.Idle) membership.loadPlan()
        } else {
            membership.loadPlan()
            membership.loadUsage()
        }
    }
    val current = plan.valueOrNull()
    val tier = current?.takeIf { plan is ReadState.Ready && !it.needsRefresh(now) }?.tier
    val list = workspaceListState(client.navigation, if (comparison) "membership:compare" else "membership:overview")
    LazyColumn(Modifier.fillMaxSize().testTag("membership-list"), state = list, contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (comparison) {
            item {
                Text(str(R.string.text_every_plan_includes_end_to_end_encryption_and_device_revocation_3f9af04), style = MaterialTheme.typography.bodyMedium)
                ReadFeedback(catalog, membership::loadCatalog)
            }
            items(catalog.valueOrNull()?.plans.orEmpty(), key = { it.tier }) { benefit ->
                DetailCard(Modifier.testTag("plan-card-" + benefit.tier),
                    color = if (benefit.tier == tier) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.card) {
                    Text(benefit.tier, Modifier.semantics { heading() }, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
                    if (benefit.tier == tier) Text(str(R.string.text_current_plan_7901977), style = MaterialTheme.typography.labelLarge)
                    HorizontalDivider()
                    LabelValue(str(R.string.text_cloud_storage_3a75090), fileSize(benefit.limits.storage))
                    LabelValue(str(R.string.text_active_devices_2bf39f3), localizedNumber(benefit.limits.devices))
                    LabelValue(str(R.string.hosted_agent_ids), benefit.limits.agentIds?.let(::localizedNumber) ?: str(R.string.text_unavailable_4ae32d2))
                    LabelValue(str(R.string.text_cloud_backup_file_limit_b0ce7c9), fileSize(benefit.backupFile))
                    LabelValue(str(R.string.text_active_file_request_links_67f03f4), localizedNumber(benefit.requests))
                    LabelValue(str(R.string.text_conversation_limit_cdef0ca), localizedNumber(benefit.limits.channels))
                    LabelValue(str(R.string.text_monthly_execution_limit_096e8a4), localizedNumber(benefit.limits.executions))
                    LabelValue(str(R.string.text_multisig_wallet_slots_50467bd), benefit.wallets?.let(::localizedNumber) ?: str(R.string.text_unavailable_4ae32d2))
                }
            }
            item { Text(str(R.string.text_limits_are_for_the_base_plan_storage_packs_are_additional_changin_41b82a1), style = MaterialTheme.typography.bodySmall) }
        } else {
            item {
                DetailCard(color = if (tier != null) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.card) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        RowIcon(if (tier != null) Icons.Outlined.Verified else Icons.Outlined.Info, MaterialTheme.colorScheme.surfaceContainer)
                        Column(Modifier.weight(1f)) {
                            Text(str(R.string.text_current_plan_and_limits_1d28c4a), Modifier.semantics { heading() }, style = MaterialTheme.typography.labelLarge)
                            if (current != null) Text(tier ?: str(R.string.text_status_needs_updating_1842722),
                                style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold)
                        }
                        IconButton(onClick = { membership.loadPlan(true); membership.loadUsage() }, enabled = !state.working("membership:plan") && !state.working("membership:usage")) {
                            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_membership_5fe7c8a))
                        }
                    }
                    ReadFeedback(plan) { membership.loadPlan(true) }
                    if (current != null) {
                        if (tier != null) {
                            HorizontalDivider(color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.16f))
                            LabelValue(str(R.string.text_storage_limit_bda7ca4), fileSize(current.limits.storage))
                            LabelValue(str(R.string.text_conversation_limit_cdef0ca), localizedNumber(current.limits.channels))
                            LabelValue(str(R.string.text_monthly_execution_limit_096e8a4), localizedNumber(current.limits.executions))
                            if (current.source == "Expired") Text(str(R.string.text_your_paid_membership_has_ended_you_are_now_on_free_6b08968))
                        } else if (plan is ReadState.Ready) Notice(str(R.string.text_refresh_your_membership_to_verify_the_current_benefits_90b41c1))
                        current.term?.let { term ->
                            if (tier != null && term.contract != null) Text(str(R.string.text_verified_service_term_33c14bd),
                                Modifier.reminderVisible(client, NoticeTarget("plan", term.contract), revision = term), style = MaterialTheme.typography.labelMedium)
                            LabelValue(str(if (tier == null) R.string.text_last_recorded_term_7f08136 else R.string.text_current_term_ends_b4c54ff), displayTime(term.endsAt))
                            if (term.method == "Promo" || term.method == "Compensation") {
                                Text(str(R.string.promo_granted), style = MaterialTheme.typography.labelMedium)
                                Text(str(R.string.promo_no_charge), style = MaterialTheme.typography.bodySmall)
                            }
                            if (term.method == "AppStore") Text(str(R.string.text_manage_this_subscription_in_the_apple_account_that_purchased_it_d_7deb40b), style = MaterialTheme.typography.bodySmall)
                            if (term.environment in setOf("Sandbox", "Test")) Text(str(R.string.text_test_subscription_d0447bf), style = MaterialTheme.typography.labelMedium)
                        }
                    }
                }
            }
            item {
                DetailCard {
                    Text(str(R.string.text_usage_ec3a2e3), Modifier.semantics { heading() }, style = MaterialTheme.typography.titleLarge)
                    ReadFeedback(usage, membership::loadUsage)
                    val counts = usage.valueOrNull()
                    counts?.cloud?.let { cloud ->
                        val limit = cloud.limit
                        LabelValue(str(R.string.text_used_20c05e4), if (limit == null) fileSize(cloud.committed) else numericExpression(fileSize(cloud.committed) + " / " + fileSize(limit)))
                        if (limit != null && limit > 0uL) {
                            val occupied = (cloud.committed.toDouble() + cloud.reserved.toDouble()) / limit.toDouble()
                            LinearProgressIndicator(progress = { occupied.coerceIn(0.0, 1.0).toFloat() },
                                modifier = Modifier.fillMaxWidth().semantics { contentDescription = str(R.string.text_cloud_storage_3a75090) })
                        }
                        LabelValue(str(R.string.text_reserved_for_uploads_c224c25), fileSize(cloud.reserved))
                        LabelValue(str(R.string.text_remaining_storage_78b2855), cloud.remaining?.let(::fileSize) ?: str(R.string.text_unavailable_4ae32d2))
                        HorizontalDivider()
                        val active = cloud.channels?.let(::localizedNumber)
                        val maximum = cloud.channelLimit?.let(::localizedNumber)
                        LabelValue(str(R.string.text_active_conversations_edbf7fc), when {
                            active == null -> str(R.string.text_unavailable_4ae32d2)
                            maximum == null -> active
                            else -> numericExpression("$active / $maximum")
                        })
                        if (active == null && maximum != null) LabelValue(str(R.string.text_conversation_limit_cdef0ca), maximum)
                    }
                    counts?.execution?.let { execution ->
                        HorizontalDivider()
                        Text("UTC · " + (execution.month / 100u) + "-" + (execution.month % 100u).toString().padStart(2, '0'),
                            style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                        LabelValue(str(R.string.text_monthly_execution_units_9ebde11), numericExpression(localizedNumber(execution.charged) + " / " + localizedNumber(execution.allowed)))
                        LabelValue(str(R.string.text_remaining_executions_307f41b), if (now < execution.validUntil) localizedNumber(execution.remaining) else str(R.string.text_status_needs_updating_1842722))
                    }
                    if (counts != null && (counts.cloudError != null || counts.executionError != null)) {
                        Notice(str(R.string.text_some_usage_is_unavailable_unknown_values_are_not_zero_5950ec4))
                        counts.cloudError?.let { Text(str(R.string.text_cloud_storage_887f8fc) + errorText(it), style = MaterialTheme.typography.bodyMedium) }
                        counts.executionError?.let { Text(str(R.string.text_execution_usage_bf379b7) + errorText(it), style = MaterialTheme.typography.bodyMedium) }
                        if (usage is ReadState.Ready) TextButton(onClick = membership::loadUsage, enabled = !state.working("membership:usage")) { Text(str(R.string.text_retry_usage_367035b)) }
                    }
                }
            }
            item { Section { ListRow(str(R.string.text_compare_all_plans_a6fbd12), icon = Icons.AutoMirrored.Outlined.ViewList) { client.navigation.push(Route("membership-compare")) } } }
            item { DetailCard { PlayPurchasePanel(client) } }
        }
    }
}
