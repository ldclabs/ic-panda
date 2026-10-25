//! Explicit owner display methods, with local/SWR/fresh reads. No command or
//! authorization method can enter this scheduler merely by supplying a mode.

use crate::{daemon::Daemon, params::to_json};
use dmsg_client::{
    cache::{Kind, Snapshot},
    display::{self, Session},
    hold::{Hold, Unlocked},
    now_ms,
};
use dmsg_types::{Error, Result, ensure_valid};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Resource {
    kind: Kind,
    id: String,
}

pub(crate) fn supports(method: &str) -> bool {
    matches!(
        method,
        "plan/status"
            | "plan/catalog"
            | "plan/assets"
            | "plan/panda"
            | "handle/mine"
            | "account/show"
            | "account/security"
            | "recovery/status"
            | "wallet/show"
            | "wallet/balances"
            | "wallet/history"
            | "wallet/queue"
            | "inbox/policy"
            | "files/storage"
            | "files/cloud_usage"
    )
}

pub(crate) fn invalidates(method: &str) -> &'static [Kind] {
    if crate::wallet_methods::events_of(method).contains(&"wallet/changed") {
        return &[Kind::Wallet, Kind::Balances, Kind::History, Kind::Asset];
    }
    match method {
        "approval/respond" | "approval/reconcile" => &[
            Kind::Account,
            Kind::Summary,
            Kind::Recovery,
            Kind::Plan,
            Kind::Handles,
            Kind::Wallet,
            Kind::Balances,
            Kind::History,
        ],
        "plan/cancel" | "plan/reconcile" | "plan/notice_ack" => &[Kind::Plan, Kind::Usage],
        "handle/reconcile" | "handle/transfer_commit" => &[Kind::Handles],
        "files/storage_action" => &[Kind::Storage, Kind::Usage],
        _ => &[],
    }
}

fn resource(method: &str, params: &Value) -> Result<Resource> {
    let kind = match method {
        "plan/status" => {
            ensure_valid(
                params["refresh"] != true,
                "display reads do not refresh entitlement",
            )?;
            Kind::Plan
        }
        "plan/catalog" => Kind::Catalog,
        "plan/assets" => Kind::Assets,
        "plan/panda" => Kind::Panda,
        "handle/mine" => Kind::Handles,
        "account/security" => Kind::Account,
        "account/show" => Kind::Summary,
        "recovery/status" => Kind::Recovery,
        "wallet/show" => Kind::Wallet,
        "wallet/balances" => Kind::Balances,
        "wallet/history" | "wallet/queue" => Kind::History,
        "inbox/policy" => Kind::Inbox,
        "files/storage" => Kind::Storage,
        "files/cloud_usage" => Kind::Usage,
        _ => {
            return Err(dmsg_types::invalid(
                "display mode is not supported by this method",
            ));
        }
    };
    let id = if matches!(kind, Kind::Wallet | Kind::Balances | Kind::History) {
        candid::Principal::from_text(crate::params::text(params, "wallet")?)
            .map_err(|_| dmsg_types::invalid("wallet"))?
            .to_text()
    } else {
        String::new()
    };
    Ok(Resource { kind, id })
}

#[derive(Default)]
struct Queue {
    session: Option<Session>,
    pending: BTreeSet<Resource>,
    running: BTreeSet<Resource>,
    failed: BTreeMap<Resource, (Instant, String)>,
}

#[derive(Default)]
pub(crate) struct Snapshots {
    queue: Mutex<Queue>,
    wake: Notify,
}

impl Snapshots {
    pub(crate) fn clear(&self) {
        *self.queue.lock().expect("snapshots") = Queue::default();
    }
}

impl Daemon {
    pub(crate) async fn snapshot(&self, method: &str, params: &Value) -> Result<Value> {
        let mode = params["mode"]
            .as_str()
            .ok_or_else(|| dmsg_types::invalid("mode"))?;
        ensure_valid(matches!(mode, "local" | "swr" | "fresh"), "mode")?;
        let r = resource(method, params)?;
        let shared = self.hold();
        let h = Unlocked::new(&shared).await?;
        let session = h.with(|c| c.display_session()).await?;
        if mode == "fresh" {
            let _turn = self
                .turn(&format!("display:{}:{}", r.kind.name(), r.id))
                .await;
            fetch(self, &h, session, &r).await?;
        }
        h.with(|c| {
            c.check_display_session(session)?;
            let now = now_ms();
            let saved = c.cache_read(r.kind, &r.id)?;
            let stale = saved
                .as_ref()
                .is_none_or(|s| display::stale(s.observed_at_ms, r.kind.ttl(), now));
            let mut q = self.snapshots.queue.lock().expect("snapshots");
            if q.session != Some(session) {
                *q = Queue {
                    session: Some(session),
                    ..Default::default()
                };
            }
            let can_retry = q.failed.get(&r).is_none_or(|(at, _)| *at <= Instant::now());
            if mode == "swr"
                && stale
                && can_retry
                && !q.running.contains(&r)
                && q.pending.len() < 128
            {
                q.pending.insert(r.clone());
                self.snapshots.wake.notify_one();
            }
            let refreshing = q.pending.contains(&r) || q.running.contains(&r);
            let error = q.failed.get(&r).map(|(_, code)| code.as_str());
            Ok(response(
                saved,
                stale,
                refreshing,
                error,
                method == "wallet/queue",
            ))
        })
        .await
    }
}

fn response(
    saved: Option<Snapshot>,
    stale: bool,
    refreshing: bool,
    error: Option<&str>,
    pending: bool,
) -> Value {
    let mut data = saved
        .as_ref()
        .map(|s| s.data.clone())
        .unwrap_or(Value::Null);
    if pending && let Some(rows) = data["proposals"].as_array_mut() {
        rows.retain(|p| {
            !matches!(
                p["status"].as_str(),
                Some(
                    "Settled"
                        | "Failed"
                        | "Rejected"
                        | "Vetoed"
                        | "Cancelled"
                        | "Expired"
                        | "Stale"
                )
            ) || p["outcome"]["kind"] == "call_unknown"
                || p["status"] == "Failed"
                    && p["outcome"]["blocks"]
                        .as_array()
                        .is_some_and(|b| !b.is_empty())
        });
    }
    json!({"state": if saved.is_none() { "missing" } else if data.is_null() { "absent" } else { "present" },
        "data":data, "observed_at_ms":saved.map(|s| s.observed_at_ms), "stale":stale, "refreshing":refreshing, "error":error})
}

pub(crate) async fn run(d: Arc<Daemon>) {
    tokio::join!(worker(&d), worker(&d), worker(&d), worker(&d));
}

async fn worker(d: &Daemon) {
    loop {
        d.snapshots.wake.notified().await;
        loop {
            let job = {
                let mut q = d.snapshots.queue.lock().expect("snapshots");
                q.session.and_then(|s| {
                    q.pending.pop_first().map(|r| {
                        q.running.insert(r.clone());
                        (s, r)
                    })
                })
            };
            let Some((session, r)) = job else {
                break;
            };
            d.snapshots.wake.notify_one();
            let result = async {
                let shared = d.hold();
                let unlocked = Unlocked::new(&shared).await?;
                let h = display::Scoped {
                    inner: &unlocked,
                    session,
                };
                h.with(|c| c.check_display_session(session)).await?;
                let _turn = d.turn(&format!("display:{}:{}", r.kind.name(), r.id)).await;
                // A concurrent fresh read may already have filled it while we waited.
                if h.with(|c| {
                    Ok(c.cache_read(r.kind, &r.id)?
                        .is_some_and(|s| !display::stale(s.observed_at_ms, r.kind.ttl(), now_ms())))
                })
                .await?
                {
                    return Ok(());
                }
                fetch(d, &h, session, &r).await
            }
            .await;
            let mut q = d.snapshots.queue.lock().expect("snapshots");
            if q.session != Some(session) {
                continue;
            }
            q.running.remove(&r);
            match result {
                // A newer observation or invalidation of this kind won; the
                // invalidated snapshot is stale, so the next read refreshes it.
                Ok(()) | Err(Error::VersionConflict) => {
                    q.failed.remove(&r);
                }
                Err(error) => {
                    dmsg_client::diagnostic::Diagnostic::error(&error).emit();
                    let code = dmsg_client::error_code(&error);
                    let changed = q.failed.get(&r).is_none_or(|(_, old)| old != &code);
                    q.failed
                        .insert(r.clone(), (Instant::now() + Duration::from_secs(60), code));
                    if changed {
                        d.emit("cache/changed", json!({"kind":r.kind.name()}));
                    }
                }
            }
        }
    }
}

async fn fetch<H: Hold>(d: &Daemon, h: &H, session: Session, r: &Resource) -> Result<()> {
    use dmsg_client::{
        commerce, handle, inbox,
        library::{cloud, storage},
        security, wallet,
    };
    let scoped = display::Scoped { inner: h, session };
    let h = &scoped;
    h.with(|c| c.check_display_session(session)).await?;
    let wallet = || candid::Principal::from_text(&r.id).map_err(|_| dmsg_types::invalid("wallet"));
    let scope = h.with(|c| c.cache_scope(r.kind)).await?;
    let data = match r.kind {
        Kind::Plan => commerce::plan_status(h, false).await?,
        Kind::Catalog => commerce::catalog_view(&commerce::catalog(h).await?),
        Kind::Assets => commerce::plan_assets(h).await?,
        Kind::Panda => commerce::plan_panda(h).await?,
        Kind::Handles => Value::Array(handle::mine(h).await?),
        Kind::Account => security::account_security(h).await?,
        Kind::Summary => {
            let state = h
                .warm(dmsg_client::client::Warm::Home)
                .await?
                .ok_or(Error::PolicyStale)?;
            h.with(|c| Ok(to_json(&c.account_view_of(&state)?))).await?
        }
        Kind::Recovery => dmsg_client::recovery::status(h).await?,
        Kind::Wallet => wallet::show(h, wallet()?).await?,
        Kind::Balances => wallet::balances(h, wallet()?).await?,
        Kind::History => wallet::desktop::history(h, wallet()?, false).await?,
        Kind::Inbox => inbox::policy(h).await?.unwrap_or(Value::Null),
        Kind::Storage => storage::refresh(h).await?,
        Kind::Usage => to_json(&cloud::cloud_usage(h).await?),
        Kind::Asset => return Err(Error::Forbidden),
    };
    h.with(|c| {
        c.check_display_session(session)?;
        if r.kind == Kind::History && data["persisted"] == false {
            return Err(Error::Unavailable(
                "storage: display snapshot was not persisted".into(),
            ));
        }
        // These producers already write the same common snapshot; no second copy.
        if !matches!(r.kind, Kind::History | Kind::Inbox | Kind::Storage) {
            c.cache_save_checked(&scope, r.kind, &r.id, &data, now_ms())?;
        }
        let mut q = d.snapshots.queue.lock().expect("snapshots");
        if q.session == Some(session) {
            q.failed.remove(r);
        }
        drop(q);
        d.emit("cache/changed", json!({"kind":r.kind.name()}));
        Ok(())
    })
    .await
}
