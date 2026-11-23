package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import android.app.TimePickerDialog
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LocalLifecycleOwner
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
    val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateFlow.collectAsState()
    val focused = LocalWindowInfo.current.isWindowFocused
    val allowed = client.notices.system.allowed()
    val blockedChannels = remember(lifecycle, focused, allowed) { noticeCategories.filter(client.notices.system::channelBlocked) }
    fun openSystem(category: String? = null) {
        client.beginExternalInteraction()
        context.startActivity(client.notices.system.settings(category))
    }
    var advanced by remember { mutableStateOf(false) }
    var confirmingAccount by remember { mutableStateOf(false) }
    var confirmingRetry by remember { mutableStateOf(false) }
    LaunchedEffect(state.epoch) { client.notifications.load() }
    LazyColumn(
        Modifier.fillMaxSize().testTag("notification-settings"),
        contentPadding = pagePadding(),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                val currentFailure = failure
                val preferenceProblem = when {
                    currentFailure != null -> errorText(currentFailure) to true
                    preferences.valueOrNull()?.conflict == true -> str(R.string.text_notification_preferences_changed_elsewhere_review_your_choices_be_43a8495) to false
                    else -> null
                }
                val delivery = str(when (push) {
                    PushCoordinator.Status.UNCONFIGURED -> R.string.text_background_delivery_is_unavailable_in_this_build_opening_dmsg_ref_c768a1b
                    PushCoordinator.Status.CONSENT -> R.string.text_enable_account_reminders_below_then_register_this_device_for_back_d2141d8
                    PushCoordinator.Status.PERMISSION -> R.string.text_allow_notifications_in_android_settings_before_registering_c3ab5c2
                    PushCoordinator.Status.LOCKED -> R.string.text_unlock_dmsg_to_renew_this_device_registration_524a0c3
                    PushCoordinator.Status.INACTIVE -> R.string.text_background_delivery_is_paused_for_this_account_811c146
                    PushCoordinator.Status.READY -> R.string.text_device_registered_delivery_still_follows_your_preferences_and_and_545ba62
                    PushCoordinator.Status.WORKING -> R.string.text_verifying_this_installation_3fd71b5
                    PushCoordinator.Status.FAILED -> R.string.text_registration_was_not_confirmed_check_the_connection_and_retry_6a52481
                })
                val deliveryFailed = push == PushCoordinator.Status.FAILED
                preferenceProblem?.let { (message, warning) -> Notice(message, warning) }
                Box(Modifier.openedReminderVisible(client, preferences is ReadState.Ready) {
                    it.kind == "service" && it.id?.startsWith("registration:") == true
                }) {
                    if (preferenceProblem == null) Notice(delivery, deliveryFailed)
                    else Text(delivery, style = MaterialTheme.typography.bodyMedium,
                        color = if (deliveryFailed) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (currentFailure != null) {
                    OutlinedButton(onClick = { confirmingRetry = true }, enabled = preferences is ReadState.Ready && !state.working("notices:save"), modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_review_and_retry_saved_edits_3c4f724))
                    }
                }
                ReadFeedback(preferences, client.notifications::load)
                Section(title = str(R.string.text_notification_delivery_937a3db)) {
                    ListRow(str(R.string.text_system_permission_ecf1e5e), icon = Icons.Outlined.NotificationsActive,
                        value = if (allowed) str(R.string.text_allowed_888622f) else str(R.string.text_not_allowed_48efbe6))
                    ListRow(str(R.string.text_open_notification_settings_aa6d826), icon = Icons.Outlined.Settings) { openSystem() }
                    if (allowed) blockedChannels.forEach { category ->
                        ListRow(
                            headline = noticeCategoryTitle(category),
                            supporting = str(R.string.text_system_permission_ecf1e5e) + " · " + str(R.string.text_not_allowed_48efbe6),
                            icon = Icons.Outlined.NotificationsOff,
                        ) { openSystem(category) }
                    }
                }
                if (!allowed && android.os.Build.VERSION.SDK_INT >= 33) {
                    Button(onClick = permission, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_allow_system_notifications_2b06bbf)) }
                }
                if (client.push.configured) {
                    OutlinedButton(onClick = client.push::enable, enabled = state.unlocked && push != PushCoordinator.Status.WORKING && !state.working("notices:save"), modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.text_register_renew_background_delivery_629efa3))
                    }
                }
            }
        }
        draft?.let { value ->
            item {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (state.working("notices:save")) LinearProgressIndicator(Modifier.fillMaxWidth())
                    if (preferences.valueOrNull()?.pending == true) {
                        Text(str(R.string.text_saved_locally_waiting_to_sync_preferences_9b29487),
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Section(
                        title = str(R.string.text_this_installation_8ecc58c),
                        footer = str(R.string.text_each_account_needs_its_own_consent_000863e) + " " + str(R.string.text_android_and_your_launcher_control_notification_dots_and_counts_ch_78649e5),
                    ) {
                        NoticeSwitch(str(R.string.text_enable_notifications_01ecccc), value.global.enabled) { client.notifications.edit(global = value.global.copy(enabled = it)) }
                        ListItem(
                            headlineContent = {
                                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                    AssistChip(onClick = { client.notifications.edit(global = value.global.copy(pausedUntil = System.currentTimeMillis().toULong() + 900_000u)) }, label = { Text(str(R.string.text_pause_15_min_157680c)) })
                                    AssistChip(onClick = { client.notifications.edit(global = value.global.copy(pausedUntil = System.currentTimeMillis().toULong() + 3_600_000u)) }, label = { Text(str(R.string.text_pause_1_hour_f49d294)) })
                                    AssistChip(onClick = { client.notifications.edit(global = value.global.copy(pausedUntil = ZonedDateTime.now().toLocalDate().plusDays(1).atTime(8, 0).atZone(ZoneId.systemDefault()).toInstant().toEpochMilli().toULong())) }, label = { Text(str(R.string.text_until_tomorrow_ff2f949)) })
                                }
                            },
                        )
                        value.global.pausedUntil?.takeIf { it > System.currentTimeMillis().toULong() }?.let {
                            ListRow(str(R.string.text_paused_until_fb01e59), value = displayTime(it))
                            ListRow(str(R.string.text_resume_notifications_7a17d27), icon = Icons.Outlined.NotificationsActive) {
                                client.notifications.edit(global = value.global.copy(pausedUntil = null))
                            }
                        }
                    }
                }
            }
            item {
                Section(
                    title = str(R.string.text_quiet_hours_c4c8f63),
                    footer = if (value.global.quiet != null) str(R.string.text_this_never_overrides_the_system_s_notification_controls_cab21e8) else null,
                ) {
                    NoticeSwitch(str(R.string.text_daily_quiet_hours_7fe22b0), value.global.quiet != null) {
                        client.notifications.edit(global = value.global.copy(quiet = if (it) NoticeQuietHours(22 * 60, 8 * 60, ZoneId.systemDefault().id) else null))
                    }
                    value.global.quiet?.let { quiet ->
                        fun pickTime(start: Boolean) {
                            val minutes = if (start) quiet.start else quiet.end
                            TimePickerDialog(context, { _, hour, minute ->
                                val next = if (start) quiet.copy(start = hour * 60 + minute) else quiet.copy(end = hour * 60 + minute)
                                client.notifications.edit(global = value.global.copy(quiet = next))
                            }, minutes / 60, minutes % 60, android.text.format.DateFormat.is24HourFormat(context)).show()
                        }
                        ListRow(str(R.string.text_from_6fe638c).trimEnd(' ', '·', ':'), value = minuteTime(quiet.start), icon = Icons.Outlined.Bedtime) { pickTime(true) }
                        ListRow(str(R.string.text_until_186ebd7).trimEnd(' ', '·', ':'), value = minuteTime(quiet.end), icon = Icons.Outlined.WbSunny) { pickTime(false) }
                        ListRow(quiet.timezone, icon = Icons.Outlined.Public) { client.navigation.push(Route("notification-timezone")) }
                        if (!quiet.valid) {
                            ListItem(headlineContent = { Text(str(R.string.text_choose_different_start_and_end_times_4c03d76), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) })
                        }
                        NoticeSwitch(str(R.string.text_allow_security_alerts_during_quiet_hours_fe394b7), value.global.securityException) {
                            client.notifications.edit(global = value.global.copy(securityException = it))
                        }
                    }
                }
            }
            value.account?.let { account ->
                item {
                    Section(title = str(R.string.text_this_account_24d133a)) {
                        value.accountId?.takeIf(String::isNotBlank)?.let { CodeRow(str(R.string.text_account_id_43f0a66), it) }
                        NoticeSwitch(str(R.string.text_notify_for_this_account_f989be2), account.enabled) { enabled ->
                            if (enabled) confirmingAccount = true else client.notifications.edit(account = account.copy(enabled = false))
                        }
                        if (advanced) NoticeSwitch(str(R.string.text_coordinate_with_other_devices_c33e88f), account.mergeDevices) {
                            client.notifications.edit(account = account.copy(mergeDevices = it))
                        }
                    }
                }
                items(noticeCategories, key = { it }) { category ->
                    val selected = account.categories.getValue(category)
                    Section {
                        NoticeSwitch(noticeCategoryTitle(category), selected.enabled) { enabled ->
                            client.notifications.edit(account = account.copy(categories = account.categories + (category to selected.copy(enabled = enabled))))
                        }
                        if (advanced) NoticeSwitch(str(R.string.text_sound_29e55cd) + " · " + noticeCategoryTitle(category), selected.sound) { sound ->
                            client.notifications.edit(account = account.copy(categories = account.categories + (category to selected.copy(sound = sound))))
                        }
                    }
                }
            }
        }
        item {
            Section {
                SettingToggle(str(R.string.text_advanced), advanced, { advanced = it }, icon = Icons.Outlined.Tune)
                if (advanced) {
                    noticeCategories.filterNot { allowed && it in blockedChannels }.forEach { category ->
                        ListRow(str(R.string.text_android_settings_33f5c54) + noticeCategoryTitle(category), icon = Icons.Outlined.Settings) { openSystem(category) }
                    }
                    ListRow(
                        headline = str(R.string.notice_test),
                        supporting = test?.let { result ->
                            when (result) {
                                net.dmsg.android.platform.SystemNotices.Result.SUBMITTED -> str(R.string.text_test_notification_submitted_to_android_2ea1fbf)
                                net.dmsg.android.platform.SystemNotices.Result.SUPPRESSED -> str(R.string.text_allow_notifications_and_the_service_category_in_android_settings_613b226)
                                net.dmsg.android.platform.SystemNotices.Result.FAILED -> str(R.string.text_the_test_notification_could_not_be_submitted_try_again_2f69e15)
                            }
                        },
                        icon = Icons.Outlined.NotificationsNone,
                    ) { client.notices.test() }
                }
            }
        }
        item {
            Section {
                ListRow(str(R.string.text_reminders_e43c7b0), icon = Icons.Outlined.NotificationsNone) { client.navigation.push(Route("reminders")) }
            }
        }
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
    val locale = net.dmsg.android.platform.AppLanguage.locale()
    val current = draft?.global?.quiet?.timezone
    val zones = remember(locale, current) {
        val names = android.icu.text.TimeZoneNames.getInstance(locale)
        val ids = java.util.TimeZone.getAvailableIDs().mapNotNull { android.icu.util.TimeZone.getCanonicalID(it) }
            .filter { '/' in it || it == "UTC" }.toMutableSet()
        current?.let(ids::add)
        val sorter = java.text.Collator.getInstance(locale)
        ids.map { id ->
            val name = names.getExemplarLocationName(id) ?: id.substringAfterLast('/').replace('_', ' ')
            val zone = java.util.TimeZone.getTimeZone(id)
            Triple(id, name, zone.getDisplayName(false, java.util.TimeZone.LONG, locale))
        }.sortedWith(compareBy<Triple<String, String, String>> { it.first != current }.thenComparator { a, b -> sorter.compare(a.second, b.second) })
    }
    Column(Modifier.fillMaxSize().padding(horizontal = pageMargin()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        WorkspaceSearchField(query, { query = it }, str(R.string.text_search_time_zones_e44910f), Modifier.padding(top = 8.dp))
        LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(bottom = pageMargin()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            val visible = zones.filter { (id, city, region) -> listOf(id, city, region).any { it.contains(query.trim(), ignoreCase = true) } }
            if (visible.isEmpty()) item { EmptyState(str(R.string.no_matches), icon = Icons.Outlined.Public) }
            items(visible, key = { it.first }) { (id, city, region) ->
                Section {
                    ListItem(
                        headlineContent = { Text(city) },
                        supportingContent = { Text(region + " · " + id) },
                        trailingContent = {
                            if (current == id) Icon(Icons.Outlined.Check, str(R.string.text_selected_fea261f), tint = MaterialTheme.colorScheme.primary)
                        },
                        modifier = Modifier.clickable {
                            draft?.global?.let { global -> global.quiet?.let { client.notifications.edit(global = global.copy(quiet = it.copy(timezone = id))) } }
                            client.navigation.back()
                        },
                    )
                }
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
    ScreenBar(null) {
        IconButton(onClick = client.notifications::refresh, enabled = !state.working("notices:refresh")) {
            Icon(Icons.Outlined.Refresh, str(R.string.text_refresh_cba212b))
        }
    }
    LazyColumn(Modifier.fillMaxSize().testTag("reminders-list"), contentPadding = pagePadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                state.session?.account?.takeIf(String::isNotBlank)?.let {
                    Text(it, style = MaterialTheme.typography.bodySmall.code(), color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                SegmentedChoice(listOf(str(R.string.text_needs_action_0368a31), str(R.string.text_recent_3afe2bb)), if (recent) 1 else 0) {
                    client.notifications.selectRecent(it == 1)
                }
                ReadFeedback(reminders, { client.notifications.list() })
                if (status.valueOrNull()?.counts?.unverified == true) Notice(str(R.string.text_some_updates_still_need_verification_the_last_known_count_is_reta_45abf79))
            }
        }
        items(reminders.valueOrNull()?.items.orEmpty(), key = { it.id }) { item ->
            DetailCard(Modifier.testTag("reminder-" + item.id)) {
                Text(reminderTitle(item.target), style = MaterialTheme.typography.titleMedium)
                Text(
                    (if (item.isUnread) str(R.string.text_unread_reminder_dea8c23) else str(R.string.text_read_e0ed4e0)) + " · " + displayTime(item.updatedAt),
                    style = MaterialTheme.typography.bodySmall,
                    color = if (item.isUnread) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                )
                item.deadline?.let { Text(str(R.string.text_due_051e317) + displayTime(it), style = MaterialTheme.typography.bodySmall) }
                item.snoozedUntil?.takeIf { it > System.currentTimeMillis().toULong() }?.let { Text(str(R.string.text_remind_after_ee1bbf6) + displayTime(it), style = MaterialTheme.typography.bodySmall) }
                if (item.unverified) Text(str(R.string.text_updates_need_verification_4be083a), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                else item.unread?.takeIf { it > 0u }?.let { Text(str(R.string.text_last_verified_unread_count_9ef32ef) + localizedNumber(it), style = MaterialTheme.typography.bodySmall) }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = { client.notifications.open(item) }) { Text(str(R.string.text_open_item_37482aa)) }
                    if (item.isUnread) TextButton(onClick = { client.notifications.markRead(item) }, enabled = !state.working("notices:read:" + item.id)) { Text(str(R.string.text_mark_reminder_read_004f9f9)) }
                    if (item.open && item.actionable) TextButton(onClick = { snoozing = item }) { Text(str(R.string.text_remind_later_e978e2b)) }
                }
            }
        }
        if (reminders is ReadState.Empty) item {
            EmptyState(if (recent) str(R.string.text_no_reminders_yet_13d6558) else str(R.string.text_nothing_needs_your_attention_e946421), icon = Icons.Outlined.NotificationsNone)
        }
        if (reminders.valueOrNull()?.next != null) item {
            OutlinedButton(onClick = { client.notifications.list(true) }, enabled = reminders !is ReadState.Loading, modifier = Modifier.fillMaxWidth()) {
                Text(str(R.string.text_load_more_4e1d9f8))
            }
        }
        item {
            Section {
                ListRow(str(R.string.text_notification_preferences_d625ba4), icon = Icons.Outlined.Tune) { client.navigation.push(Route("notifications")) }
                ListRow(str(R.string.text_mark_reminders_for_all_accounts_as_read_681d591), icon = Icons.Outlined.DoneAll) { markAll = true }
                ListRow(str(R.string.text_clear_recent_history_d9e41d3), icon = Icons.Outlined.CleaningServices) { clearing = true }
            }
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
