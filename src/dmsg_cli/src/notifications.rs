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

pub(crate) const METHODS: [&str; 10] = [
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
            *self
                .notifications
                .context
                .lock()
                .expect("notification context") = Some((generation, context));
            self.notifications.wake.notify_one();
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
                    Ok(json!({"global":runtime.settings,"account":settings,"account_id":account.map(|id| id.to_string()),"remote_supported":false}))
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
                        let batch = c.with_notification_store(&id, |store| core::next(store, &mut runtime, &context, |runtime| runtime.save(&path)))?;
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
                            let scope = params["scope"].as_str().unwrap_or("open");
                            ensure(matches!(scope, "open" | "recent"), Error::InvalidInput("scope".into()))?;
                            let cleared = store.meta::<u64>("notification:cleared_at")?.unwrap_or(0);
                            let mut items = store.notification_items()?;
                            items.retain(|item| {
                                let actionable = item.open && item.actionable && !(item.category() == core::Category::Action && item.deadline.is_some_and(|at| at <= now));
                                if scope == "open" { actionable } else {
                                    actionable || (item.updated_at >= now.saturating_sub(core::RETENTION_MS)
                                        && (item.open || item.updated_at > cleared))
                                }
                            });
                            if scope == "open" { items.sort_by_key(|item| (item.category() != core::Category::Security, item.deadline.unwrap_or(u64::MAX), std::cmp::Reverse(item.first_at))); }
                            else { items.truncate(1000); }
                            let prefix = format!("{account}:{scope}:");
                            let offset = match params["cursor"].as_str() {
                                Some(cursor) => cursor.strip_prefix(&prefix).and_then(|p| p.parse::<usize>().ok()).ok_or(Error::InvalidInput("cursor".into()))?,
                                None => 0,
                            };
                            let limit = params["limit"].as_u64().unwrap_or(50).clamp(1,100) as usize;
                            let next = offset.saturating_add(limit);
                            let cursor = (next < items.len()).then(|| format!("{prefix}{next}"));
                            Ok(json!({"items":items.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"next_cursor":cursor}))
                        }
                        "notifications/observed" => { core::observe(store, &text(params,"id")?, sequence(params,"version")?)?; Ok(Value::Null) }
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
                let Some(account) = c.session_state().account else {
                    return Ok(());
                };
                ensure(account == expected_account, Error::Locked)?;
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
                    if !matches!(kind, "channel" | "inbox") {
                        return Ok(());
                    }
                    let target = Target::Summary(core::Category::Message);
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
