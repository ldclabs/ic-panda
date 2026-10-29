//! Owner-only notification queries and one deadline/event wakeup loop.
use crate::{
    daemon::Daemon,
    params::{account, number, parse, text},
};
use dmsg_client::{
    hold::Hold,
    notifications::{self as core, Runtime, Target, policy::Context},
    now_ms,
};
use dmsg_types::{AccountId, Error, Result, ensure};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;

pub(crate) const METHODS: [&str; 11] = [
    "notifications/migrate",
    "notifications/list",
    "notifications/status",
    "notifications/settings",
    "notifications/settings_set",
    "notifications/observed",
    "notifications/snooze",
    "notifications/clear_recent",
    "notifications/context",
    "notifications/next",
    "notifications/result",
];

#[derive(Default)]
pub(crate) struct Notifications {
    pub(crate) wake: Notify,
    pub(crate) inbox: Notify,
    pub(crate) receive: Notify,
    scheduled: Notify,
    deadline: AtomicU64,
    context: Mutex<Option<(u64, Context)>>,
}

/// Never consumes a plan. A headless daemon only keeps state and emits invalidations.
pub(crate) async fn run(d: Arc<Daemon>) {
    loop {
        let at = d.notifications.deadline.load(Ordering::Relaxed);
        let wait = if at == 0 {
            Duration::from_secs(86_400)
        } else {
            Duration::from_millis(at.saturating_sub(now_ms()))
        };
        tokio::select! {
            _ = d.notifications.scheduled.notified() => { continue; },
            _ = d.notifications.wake.notified() => {},
            _ = tokio::time::sleep(wait) => { d.notifications.deadline.store(0, Ordering::Relaxed); }
        }
        let health = d.hold().with(|c| c.notification_health(now_ms())).await;
        if let Ok(Some(at)) = health {
            d.notifications
                .deadline
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(if old == 0 { at } else { old.min(at) })
                })
                .ok();
        }
        d.emit("notifications/changed", Value::Null);
    }
}

/// Ordinary inbox synchronization shares its verifier with foreground reads.
pub(crate) async fn inbox_run(d: Arc<Daemon>) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut seen = None;
    loop {
        tokio::select! { _ = tick.tick() => {}, _ = d.notifications.inbox.notified() => {} }
        if d.stopping() {
            continue;
        }
        let h = d.hold();
        let Ok(session) = h.with(|c| Ok(c.session_state())).await else {
            continue;
        };
        let Some(account) = session.unlocked_account().map(str::to_string) else {
            seen = None;
            continue;
        };
        let Some(_turn) = d.try_turn(&format!("inbox:{account}")) else {
            continue;
        };
        if seen.as_ref() != Some(&account) {
            let result = h
                .with(|c| {
                    let now = now_ms();
                    c.reconcile_notifications(now)?;
                    let path = c.agent_runtime()?;
                    for agent in c.agent_list()?["agents"].as_array().into_iter().flatten() {
                        if let Some(agent) = agent["agent_id"].as_str() {
                            let (watermark, unread) = d.agents().mail_notification(&path, agent)?;
                            c.project_agent_mail_notification(agent, watermark, unread, now)?;
                        }
                    }
                    Ok(())
                })
                .await;
            d.note_result("notifications/reconcile", &result);
            if result.is_ok() {
                seen = Some(account);
            }
            let result = core::refresh_capabilities(&h).await;
            d.note_result("notifications/capabilities", &result);
            d.notifications.receive.notify_one();
        }
        let result = dmsg_client::inbox::sync(&h, 10).await;
        d.note_result("notifications/inbox", &result);
        if let Ok(report) = result {
            if !report.changed.is_empty() {
                d.emit("inbox/changed", json!({"synced":true}));
            }
            if report.more {
                d.notifications.inbox.notify_one();
                tokio::task::yield_now().await;
            }
        }
    }
}

/// Service/wallet reads cannot delay the inbox loop. No content keys remain
/// across account locks; every network step is fenced by the existing Hold.
pub(crate) async fn services_run(d: Arc<Daemon>) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut account = None;
    let mut plan_at = 0;
    loop {
        tick.tick().await;
        if d.stopping() {
            continue;
        }
        let h = d.hold();
        let Ok(session) = h.with(|c| Ok(c.session_state())).await else {
            continue;
        };
        let Some(current) = session.unlocked_account() else {
            account = None;
            continue;
        };
        if account.as_deref() != Some(current) {
            account = Some(current.to_string());
            plan_at = 0;
        }
        let now = now_ms();
        if now.saturating_sub(plan_at) >= 3_600_000 {
            plan_at = now;
            let result = dmsg_client::commerce::plan_status(&h, false).await;
            d.note_result("notifications/service", &result);
            let _ = core::refresh_capabilities(&h).await;
        }
        let result = dmsg_client::wallet::refresh_notifications(&h).await;
        d.note_result("notifications/wallet", &result);
        d.notifications.wake.notify_one();
    }
}

/// One bounded metadata receive pass per account. No active-workspace switch,
/// background signing renewal or client lock is held over a network wait.
pub(crate) async fn receive_run(d: Arc<Daemon>) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {_ = tick.tick() => {}, _ = d.notifications.receive.notified() => {}}
        if d.stopping() {
            continue;
        }
        let h = d.hold();
        let Ok(session) = h.with(|c| Ok(c.session_state())).await else {
            continue;
        };
        if session.unlocked_account().is_some() {
            let registered = core::remote::register(&h, "en", None).await;
            if let Ok(Some(context)) = registered {
                let result = core::remote::sync_settings(&h, &context).await;
                d.note_result("notifications/settings", &result);
                let result = core::preferences::sync(&h, &context).await;
                d.note_result("notifications/resources", &result);
                let result = core::remote::sync_snoozes(&h, &context).await;
                d.note_result("notifications/snooze", &result);
                let result = core::remote::sync_recovery(&h, &context).await;
                d.note_result("notifications/recovery", &result);
                d.hints_changed().notify_one();
            }
        }
        let Ok(receivers) = h.with(|c| c.notification_receivers()).await else {
            continue;
        };
        for context in receivers {
            let Some(_turn) = d.try_turn(&format!("notifications:{}", context.account)) else {
                continue;
            };
            // Actual view receipts are sent before catch-up, within the original merge window.
            let _ = core::remote::send_observed(&h, &context).await;
            let result = core::remote::refresh(&h, &context).await;
            d.note_result("notifications/receive", &result);
            if result.is_ok()
                || matches!(result, Err(Error::AuthRequired | Error::DeviceNotApproved))
            {
                let needs_auth = result.is_err()
                    || (context.expires_at <= now_ms().saturating_add(86_400_000)
                        && session.unlocked_account() != Some(context.account.as_str()));
                let _ = h
                    .with(|c| c.notification_receiver_health(&context, needs_auth))
                    .await;
            }
            if result == Ok(true) {
                d.notifications.receive.notify_one();
            }
            if result.is_ok() {
                d.notifications.wake.notify_one();
            }
        }
    }
}

fn sequence(params: &Value, name: &str) -> Result<u64> {
    let value = text(params, name)?;
    let number = value
        .parse::<u64>()
        .map_err(|_| Error::InvalidInput(name.into()))?;
    ensure(
        number.to_string() == value,
        Error::InvalidInput(name.into()),
    )?;
    Ok(number)
}

impl Daemon {
    pub(crate) async fn call_notifications(&self, method: &str, params: &Value) -> Result<Value> {
        let h = self.hold();
        if method == "notifications/context" {
            let mut context: Context = parse(params, "context")?;
            ensure(context.visible.len() <= 32, Error::QuotaExceeded)?;
            let generation = h
                .with(|c| {
                    ensure(
                        params["session"] == self.session_token(c.session_generation()),
                        Error::Locked,
                    )?;
                    if c.session_state().state != "unlocked" {
                        context.visible.clear();
                    }
                    Ok(c.session_generation())
                })
                .await?;
            context.now = 0; // The host never chooses the policy clock.
            context.viewing = false;
            context.remote_viewed = false;
            let changed = {
                let mut previous = self
                    .notifications
                    .context
                    .lock()
                    .expect("notification context");
                let next = Some((generation, context));
                let changed = *previous != next;
                *previous = next;
                changed
            };
            if changed {
                self.notifications.wake.notify_one();
            }
            return Ok(Value::Null);
        }
        let result = h.with(|c| {
            let now = now_ms();
            let path = c.notification_runtime_path();
            let mut runtime = Runtime::load(&path, now)?;
            let selected = c.session_state().account;
            let account = match params.get("account").filter(|a| !a.is_null()) {
                Some(_) => Some(account(params, "account")?),
                None => selected.as_deref().map(str::parse::<AccountId>).transpose().map_err(|_| Error::IntegrityFailed)?,
            };
            match method {
                "notifications/migrate" => {
                    if !runtime.reminders_imported {
                        let plans: std::collections::BTreeMap<String, u8> = parse(params, "plans")?;
                        ensure(plans.len() <= 1000, Error::QuotaExceeded)?;
                        for (id, days) in plans {
                            ensure(dmsg_client::channel_log::is_hash(&id), Error::InvalidInput("contract".into()))?;
                            let stage = match days { 30 => 1, 7 => 2, 1 => 3, _ => return Err(Error::InvalidInput("reminder stage".into())) };
                            runtime.legacy_plans.insert(id, stage);
                        }
                        runtime.reminders_imported = true;
                        runtime.save(&path)?;
                    }
                    let mut claimed = std::collections::BTreeSet::new();
                    for local in c.accounts()?.into_iter().filter(|a| a.bound) {
                        let account = local.account.parse().map_err(|_| Error::IntegrityFailed)?;
                        c.with_notification_store(&account, |store| {
                            for mut item in store.notification_items()? {
                                if let Target::Plan(id) = &item.target && let Some(stage) = runtime.legacy_plans.get(id) {
                                    item.stage = item.stage.max(*stage);
                                    claimed.insert(id.clone());
                                    store.set_notification_item(&item)?;
                                }
                            }
                            Ok(())
                        })?;
                    }
                    for id in claimed { runtime.legacy_plans.remove(&id); }
                    runtime.save(&path)?;
                    Ok(json!({"imported":true,"awaiting_account":runtime.legacy_plans.len()}))
                }
                "notifications/status" => {
                    let mut total = core::Counts::default();
                    let mut accounts = Vec::new();
                    for local in c.accounts()?.into_iter().filter(|a| a.bound) {
                        let id: AccountId = local.account.parse().map_err(|_| Error::IntegrityFailed)?;
                        c.with_notification_store(&id, |store| {
                            let settings = core::settings(store)?;
                            let counts = core::counts(&store.notification_items()?, now);
                            if settings.enabled {
                                total.actionable = total.actionable.saturating_add(counts.actionable);
                                total.unread = total.unread.saturating_add(counts.unread);
                                total.unverified |= counts.unverified;
                            }
                            accounts.push(json!({"account":local.account,"enabled":settings.enabled,"counts":counts,
                                "pending_sync":settings.revision > settings.remote_revision}));
                            Ok(())
                        })?;
                    }
                    let badge = match runtime.settings.badge { core::Badge::All => total.actionable.saturating_add(total.unread), core::Badge::Actions => total.actionable, core::Badge::Off => 0 };
                    Ok(json!({"counts":total,"accounts":accounts,"badge":badge,"enabled":runtime.settings.enabled}))
                }
                "notifications/settings" => {
                    let settings = account.as_ref().map(|id| c.with_notification_store(id, core::settings)).transpose()?;
                    let remote_supported = account.as_ref().map(|id|c.notification_remote_supported(id)).transpose()?.unwrap_or(false);
                    let account_conflict = account.as_ref().map(|id|c.notification_settings_conflict(id)).transpose()?.unwrap_or(false);
                    Ok(json!({"global":runtime.settings,"account":settings,"account_id":account.map(|id| id.to_string()),"remote_supported":remote_supported,
                        "global_pending_sync":runtime.settings.revision > runtime.remote_revision,"global_conflict":runtime.remote_blocked == Some(runtime.settings.revision),"account_conflict":account_conflict}))
                }
                "notifications/settings_set" if params["scope"] == "installation" => {
                    runtime.configure(number(params, "expected_revision")?, parse(params, "settings")?)?;
                    runtime.save(&path)?;
                    Ok(json!({"settings":runtime.settings,"pending_sync":true}))
                }
                "notifications/next" => {
                    let context = self.notifications.context.lock().expect("notification context").clone();
                    let Some((generation, mut context)) = context.filter(|(generation, _)| *generation == c.session_generation()) else {
                        return Ok(json!({"plans":[],"remove":[],"next_due":null}));
                    };
                    let _ = generation;
                    context.now = now;
                    let mut all = core::Batch::default();
                    for local in c.accounts()?.into_iter().filter(|a| a.bound) {
                        let id: AccountId = local.account.parse().map_err(|_| Error::IntegrityFailed)?;
                        let batch = c.with_notification_store(&id, |store| {
                            if params["restore"] == true {
                                // Catch up quietly when a native owner returns. Never
                                // replay covered versions or reset security phases.
                                for mut item in store.notification_items()? {
                                    if item.open && item.category() != core::Category::Security && item.notified < item.version {
                                        item.quiet = true;
                                        item.pending_since = now;
                                        store.set_notification_item(&item)?;
                                    }
                                }
                            }
                            core::next(store, &mut runtime, &context, |runtime| runtime.save(&path))
                        })?;
                        if params["restore"] == true {
                            let previous = c.with_notification_store(&id, |store| store.notification_deliveries())?;
                            all.history.extend(previous.into_iter().filter(|plan| plan.result != core::DeliveryResult::Suppressed));
                        }
                        all.plans.extend(batch.plans);
                        all.remove.extend(batch.remove);
                        if let Some(at) = batch.next_due { all.next_due = Some(all.next_due.map_or(at, |previous| previous.min(at))); }
                    }
                    if let Some(at) = c.notification_health(now)? { all.next_due = Some(all.next_due.map_or(at, |previous| previous.min(at))); }
                    self.notifications.deadline.store(all.next_due.unwrap_or(0), Ordering::Relaxed);
                    self.notifications.scheduled.notify_one();
                    Ok(serde_json::to_value(all).map_err(|_| Error::IntegrityFailed)?)
                }
                _ => {
                    let account = account.ok_or(Error::AuthRequired)?;
                    if method == "notifications/observed" {
                        ensure(c.session_state().unlocked_account() == Some(account.to_string().as_str())
                            && params["session"] == self.session_token(c.session_generation()), Error::Locked)?;
                    }
                    c.with_notification_store(&account, |store| match method {
                        "notifications/settings_set" if params["scope"] == "account" => {
                            let settings = core::configure(store, number(params, "expected_revision")?, parse(params, "settings")?)?;
                            Ok(json!({"settings":settings,"pending_sync":true}))
                        }
                        "notifications/list" => {
                            if let Some(id) = params["id"].as_str() {
                                let item = store.notification_item(id)?;
                                return Ok(json!({"items":item.into_iter().collect::<Vec<_>>(),"next_cursor":null}));
                            }
                            let scope = params["scope"].as_str().unwrap_or("open");
                            ensure(matches!(scope, "open" | "recent"), Error::InvalidInput("scope".into()))?;
                            let prefix = format!("{account}:{scope}:");
                            let offset = match params["cursor"].as_str() {
                                Some(cursor) => cursor.strip_prefix(&prefix).and_then(|p| p.parse::<usize>().ok()).ok_or(Error::InvalidInput("cursor".into()))?,
                                None => 0,
                            };
                            let limit = params["limit"].as_u64().unwrap_or(50).clamp(1,100) as usize;
                            let page = core::list(store, scope == "open", offset, limit, now)?;
                            let cursor = page.next.map(|next| format!("{prefix}{next}"));
                            Ok(json!({"items":page.items,"next_cursor":cursor}))
                        }
                        "notifications/observed" => {
                            let target = params.get("target").map(|target| serde_json::from_value::<Target>(target.clone()).map_err(|_| Error::InvalidInput("target".into()))).transpose()?;
                            let id = match &target { Some(target) => core::item_id(&account.to_string(), target), None => text(params,"id")? };
                            let item = store.notification_item(&id)?.ok_or(Error::NotFound)?;
                            let version = if matches!(target, Some(Target::Channel(_))) && params.get("watermark").is_some() {
                                ensure(sequence(params,"watermark")? >= item.version, Error::VersionConflict)?;
                                item.version
                            } else if matches!(target, Some(Target::Inbox(Some(_)))) && params.get("version").is_none() {
                                // An accepted letter is immutable under its order ID.
                                item.version
                            } else { sequence(params,"version")? };
                            core::observe(store, &id, version)?;
                            Ok(json!({"id":id,"version":version.to_string(),"stage":item.stage}))
                        }
                        "notifications/snooze" => { core::snooze(store, &text(params,"id")?, sequence(params,"version")?, number(params,"until")?, now)?; Ok(Value::Null) }
                        "notifications/clear_recent" => { store.set_meta("notification:cleared_at", &now)?; Ok(Value::Null) }
                        "notifications/result" => { core::result(store, &text(params,"system_id")?, &text(params,"delivery_id")?, parse(params,"result")?, now)?; Ok(Value::Null) }
                        _ => Err(Error::InvalidInput("notification method or scope".into())),
                    })
                }
            }
        }).await?;
        if !matches!(
            method,
            "notifications/next"
                | "notifications/list"
                | "notifications/status"
                | "notifications/settings"
        ) {
            self.notifications.wake.notify_one();
            if matches!(
                method,
                "notifications/settings_set" | "notifications/observed"
            ) {
                self.notifications.receive.notify_one();
            }
        }
        Ok(result)
    }

    /// Legacy untyped hints create one pending quiet item, not one per hint.
    pub(crate) async fn notification_hint(
        &self,
        expected_account: &str,
        kind: &str,
        resource: Option<&str>,
    ) {
        let _ = self
            .hold()
            .with(|c| {
                let account = expected_account.to_string();
                let id: AccountId = account.parse().map_err(|_| Error::IntegrityFailed)?;
                let now = now_ms();
                c.with_notification_store(&id, |store| {
                    if let Some(resource) = resource {
                        let target = Target::Channel(resource.to_string());
                        if store
                            .notification_item(&core::item_id(&account, &target))?
                            .is_some_and(|i| i.is_muted(now))
                        {
                            return Ok(());
                        }
                    }
                    if !matches!(kind, "channel" | "inbox" | "connect") {
                        return Ok(());
                    }
                    let target = Target::Summary(if kind == "connect" {
                        core::Category::Action
                    } else {
                        core::Category::Message
                    });
                    let key = core::item_id(&account, &target);
                    if store.notification_item(&key)?.is_some_and(|i| i.open) {
                        return Ok(());
                    }
                    let mut item = core::Item::new(account, target, now.max(1), now);
                    item.unread = None;
                    item.quiet = true;
                    core::upsert(store, item)?;
                    Ok(())
                })
            })
            .await;
        self.notifications.wake.notify_one();
    }
}
