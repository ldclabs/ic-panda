package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import android.app.TimePickerDialog
import android.content.Intent
import android.provider.Settings
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationManagerCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.*
import java.time.ZoneId
import java.time.ZonedDateTime

@Composable internal fun NoticeSwitch(title: String, checked: Boolean, enabled: Boolean = true, change: (Boolean) -> Unit) {
    SettingToggle(title, checked, change, enabled, modifier = Modifier.testTag(title))
}
internal fun noticeCategoryTitle(category: String) = when (category) {
    "security" -> str(R.string.text_account_security_41c2886)
    "action" -> str(R.string.text_approvals_and_tasks_9ea7041)
    "message" -> str(R.string.text_messages_3a9ab2d)
    else -> str(R.string.text_service_updates_d5010aa)
}

@Composable fun NotificationsScreen(client: ClientController) {
    val preferences by client.notifications.preferences.state.collectAsStateWithLifecycle()
    val draft by client.notifications.draft.collectAsStateWithLifecycle()
    val failure by client.notifications.saveError.collectAsStateWithLifecycle()
    val test by client.notices.testResult.collectAsStateWithLifecycle()
    val push by client.push.status.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val permission = LocalNoticePermission.current
    var confirmingAccount by remember { mutableStateOf(false) }
    var confirmingRetry by remember { mutableStateOf(false) }
    LaunchedEffect(state.epoch) { client.notifications.load() }
    LazyColumn(Modifier.fillMaxSize().testTag("notification-settings"), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Text(str(R.string.text_notification_delivery_937a3db), modifier = Modifier.openedReminderVisible(client, preferences is ReadState.Ready) { it.kind == "service" && it.id?.startsWith("registration:") == true }, style = MaterialTheme.typography.titleMedium)
            ReadFeedback(preferences, client.notifications::load)
            if (!NotificationManagerCompat.from(context).areNotificationsEnabled()) OutlinedButton(onClick = permission) { Text(str(R.string.text_allow_system_notifications_2b06bbf)) }
            noticeCategories.forEach { category ->
                TextButton(onClick = { client.beginExternalInteraction(); context.startActivity(client.notices.system.settings(category)) }) { Text(str(R.string.text_android_settings_33f5c54) + noticeCategoryTitle(category)) }
            }
            TextButton(onClick = client.notices::test) { Text(str(R.string.notice_test)) }
            test?.let { result -> Text(when (result) {
                net.dmsg.android.platform.SystemNotices.Result.SUBMITTED -> str(R.string.text_test_notification_submitted_to_android_2ea1fbf)
                net.dmsg.android.platform.SystemNotices.Result.SUPPRESSED -> str(R.string.text_allow_notifications_and_the_service_category_in_android_settings_613b226)
                net.dmsg.android.platform.SystemNotices.Result.FAILED -> str(R.string.text_the_test_notification_could_not_be_submitted_try_again_2f69e15)
            }) }
            LabelValue(str(R.string.text_system_permission_ecf1e5e), if (NotificationManagerCompat.from(context).areNotificationsEnabled()) str(R.string.text_allowed_888622f) else str(R.string.text_not_allowed_48efbe6))
            OutlinedButton(onClick = { client.beginExternalInteraction(); context.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)) }) { Text(str(R.string.text_open_notification_settings_aa6d826)) }
            Notice(when (push) {
                net.dmsg.android.core.PushCoordinator.Status.UNCONFIGURED -> str(R.string.text_background_delivery_is_unavailable_in_this_build_opening_dmsg_ref_c768a1b)
                net.dmsg.android.core.PushCoordinator.Status.CONSENT -> str(R.string.text_enable_account_reminders_below_then_register_this_device_for_back_d2141d8)
                net.dmsg.android.core.PushCoordinator.Status.PERMISSION -> str(R.string.text_allow_notifications_in_android_settings_before_registering_c3ab5c2)
                net.dmsg.android.core.PushCoordinator.Status.LOCKED -> str(R.string.text_unlock_dmsg_to_renew_this_device_registration_524a0c3)
                net.dmsg.android.core.PushCoordinator.Status.INACTIVE -> str(R.string.text_background_delivery_is_paused_for_this_account_811c146)
                net.dmsg.android.core.PushCoordinator.Status.READY -> str(R.string.text_device_registered_delivery_still_follows_your_preferences_and_and_545ba62)
                net.dmsg.android.core.PushCoordinator.Status.WORKING -> str(R.string.text_verifying_this_installation_3fd71b5)
                net.dmsg.android.core.PushCoordinator.Status.FAILED -> str(R.string.text_registration_was_not_confirmed_check_the_connection_and_retry_6a52481)
            })
            if (client.push.configured) OutlinedButton(onClick = client.push::enable, enabled = state.unlocked && push != net.dmsg.android.core.PushCoordinator.Status.WORKING && !state.working("notices:save")) { Text(str(R.string.text_register_renew_background_delivery_629efa3)) }
        }
        draft?.let { value ->
            item {
                if (state.working("notices:save")) LinearProgressIndicator(Modifier.fillMaxWidth())
                if (failure != null) {
                    Notice(errorText(failure!!), true)
                    TextButton(onClick = { confirmingRetry = true }, enabled = preferences is ReadState.Ready && !state.working("notices:save")) { Text(str(R.string.text_review_and_retry_saved_edits_3c4f724)) }
                }
                if (preferences.valueOrNull()?.conflict == true) Notice(str(R.string.text_notification_preferences_changed_elsewhere_review_your_choices_be_43a8495))
                if (preferences.valueOrNull()?.pending == true) Text(str(R.string.text_saved_locally_waiting_to_sync_preferences_9b29487), style = MaterialTheme.typography.bodySmall)
                SectionTitle(str(R.string.text_this_installation_8ecc58c))
                NoticeSwitch(str(R.string.text_enable_notifications_01ecccc), value.global.enabled) { client.notifications.edit(global = value.global.copy(enabled = it)) }
                Text(str(R.string.text_each_account_needs_its_own_consent_000863e), style = MaterialTheme.typography.bodySmall)
                Text(str(R.string.text_launcher_badges_38c041f), style = MaterialTheme.typography.titleSmall)
                Text(str(R.string.text_android_and_your_launcher_control_notification_dots_and_counts_ch_78649e5), style = MaterialTheme.typography.bodySmall)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = { client.notifications.edit(global = value.global.copy(pausedUntil = System.currentTimeMillis().toULong() + 900_000u)) }) { Text(str(R.string.text_pause_15_min_157680c)) }
                    TextButton(onClick = { client.notifications.edit(global = value.global.copy(pausedUntil = System.currentTimeMillis().toULong() + 3_600_000u)) }) { Text(str(R.string.text_pause_1_hour_f49d294)) }
                    TextButton(onClick = { client.notifications.edit(global = value.global.copy(pausedUntil = ZonedDateTime.now().toLocalDate().plusDays(1).atTime(8, 0).atZone(ZoneId.systemDefault()).toInstant().toEpochMilli().toULong())) }) { Text(str(R.string.text_until_tomorrow_ff2f949)) }
                }
                value.global.pausedUntil?.takeIf { it > System.currentTimeMillis().toULong() }?.let {
                    LabelValue(str(R.string.text_paused_until_fb01e59), displayTime(it))
                    TextButton(onClick = { client.notifications.edit(global = value.global.copy(pausedUntil = null)) }) { Text(str(R.string.text_resume_notifications_7a17d27)) }
                }
            }
            item {
                SectionTitle(str(R.string.text_quiet_hours_c4c8f63))
                NoticeSwitch(str(R.string.text_daily_quiet_hours_7fe22b0), value.global.quiet != null) { client.notifications.edit(global = value.global.copy(quiet = if (it) NoticeQuietHours(22 * 60, 8 * 60, ZoneId.systemDefault().id) else null)) }
                value.global.quiet?.let { quiet ->
                    fun pickTime(start: Boolean) {
                        val minutes = if (start) quiet.start else quiet.end
                        TimePickerDialog(context, { _, hour, minute ->
                            val next = if (start) quiet.copy(start = hour * 60 + minute) else quiet.copy(end = hour * 60 + minute)
                            client.notifications.edit(global = value.global.copy(quiet = next))
                        }, minutes / 60, minutes % 60, android.text.format.DateFormat.is24HourFormat(context)).show()
                    }
                    AccountRow(str(R.string.text_from_6fe638c) + minuteTime(quiet.start)) { pickTime(true) }
                    AccountRow(str(R.string.text_until_186ebd7) + minuteTime(quiet.end)) { pickTime(false) }
                    AccountRow(quiet.timezone) { client.navigation.push(Route("notification-timezone")) }
                    if (!quiet.valid) Notice(str(R.string.text_choose_different_start_and_end_times_4c03d76), true)
                    NoticeSwitch(str(R.string.text_allow_security_alerts_during_quiet_hours_fe394b7), value.global.securityException) { client.notifications.edit(global = value.global.copy(securityException = it)) }
                    Text(str(R.string.text_this_never_overrides_the_system_s_notification_controls_cab21e8), style = MaterialTheme.typography.bodySmall)
                }
            }
            value.account?.let { account ->
                item {
                    SectionTitle(str(R.string.text_this_account_24d133a))
                    Text(value.accountId.orEmpty(), style = MaterialTheme.typography.bodySmall)
                    NoticeSwitch(str(R.string.text_notify_for_this_account_f989be2), account.enabled) { enabled ->
                        if (enabled) confirmingAccount = true else client.notifications.edit(account = account.copy(enabled = false))
                    }
                    NoticeSwitch(str(R.string.text_coordinate_with_other_devices_c33e88f), account.mergeDevices) { client.notifications.edit(account = account.copy(mergeDevices = it)) }
                }
                items(noticeCategories, key = { it }) { category ->
                    val selected = account.categories.getValue(category)
                    Card {
                        Column {
                            NoticeSwitch(noticeCategoryTitle(category), selected.enabled) { enabled -> client.notifications.edit(account = account.copy(categories = account.categories + (category to selected.copy(enabled = enabled)))) }
                            NoticeSwitch(str(R.string.text_sound_29e55cd) + " · " + noticeCategoryTitle(category), selected.sound) { sound -> client.notifications.edit(account = account.copy(categories = account.categories + (category to selected.copy(sound = sound)))) }
                        }
                    }
                }
            }
        }
        item { AccountRow(str(R.string.text_reminders_e43c7b0)) { client.navigation.push(Route("reminders")) } }
    }
    if (confirmingAccount) AlertDialog(onDismissRequest = { confirmingAccount = false }, title = { Text(str(R.string.text_enable_this_account_s_reminders_5429cc5)) }, text = {
        Text(str(R.string.text_this_is_a_separate_choice_for_this_account_when_remote_delivery_i_76004c5))
    }, confirmButton = { TextButton(onClick = { confirmingAccount = false; draft?.account?.let { client.notifications.edit(account = it.copy(enabled = true)) } }) { Text(str(R.string.text_enable_for_this_account_85ff77f)) } }, dismissButton = { TextButton(onClick = { confirmingAccount = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (confirmingRetry) AlertDialog(onDismissRequest = { confirmingRetry = false }, title = { Text(str(R.string.text_apply_your_reviewed_preferences_4d34d52)) }, text = { Text(str(R.string.text_your_selections_above_will_be_saved_against_the_latest_loaded_rev_b1c5c6b)) }, confirmButton = { TextButton(onClick = { confirmingRetry = false; client.notifications.retrySave() }) { Text(str(R.string.text_apply_3d57c12)) } }, dismissButton = { TextButton(onClick = { confirmingRetry = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
}
private fun minuteTime(minutes: Int) = (minutes / 60).toString().padStart(2, '0') + ":" + (minutes % 60).toString().padStart(2, '0')

@Composable fun NotificationTimezoneScreen(client: ClientController) {
    val draft by client.notifications.draft.collectAsStateWithLifecycle()
    var query by remember { mutableStateOf("") }
    val zones = remember { ZoneId.getAvailableZoneIds().sorted() }
    Column(Modifier.fillMaxSize().padding(20.dp)) {
        OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth(), label = { Text(str(R.string.text_search_time_zones_e44910f)) }, singleLine = true)
        LazyColumn {
            items(zones.filter { it.contains(query, ignoreCase = true) }, key = { it }) { zone ->
                ListItem(headlineContent = { Text(zone) }, trailingContent = { if (draft?.global?.quiet?.timezone == zone) Text(str(R.string.text_selected_fea261f)) },
                    modifier = Modifier.clickable { draft?.global?.let { global -> global.quiet?.let { client.notifications.edit(global = global.copy(quiet = it.copy(timezone = zone))) } }; client.navigation.back() })
            }
        }
    }
}

@Composable fun RemindersScreen(client: ClientController) {
    val reminders by client.notifications.reminders.state.collectAsStateWithLifecycle()
    val recent by client.notifications.recent.collectAsStateWithLifecycle()
    val status by client.notifications.status.state.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    var markAll by remember { mutableStateOf(false) }
    var clearing by remember { mutableStateOf(false) }
    var snoozing by remember { mutableStateOf<ReminderItem?>(null) }
    LaunchedEffect(state.epoch) { client.notifications.list() }
    LazyColumn(Modifier.fillMaxSize().testTag("reminders-list"), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Text(state.session?.account.orEmpty(), style = MaterialTheme.typography.labelMedium)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(!recent, { client.notifications.selectRecent(false) }, label = { Text(str(R.string.text_needs_action_0368a31)) })
                FilterChip(recent, { client.notifications.selectRecent(true) }, label = { Text(str(R.string.text_recent_3afe2bb)) })
                TextButton(onClick = client.notifications::refresh, enabled = !state.working("notices:refresh")) { Text(str(R.string.text_refresh_cba212b)) }
            }
            ReadFeedback(reminders, { client.notifications.list() })
            if (status.valueOrNull()?.counts?.unverified == true) Notice(str(R.string.text_some_updates_still_need_verification_the_last_known_count_is_reta_45abf79))
        }
        items(reminders.valueOrNull()?.items.orEmpty(), key = { it.id }) { item ->
            Card(Modifier.fillMaxWidth().testTag("reminder-" + item.id)) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(reminderTitle(item.target), style = MaterialTheme.typography.titleMedium)
                    Text(if (item.isUnread) str(R.string.text_unread_reminder_dea8c23) else str(R.string.text_read_e0ed4e0), style = MaterialTheme.typography.labelMedium)
                    Text(displayTime(item.updatedAt), style = MaterialTheme.typography.bodySmall)
                    item.deadline?.let { Text(str(R.string.text_due_051e317) + displayTime(it)) }
                    item.snoozedUntil?.takeIf { it > System.currentTimeMillis().toULong() }?.let { Text(str(R.string.text_remind_after_ee1bbf6) + displayTime(it)) }
                    if (item.unverified) Text(str(R.string.text_updates_need_verification_4be083a))
                    else item.unread?.takeIf { it > 0u }?.let { Text(str(R.string.text_last_verified_unread_count_9ef32ef) + localizedNumber(it)) }
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(onClick = { client.notifications.open(item) }) { Text(str(R.string.text_open_item_37482aa)) }
                        if (item.isUnread) TextButton(onClick = { client.notifications.markRead(item) }, enabled = !state.working("notices:read:" + item.id)) { Text(str(R.string.text_mark_reminder_read_004f9f9)) }
                        if (item.open && item.actionable) TextButton(onClick = { snoozing = item }) { Text(str(R.string.text_remind_later_e978e2b)) }
                    }
                }
            }
        }
        if (reminders is ReadState.Empty) item { EmptyPanel(if (recent) str(R.string.text_no_reminders_yet_13d6558) else str(R.string.text_nothing_needs_your_attention_e946421)) }
        if (reminders.valueOrNull()?.next != null) item { OutlinedButton(onClick = { client.notifications.list(true) }, enabled = reminders !is ReadState.Loading) { Text(str(R.string.text_load_more_4e1d9f8)) } }
        item {
            HorizontalDivider()
            AccountRow(str(R.string.text_notification_preferences_d625ba4)) { client.navigation.push(Route("notifications")) }
            TextButton(onClick = { markAll = true }) { Text(str(R.string.text_mark_reminders_for_all_accounts_as_read_681d591)) }
            TextButton(onClick = { clearing = true }) { Text(str(R.string.text_clear_recent_history_d9e41d3)) }
        }
    }
    if (markAll) AlertDialog(onDismissRequest = { markAll = false }, title = { Text(str(R.string.text_mark_all_reminders_read_9114c6d)) }, text = { Text(str(R.string.text_this_affects_reminders_on_this_device_it_does_not_read_messages_a_3ad1bd5)) }, confirmButton = { TextButton(onClick = { markAll = false; client.notifications.markAllRead() }) { Text(str(R.string.text_mark_as_read_008b048)) } }, dismissButton = { TextButton(onClick = { markAll = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    if (clearing) AlertDialog(onDismissRequest = { clearing = false }, title = { Text(str(R.string.text_clear_ended_reminder_history_9d42924)) }, text = { Text(str(R.string.text_open_tasks_and_business_records_remain_605e91e)) }, confirmButton = { TextButton(onClick = { clearing = false; client.notifications.clearRecent() }) { Text(str(R.string.text_clear_a390bf3)) } }, dismissButton = { TextButton(onClick = { clearing = false }) { Text(str(R.string.text_cancel_998b9c4)) } })
    snoozing?.let { item ->
        AlertDialog(onDismissRequest = { snoozing = null }, title = { Text(str(R.string.text_remind_later_e978e2b)) }, text = {
            Column {
                Text(str(R.string.text_reminders_never_extend_a_request_s_deadline_b7b27e2))
                listOf(900_000uL to str(R.string.text_in_15_minutes_b066bbd), 3_600_000uL to str(R.string.text_in_1_hour_f1619d5)).forEach { (delay, label) ->
                    val until = System.currentTimeMillis().toULong() + delay
                    TextButton(onClick = { snoozing = null; client.notifications.snooze(item, until) }, enabled = item.deadline?.let { until < it } != false) { Text(label) }
                }
            }
        }, confirmButton = {}, dismissButton = { TextButton(onClick = { snoozing = null }) { Text(str(R.string.text_cancel_998b9c4)) } })
    }
}
internal fun reminderTitle(target: NoticeTarget) = when (target.kind) {
    "plan" -> str(R.string.text_plan_and_usage_acf1a9a)
    "task" -> if (target.id?.startsWith("conflicts:") == true) str(R.string.text_sync_conflicts_c35724e) else str(R.string.text_sync_needs_attention_19e8287)
    "service" -> if (target.id?.startsWith("network:") == true) str(R.string.text_connection_needs_attention_41c07bb) else str(R.string.text_notification_connection_1adea1d)
    else -> noticeCategoryTitle(target.category)
}
