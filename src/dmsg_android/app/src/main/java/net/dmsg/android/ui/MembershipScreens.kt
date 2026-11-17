package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.str
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Verified
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.automirrored.outlined.ArrowForward
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*

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
    AssistChip(onClick = { client.navigation.push(Route("membership")) },
        label = { Text(if (known) current.tier else str(R.string.text_membership_ff9b3ff)) },
        leadingIcon = { Icon(if (known) Icons.Outlined.Verified else Icons.Outlined.Info, null, tint = MaterialTheme.colorScheme.primary) },
        modifier = Modifier.heightIn(min = 48.dp).testTag("membership-badge"))
}

@Composable fun MembershipScreen(client: ClientController, comparison: Boolean = false) {
    val plan by client.membership.plan.state.collectAsStateWithLifecycle()
    val catalog by client.membership.catalog.state.collectAsStateWithLifecycle()
    val usage by client.membership.usage.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val now = membershipTime()
    LaunchedEffect(state.epoch, comparison) { if (!comparison) client.membership.load() else if (catalog is ReadState.Idle) client.membership.loadCatalog() }
    val current = plan.valueOrNull()
    val tier = current?.takeIf { plan is ReadState.Ready && !it.needsRefresh(now) }?.tier
    LazyColumn(Modifier.fillMaxSize().testTag("membership-list"), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        if (!comparison) {
            item {
                SectionTitle(str(R.string.text_current_plan_and_limits_1d28c4a))
                ReadFeedback(plan, { client.membership.loadPlan(true) })
                if (current == null) Text(str(R.string.text_membership_status_is_unavailable_d38f278))
                else {
                    LabelValue(str(R.string.text_plan_fc68459), tier ?: str(R.string.text_status_needs_updating_1842722))
                    if (tier != null) {
                        LabelValue(str(R.string.text_storage_limit_bda7ca4), fileSize(current.limits.storage))
                        LabelValue(str(R.string.text_conversation_limit_cdef0ca), localizedNumber(current.limits.channels))
                        LabelValue(str(R.string.text_monthly_execution_limit_096e8a4), localizedNumber(current.limits.executions))
                        if (current.source == "Expired") Text(str(R.string.text_your_paid_membership_has_ended_you_are_now_on_free_6b08968))
                    } else Notice(str(R.string.text_refresh_your_membership_to_verify_the_current_benefits_90b41c1))
                    current.term?.let { term ->
                        if (tier != null && term.contract != null) Text(str(R.string.text_verified_service_term_33c14bd), Modifier.reminderVisible(client, NoticeTarget("plan", term.contract), revision = term), style = MaterialTheme.typography.labelMedium)
                        LabelValue(if (tier == null) str(R.string.text_last_recorded_term_7f08136) else str(R.string.text_current_term_ends_b4c54ff), displayTime(term.endsAt))
                        if (term.method == "AppStore") Text(str(R.string.text_manage_this_subscription_in_the_apple_account_that_purchased_it_d_7deb40b), style = MaterialTheme.typography.bodySmall)
                        if (term.environment in setOf("Sandbox", "Test")) Text(str(R.string.text_test_subscription_d0447bf), style = MaterialTheme.typography.labelMedium)
                    }
                }
                TextButton(onClick = { client.membership.load(true) }, enabled = !state.working("membership:plan")) { Text(str(R.string.text_refresh_membership_5fe7c8a)) }
            }
            item {
                SectionTitle(str(R.string.text_usage_ec3a2e3)); ReadFeedback(usage, client.membership::loadUsage)
                val counts = usage.valueOrNull()
                counts?.cloud?.let { cloud ->
                    LabelValue(str(R.string.text_used_20c05e4), fileSize(cloud.committed))
                    LabelValue(str(R.string.text_reserved_for_uploads_c224c25), fileSize(cloud.reserved))
                    LabelValue(str(R.string.text_remaining_storage_78b2855), cloud.remaining?.let(::fileSize) ?: str(R.string.text_unavailable_4ae32d2))
                    LabelValue(str(R.string.text_active_conversations_edbf7fc), cloud.channels?.let(::localizedNumber) ?: str(R.string.text_unavailable_4ae32d2))
                    cloud.channelLimit?.let { LabelValue(str(R.string.text_conversation_limit_cdef0ca), localizedNumber(it)) }
                }
                counts?.execution?.let { execution ->
                    Text("UTC · " + (execution.month / 100u) + "-" + (execution.month % 100u).toString().padStart(2, '0'), style = MaterialTheme.typography.labelLarge)
                    LabelValue(str(R.string.text_monthly_execution_units_9ebde11), localizedNumber(execution.charged) + " / " + localizedNumber(execution.allowed))
                    LabelValue(str(R.string.text_remaining_executions_307f41b), if (now < execution.validUntil) localizedNumber(execution.remaining) else str(R.string.text_status_needs_updating_1842722))
                }
                if (counts == null || counts.cloudError != null || counts.executionError != null) {
                    Notice(str(R.string.text_some_usage_is_unavailable_unknown_values_are_not_zero_5950ec4))
                    counts?.cloudError?.let { Text(str(R.string.text_cloud_storage_887f8fc) + errorText(it)) }
                    counts?.executionError?.let { Text(str(R.string.text_execution_usage_bf379b7) + errorText(it)) }
                    TextButton(onClick = client.membership::loadUsage, enabled = !state.working("membership:usage")) { Text(str(R.string.text_retry_usage_367035b)) }
                }
            }
            item { AccountRow(str(R.string.text_compare_all_plans_a6fbd12)) { client.navigation.push(Route("membership-compare")) } }
        }
        item {
            SectionTitle(str(R.string.text_plan_benefits_4ba263e)); ReadFeedback(catalog, client.membership::loadCatalog)
            Text(str(R.string.text_every_plan_includes_end_to_end_encryption_and_device_revocation_3f9af04), style = MaterialTheme.typography.bodyMedium)
        }
        items(catalog.valueOrNull()?.plans.orEmpty(), key = { it.tier }) { benefit ->
            Card(Modifier.fillMaxWidth().testTag("plan-card-" + benefit.tier), colors = CardDefaults.cardColors(containerColor = if (benefit.tier == tier) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surfaceContainer)) {
                Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(benefit.tier, style = MaterialTheme.typography.headlineSmall)
                    if (benefit.tier == tier) Text(str(R.string.text_current_plan_7901977), style = MaterialTheme.typography.labelLarge)
                    LabelValue(str(R.string.text_cloud_storage_3a75090), fileSize(benefit.limits.storage))
                    LabelValue(str(R.string.text_active_devices_2bf39f3), localizedNumber(benefit.limits.devices))
                    LabelValue(str(R.string.text_cloud_backup_file_limit_b0ce7c9), fileSize(benefit.backupFile))
                    LabelValue(str(R.string.text_active_file_request_links_67f03f4), localizedNumber(benefit.requests))
                    LabelValue(str(R.string.text_conversation_limit_cdef0ca), localizedNumber(benefit.limits.channels))
                    LabelValue(str(R.string.text_monthly_execution_limit_096e8a4), localizedNumber(benefit.limits.executions))
                    LabelValue(str(R.string.text_multisig_wallet_slots_50467bd), benefit.wallets?.let(::localizedNumber) ?: str(R.string.text_unavailable_4ae32d2))
                }
            }
        }
        item {
            Text(str(R.string.text_limits_are_for_the_base_plan_storage_packs_are_additional_changin_41b82a1), style = MaterialTheme.typography.bodySmall)
        }
        item { PlayPurchasePanel(client) }
    }
}
