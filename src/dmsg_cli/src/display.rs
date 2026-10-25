//! Local display reads and two bounded, session-owned refresh workers.
//! No worker retains client keys across a network wait or records activity.

use crate::daemon::Daemon;
use base64::Engine;
use dmsg_client::{
    display::{self, Session},
    hold::{Hold, Unlocked},
    now_ms, principal,
};
use dmsg_types::{AccountId, Error, Result, ensure_valid};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

#[derive(Default)]
struct Queue {
    session: Option<Session>,
    profile: bool,
    profile_running: bool,
    profile_after: Option<Instant>,
    avatar_after: Option<Instant>,
    avatar_url: Option<String>,
    names: BTreeSet<AccountId>,
    running: BTreeSet<AccountId>,
    after: BTreeMap<AccountId, Instant>,
}

#[derive(Default)]
pub(crate) struct Display {
    queue: Mutex<Queue>,
    profile: Notify,
    names: Notify,
}

fn ready(after: Option<Instant>, now: Instant) -> bool {
    after.is_none_or(|at| at <= now)
}

impl Queue {
    fn bind(&mut self, session: Session) {
        if self.session != Some(session) {
            *self = Self {
                session: Some(session),
                ..Self::default()
            };
        }
    }
}

impl Display {
    #[cfg(test)]
    pub(crate) fn profile_work(&self) -> (bool, bool) {
        let q = self.queue.lock().expect("display queue");
        (q.profile, q.profile_running)
    }

    pub(crate) fn clear(&self) {
        *self.queue.lock().expect("display queue") = Queue::default();
    }

    fn profile(&self, session: Session, profile: bool, avatar: Option<&str>) -> bool {
        let mut q = self.queue.lock().expect("display queue");
        q.bind(session);
        if avatar.is_some() && q.avatar_url.as_deref() != avatar {
            q.avatar_after = None;
            q.avatar_url = avatar.map(str::to_owned);
            // A publication can replace the URL while the old download waits.
            // Keep one follow-up so the new image does not wait for another view.
            if q.profile_running {
                q.profile = true;
            }
        }
        let now = Instant::now();
        if !q.profile_running
            && ((profile && ready(q.profile_after, now))
                || (avatar.is_some() && ready(q.avatar_after, now)))
        {
            q.profile = true;
            self.profile.notify_one();
        }
        q.profile || q.profile_running
    }

    fn names(&self, session: Session, accounts: impl IntoIterator<Item = AccountId>) {
        let mut q = self.queue.lock().expect("display queue");
        q.bind(session);
        let now = Instant::now();
        q.after.retain(|_, at| *at > now);
        for account in accounts {
            if q.names.len() >= 2048 {
                break;
            }
            if !q.running.contains(&account) && !q.after.contains_key(&account) {
                q.names.insert(account);
            }
        }
        if !q.names.is_empty() {
            self.names.notify_one();
        }
    }
}

pub(crate) fn image(mime: &str, bytes: &[u8]) -> Value {
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    json!({"mime": mime, "url": format!("data:{mime};base64,{data}")})
}

impl Daemon {
    /// Both the snapshot and its queued refresh belong to this same local step.
    pub(crate) async fn profile_display(&self) -> Result<Value> {
        self.hold().with(|c| {
            let session = c.display_session()?;
            let now = now_ms();
            let profile = c.profile_display()?;
            let avatar = profile.as_ref().and_then(|p| p.avatar_url.as_deref())
                .map(|url| c.profile_avatar(url)).transpose()?.flatten();
            let stale = profile.as_ref().is_none_or(|p| display::stale(p.observed_at_ms, display::PROFILE_TTL, now));
            let avatar_due = profile.as_ref().is_some_and(|p| p.avatar_url.is_some())
                && avatar.as_ref().is_none_or(|a| display::stale(a.fetched_at_ms, display::AVATAR_TTL, now));
            let refreshing = self.display.profile(session, stale, avatar_due.then(|| profile.as_ref().and_then(|p| p.avatar_url.as_deref())).flatten());
            Ok(json!({
                "state": profile.as_ref().map_or("missing", |p| if p.updated_at.is_some() { "present" } else { "absent" }),
                "name": profile.as_ref().and_then(|p| p.name.as_deref()),
                "avatar": avatar.map(|a| image(&a.mime, &a.bytes)),
                "updated_at": profile.as_ref().and_then(|p| p.updated_at),
                "observed_at_ms": profile.as_ref().map(|p| p.observed_at_ms),
                "stale": stale, "refreshing": refreshing,
            }))
        }).await
    }

    pub(crate) async fn display_names(&self, accounts: &[String]) -> Result<Value> {
        ensure_valid(accounts.len() <= 100, "at most 100 accounts")?;
        let accounts: BTreeSet<AccountId> = accounts
            .iter()
            .map(|s| s.parse().map_err(|_| dmsg_types::invalid("account")))
            .collect::<Result<_>>()?;
        self.hold()
            .with(|c| {
                let session = c.display_session()?;
                let now = now_ms();
                let known = c.public_names()?;
                self.display.names(
                    session,
                    accounts
                        .iter()
                        .filter(|id| known.get(id).is_none_or(|n| n.stale(now)))
                        .copied(),
                );
                Ok(Value::Object(
                    accounts
                        .iter()
                        .filter_map(|id| {
                            known.get(id).map(|n| {
                                (
                                    id.to_string(),
                                    json!({"name": n.name, "stale": n.stale(now)}),
                                )
                            })
                        })
                        .collect(),
                ))
            })
            .await
    }
}

/// Start once with the daemon; each category has one network worker.
pub(crate) async fn run(daemon: Arc<Daemon>) {
    tokio::join!(profiles(&daemon), names(&daemon));
}

async fn profiles(d: &Daemon) {
    loop {
        d.display.profile.notified().await;
        loop {
            let job = {
                let mut q = d.display.queue.lock().expect("display queue");
                if q.profile {
                    q.profile = false;
                    q.profile_running = true;
                    q.session
                } else {
                    None
                }
            };
            let Some(session) = job else {
                break;
            };
            let result = refresh_profile(d, session).await;
            if let Err(ref error) = result {
                dmsg_client::diagnostic::Diagnostic::error(error).emit();
            }
            let mut q = d.display.queue.lock().expect("display queue");
            if q.session == Some(session) {
                q.profile_running = false;
                if let Err(error) = result {
                    let delay = if matches!(error, Error::Unavailable(_)) {
                        display::ABSENT_TTL
                    } else {
                        display::PROFILE_TTL
                    };
                    q.profile_after = Some(Instant::now() + Duration::from_millis(delay));
                }
            }
        }
    }
}

async fn refresh_profile(d: &Daemon, session: Session) -> Result<()> {
    let shared = d.hold();
    let h = Unlocked::new(&shared).await?;
    let (config, due) = h
        .with(|c| {
            c.check_display_session(session)?;
            let now = now_ms();
            Ok((
                c.config().clone(),
                c.profile_display()?
                    .is_none_or(|p| display::stale(p.observed_at_ms, display::PROFILE_TTL, now)),
            ))
        })
        .await?;
    let allowed = {
        let q = d.display.queue.lock().expect("display queue");
        q.session == Some(session) && ready(q.profile_after, Instant::now())
    };
    // Avatar refresh still runs when the directory is temporarily unavailable.
    let mut failure = None;
    if due && allowed {
        match principal::resolve(&config, &session.account).await {
            Ok(document) => {
                h.with(|c| {
                    c.check_display_session(session)?;
                    let changed = c.keep_profile_display(display::Profile::from_document(
                        document.as_ref(),
                        now_ms(),
                    ))?;
                    let mut q = d.display.queue.lock().expect("display queue");
                    if q.session == Some(session) {
                        q.profile_after =
                            Some(Instant::now() + Duration::from_millis(display::PROFILE_TTL));
                    }
                    drop(q);
                    if changed {
                        d.emit("profile/changed", Value::Null);
                    }
                    Ok(())
                })
                .await?
            }
            Err(error) => failure = Some(error),
        }
    }
    let (url, version) = h
        .with(|c| {
            c.check_display_session(session)?;
            let now = now_ms();
            let profile = c.profile_display()?;
            let version = profile.as_ref().and_then(|p| p.updated_at);
            let url = profile.and_then(|p| p.avatar_url);
            let due = match &url {
                Some(url) => c
                    .profile_avatar(url)?
                    .is_none_or(|a| display::stale(a.fetched_at_ms, display::AVATAR_TTL, now)),
                None => false,
            };
            Ok((url.filter(|_| due), version))
        })
        .await?;
    let allowed = {
        let mut q = d.display.queue.lock().expect("display queue");
        if q.session == Some(session) && q.avatar_url != url {
            q.avatar_url = url.clone();
            q.avatar_after = None;
        }
        q.session == Some(session) && ready(q.avatar_after, Instant::now())
    };
    if let Some(url) = url
        && allowed
    {
        let fetched_url = url.clone();
        let result = principal::avatar(&h, &url).await;
        let mut changed = false;
        match result {
            Ok(Some((mime, bytes))) => {
                changed = h
                    .with(|c| {
                        c.check_display_session(session)?;
                        if c.profile_display()?.and_then(|p| p.updated_at) != version {
                            return Ok(false);
                        }
                        c.keep_profile_avatar(display::Avatar {
                            url,
                            mime: mime.into(),
                            bytes,
                            fetched_at_ms: now_ms(),
                        })
                    })
                    .await?
            }
            Ok(None) => {}
            Err(error) => dmsg_client::diagnostic::Diagnostic::error(&error).emit(),
        }
        h.with(|c| {
            c.check_display_session(session)?;
            let current = c.profile_display()?;
            let same = current.as_ref().and_then(|p| p.updated_at) == version
                && current.as_ref().and_then(|p| p.avatar_url.as_ref()) == Some(&fetched_url);
            let mut q = d.display.queue.lock().expect("display queue");
            if q.session == Some(session) && !same {
                q.profile = true;
            }
            if q.session == Some(session) && same && q.avatar_url.as_ref() == Some(&fetched_url) {
                q.avatar_after = Some(Instant::now() + Duration::from_millis(display::AVATAR_TTL));
            }
            if changed {
                d.emit("profile/changed", Value::Null);
            }
            Ok(())
        })
        .await?;
    }
    failure.map_or(Ok(()), Err)
}

async fn names(d: &Daemon) {
    loop {
        d.display.names.notified().await;
        loop {
            let job = {
                let mut q = d.display.queue.lock().expect("display queue");
                let ids: Vec<_> = q.names.iter().take(100).copied().collect();
                for id in &ids {
                    q.names.remove(id);
                    q.running.insert(*id);
                }
                q.session.filter(|_| !ids.is_empty()).map(|s| (s, ids))
            };
            let Some((session, ids)) = job else {
                break;
            };
            let result = refresh_names(d, session, &ids).await;
            if let Err(error) = result {
                dmsg_client::diagnostic::Diagnostic::error(&error).emit();
            }
            let mut q = d.display.queue.lock().expect("display queue");
            if q.session == Some(session) {
                let after = Instant::now() + Duration::from_millis(display::ABSENT_TTL);
                for id in ids {
                    q.running.remove(&id);
                    q.after.entry(id).or_insert(after);
                }
            }
        }
    }
}

async fn refresh_names(d: &Daemon, session: Session, ids: &[AccountId]) -> Result<()> {
    let shared = d.hold();
    let h = Unlocked::new(&shared).await?;
    h.with(|c| c.check_display_session(session)).await?;
    let found = principal::names(&h, ids).await?;
    h.with(|c| {
        c.check_display_session(session)?;
        let now = now_ms();
        let retry_at = Instant::now();
        let mut kept = Vec::new();
        for (id, result) in found {
            match result {
                Ok(mut name) => {
                    name.observed_at_ms = now;
                    kept.push((id, name));
                }
                Err(error) => {
                    dmsg_client::diagnostic::Diagnostic::error(&error).emit();
                    let ttl = if matches!(error, Error::Unavailable(_)) {
                        display::ABSENT_TTL
                    } else {
                        display::PROFILE_TTL
                    };
                    let mut q = d.display.queue.lock().expect("display queue");
                    if q.session == Some(session) {
                        q.after.insert(id, retry_at + Duration::from_millis(ttl));
                    }
                }
            }
        }
        if !kept.is_empty() && c.keep_public_names(kept)? {
            d.emit("names/changed", Value::Null);
        }
        Ok(())
    })
    .await
}
