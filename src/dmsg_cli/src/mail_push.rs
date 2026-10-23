//! Owner-enabled Mail webhook lifecycle. The existing encrypted MCP callback
//! relay and hint channel carry notifications; mail itself stays on its relay.

use crate::daemon::Daemon;
use dmsg_client::{
    hold::Hold,
    mail_push::{Callback, Subscription, Watch},
    now_ms,
};
use dmsg_types::{Error, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::Notify;

const CHECK_INTERVAL: Duration = Duration::from_secs(10 * 60);
const RENEW_BEFORE_MS: u64 = 60 * 60 * 1000;

#[derive(Default)]
pub(crate) struct MailPush {
    pub(crate) receive: Notify,
    pub(crate) maintain: Notify,
}

pub(crate) async fn run(daemon: Arc<Daemon>) {
    let mut tick = tokio::time::interval(CHECK_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! { _=tick.tick()=>{}, _=daemon.mail_push().maintain.notified()=>{} }
        if daemon.stopping() {
            continue;
        }
        // One hold for the watches and their maintenance: an account switch
        // ends this round instead of pointing the queued work at its data.
        let h = daemon.hold();
        let watches = h.with(|c| c.mail_push_watches()).await;
        let Ok(watches) = watches else { continue };
        for (agent, watch) in watches {
            let generation = watch.generation;
            if let Err(error) = daemon.maintain_mail_push(&h, &agent, watch).await {
                if let Ok(mut all) = h.with(|c| c.mail_push_watches()).await
                    && let Some(watch) = all.get_mut(&agent)
                    && watch.generation == generation
                {
                    watch.last_error = Some(match &error {
                        Error::Unavailable(message) if message.starts_with("Mail event ") => {
                            message.clone()
                        }
                        _ => dmsg_client::error_code(&error),
                    });
                    let _ = h.with(|c| c.mail_push_save(&agent, watch)).await;
                }
                daemon.note("mail-push", &dmsg_client::error_code(&error));
            }
            daemon.emit("agent/mail_push", json!({"agent":agent}));
        }
    }
}

impl Daemon {
    pub(crate) async fn configure_mail_push(
        &self,
        agent: &str,
        enabled: bool,
        via: &str,
    ) -> Result<Value> {
        let view = self
            .hold()
            .with(|c| {
                let entry = c.agent_find(agent)?;
                if enabled {
                    ensure(!entry.paused, Error::Forbidden)?;
                    c.capability_check(&entry.agent_id, "events.endpoint")?;
                    ensure(
                        !c.capability_targets(
                            &entry.agent_id,
                            "mail.read",
                            self.agents().origins("mail.read"),
                        )?
                        .is_empty(),
                        Error::Forbidden,
                    )?;
                }
                let view = c.mail_push_set(&entry.agent_id, enabled)?;
                c.audit(
                    "owner",
                    via,
                    "agent/mail_push_set",
                    Some(&entry.agent_id),
                    Some(if enabled { "enabled" } else { "disabled" }),
                    "ok",
                );
                Ok(view)
            })
            .await?;
        self.mail_push().maintain.notify_one();
        self.mail_push().receive.notify_one();
        self.emit("agent/mail_push", json!({"agent":view["agent"]}));
        self.emit("audit/changed", Value::Null);
        Ok(view)
    }

    async fn maintain_mail_push(&self, h: &impl Hold, agent: &str, mut watch: Watch) -> Result<()> {
        // Bind the account before waiting for this Agent's turn: maintenance
        // queued under one account must not run against another.
        let account = h.with(|c| Ok(c.session_state().account)).await?;
        let _turn = self.turn(&format!("mail-push:{agent}")).await;
        let allowed = h
            .with(|c| {
                let current = c
                    .mail_push_watches()?
                    .remove(agent)
                    .ok_or(Error::NotFound)?;
                ensure(current.generation == watch.generation, Error::PolicyStale)?;
                Ok(watch.enabled
                    && c.agent_find(agent).is_ok_and(|a| !a.paused)
                    && c.capability_check(agent, "events.endpoint").is_ok()
                    && !c
                        .capability_targets(agent, "mail.read", self.agents().origins("mail.read"))?
                        .is_empty())
            })
            .await?;
        let origins = self.agents().origins("mail.read");
        let mut desired = BTreeMap::new();
        if allowed {
            let entry = h.with(|c| c.agent_find(agent)).await?;
            let boxes=self.call_agent(&entry,"mcp/call",&json!({"tool":"agent_protocols_mail_mailboxes_list","input":{},"mcp_session":"mail-subscriptions"})).await?;
            // A revoked Mail grant must still reach cleanup below.
            if boxes.get("error").is_none() {
                for mailbox in boxes["result"]["mailboxes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    if mailbox["status"] != "open" {
                        continue;
                    }
                    let id = mailbox["mailbox_id"]
                        .as_str()
                        .ok_or(Error::IntegrityFailed)?;
                    for route in mailbox["routes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        if !origins.iter().any(|o| o == route) {
                            continue;
                        }
                        let granted = h
                            .with(|c| {
                                Ok(c.capability_check(agent, &format!("mail.read:{route}"))
                                    .is_ok())
                            })
                            .await?;
                        if granted {
                            desired.insert(
                                format!("{route}/{id}"),
                                Subscription {
                                    service: route.into(),
                                    mailbox_id: id.into(),
                                    ..Default::default()
                                },
                            );
                        }
                    }
                }
            }
        }
        let mut endpoint = h.with(|c| c.mail_push_endpoint(agent)).await?;
        let http = self.agents().http_client();
        if let Some(endpoint) = &endpoint {
            let stale: Vec<_> = watch
                .subscriptions
                .iter()
                .filter(|(key, _)| !desired.contains_key(*key))
                .map(|(k, s)| (k.clone(), s.clone()))
                .collect();
            for (key, sub) in stale {
                // Removing a configured origin must not make another request to it.
                if origins.contains(&sub.service) {
                    let _=dmsg_client::mail_push::request(h,&http,agent,&sub.service,"events/unsubscribe",json!({"name":"dmsg.mail.available","arguments":{"mailbox_id":sub.mailbox_id},"delivery":{"mode":"webhook","url":endpoint.url}})).await;
                }
                watch.subscriptions.remove(&key);
            }
        }
        if desired.is_empty() {
            let unlocked = h
                .with(|c| Ok(c.session_state().unlocked_account().is_some()))
                .await?;
            if let Some(endpoint) = endpoint.as_ref().filter(|_| unlocked) {
                // Reuse the existing journaled endpoint lifecycle. This rare
                // cleanup still uses its documented under-lock implementation.
                watch.generation = self
                    .under_lock(async |c| {
                        ensure(
                            c.session_state().account == account,
                            Error::Unavailable("the account changed".into()),
                        )?;
                        let current = c
                            .mail_push_watches()?
                            .remove(agent)
                            .ok_or(Error::NotFound)?;
                        ensure(current.generation == watch.generation, Error::PolicyStale)?;
                        c.mcp_endpoint_delete(Some(agent), &endpoint.endpoint_id)
                            .await?;
                        // Deletion advances the callback generation under this lock.
                        Ok(c.mail_push_watches()?
                            .remove(agent)
                            .ok_or(Error::NotFound)?
                            .generation)
                    })
                    .await?;
            }
            // Resident Agents can unsubscribe while locked. Account-owned
            // endpoint deletion waits for unlock; retain its cleanup target.
            if unlocked {
                watch.endpoint_id = None;
            }
            watch.subscriptions.clear();
            watch.last_error = None;
            h.with(|c| c.mail_push_save(agent, &watch)).await?;
            return Ok(());
        }
        if endpoint.is_none() {
            h.with(|c| {
                ensure(
                    c.session_state().unlocked_account().is_some(),
                    Error::Locked,
                )
            })
            .await?;
            // Endpoint creation already journals unknown outcomes; its stable
            // context lets us recover a successful creation after a restart.
            watch.generation = self
                .under_lock(async |c| {
                    ensure(
                        c.session_state().account == account,
                        Error::Unavailable("the account changed".into()),
                    )?;
                    let current = c
                        .mail_push_watches()?
                        .remove(agent)
                        .ok_or(Error::NotFound)?;
                    ensure(
                        current.enabled && current.generation == watch.generation,
                        Error::PolicyStale,
                    )?;
                    c.mcp_endpoint_create(
                        Some(agent),
                        Some("mail-push"),
                        BTreeMap::from([("label".into(), "dMsg Mail push".into())]),
                    )
                    .await?;
                    // Keep our own creation's generation while preserving local
                    // cleanup progress; later owner/callback changes still fence it.
                    Ok(c.mail_push_watches()?
                        .remove(agent)
                        .ok_or(Error::NotFound)?
                        .generation)
                })
                .await?;
            endpoint = h.with(|c| c.mail_push_endpoint(agent)).await?;
            // A callback replacement requires all upstreams to bind the new URL.
            for sub in watch.subscriptions.values_mut() {
                sub.refresh_before = 0;
                sub.id = None;
            }
        }
        let endpoint = endpoint.ok_or(Error::IntegrityFailed)?;
        watch.endpoint_id = Some(endpoint.endpoint_id.clone());
        self.renew_mail_push(h, agent, &mut watch, &endpoint, desired)
            .await
    }

    /// Keep each mailbox's progress even when another subscription fails.
    async fn renew_mail_push(
        &self,
        h: &impl Hold,
        agent: &str,
        watch: &mut Watch,
        endpoint: &Callback,
        desired: BTreeMap<String, Subscription>,
    ) -> Result<()> {
        let http = self.agents().http_client();
        let now = now_ms();
        let due: Vec<_> = desired
            .into_iter()
            .filter(|(key, _)| {
                watch
                    .subscriptions
                    .get(key)
                    .is_none_or(|s| s.refresh_before <= now.saturating_add(RENEW_BEFORE_MS))
            })
            .collect();
        let mut failure = None;
        for (key, proposed) in due {
            let sub = watch
                .subscriptions
                .entry(key.clone())
                .or_insert(proposed)
                .clone();
            // Persist the target before subscribing so disable/restart can clean
            // up even if the response or process is lost.
            h.with(|c| {
                ensure(
                    c.mail_push_endpoint(agent)?.as_ref() == Some(endpoint),
                    Error::PolicyStale,
                )?;
                c.mail_push_save(agent, watch)
            })
            .await?;
            let renewed = async {
                let ttl = h
                    .with(|c| {
                        ensure(c.mail_push_endpoint(agent)?.as_ref() == Some(endpoint), Error::PolicyStale)?;
                        c.mail_push_ttl(agent, &sub.service, now_ms())
                    })
                    .await?;
                let response = dmsg_client::mail_push::request(
                    h,
                    &http,
                    agent,
                    &sub.service,
                    "events/subscribe",
                    json!({
                        "name": "dmsg.mail.available",
                        "arguments": {"mailbox_id": sub.mailbox_id},
                        "delivery": {"mode": "webhook", "url": endpoint.url, "secret": endpoint.secret},
                        "ttlMs": ttl,
                        "cursor": sub.cursor,
                    }),
                )
                .await?;
                let id = response["id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or(Error::IntegrityFailed)?
                    .to_owned();
                let refresh = response["refreshBefore"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .and_then(|d| u64::try_from(d.timestamp_millis()).ok())
                    .ok_or(Error::IntegrityFailed)?;
                ensure(refresh > now_ms(), Error::IntegrityFailed)?;
                Ok((id, refresh, response["cursor"].as_str().map(str::to_owned)))
            }
            .await;
            let (id, refresh, cursor) = match renewed {
                Ok(renewed) => renewed,
                Err(error) => {
                    // A failed mailbox or optional event service must not
                    // prevent the remaining subscriptions from renewing.
                    failure.get_or_insert(error);
                    continue;
                }
            };
            let current = watch.subscriptions.get_mut(&key).expect("target saved");
            current.id = Some(id);
            current.refresh_before = refresh;
            current.cursor = cursor;
            h.with(|c| {
                ensure(
                    c.mail_push_endpoint(agent)?.as_ref() == Some(endpoint),
                    Error::PolicyStale,
                )?;
                c.mail_push_save(agent, watch)
            })
            .await?;
            // Subscribe before the bootstrap read, covering mail that arrived
            // before or during callback verification and any replay gap.
            self.mail_push().receive.notify_one();
        }
        if let Some(error) = failure {
            return Err(error);
        }
        watch.last_error = None;
        h.with(|c| {
            ensure(
                c.mail_push_endpoint(agent)?.as_ref() == Some(endpoint),
                Error::PolicyStale,
            )?;
            c.mail_push_save(agent, watch)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::post};
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };

    #[tokio::test]
    async fn queued_mail_maintenance_does_not_follow_an_account_switch() {
        let dir = dmsg_client::testing::TempDir::new();
        let home = crate::home::Home {
            dir: dir.path("home"),
        };
        let mut client = dmsg_client::testing::unlocked(&dir.path("workspace"), false, None);
        let agent = client.agent_create("mail-test").unwrap().agent_id;
        client.mail_push_set(&agent, true).unwrap();
        let watch = client.mail_push_watches().unwrap().remove(&agent).unwrap();
        let daemon = Arc::new(Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            client,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        ));
        let h = daemon.hold();
        let turn = daemon.turn(&format!("mail-push:{agent}")).await;
        let mut work = Box::pin(daemon.maintain_mail_push(&h, &agent, watch));
        assert!(futures_util::poll!(&mut work).is_pending());
        {
            let mut client = daemon.client().lock().await;
            *client = dmsg_client::Client::open(
                dmsg_client::testing::config(false),
                &dir.path("other-account"),
                None,
            )
            .unwrap();
        }
        drop(turn);
        assert_eq!(
            work.await,
            Err(Error::Unavailable("the account changed".into()))
        );
    }

    #[tokio::test]
    async fn a_failed_subscription_does_not_block_other_mailboxes_or_their_renewals() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let failed = Arc::new(AtomicBool::new(true));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new().route("/mcp", post({
            let failed = failed.clone();
            let requests = requests.clone();
            move |Json(input): Json<Value>| {
                let failed = failed.clone();
                let requests = requests.clone();
                async move {
                    assert_eq!(input["params"]["ttlMs"], 7 * 86_400_000u64);
                    let mailbox = input["params"]["arguments"]["mailbox_id"].as_str().unwrap();
                    let mut seen = requests.lock().unwrap();
                    if mailbox == "second" && seen.iter().any(|name| name == "second") {
                        assert_eq!(input["params"]["cursor"], "head");
                    }
                    seen.push(mailbox.to_owned());
                    drop(seen);
                    if mailbox == "first" && failed.load(Ordering::SeqCst) {
                        Json(json!({"jsonrpc":"2.0","id":1,"error":{"code":-32027}}))
                    } else {
                        Json(json!({"jsonrpc":"2.0","id":1,"result":{
                            "id":format!("subscription-{mailbox}"),
                            "refreshBefore":dmsg_client::mcp_events::timestamp(now_ms()+7*86_400_000),
                            "cursor":"head"
                        }}))
                    }
                }
            }
        }));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dir = dmsg_client::testing::TempDir::new();
        let home = crate::home::Home {
            dir: dir.path("home"),
        };
        let mut client = dmsg_client::testing::unlocked(
            &dir.path("workspace"),
            false,
            Some(Arc::new(dmsg_client::testing::MemoryStorage::default())),
        );
        let agent = client.agent_create("mail-test").unwrap().agent_id;
        client
            .capability_grant(
                "cli",
                &agent,
                vec!["mail.read:*".into(), "events.endpoint".into()],
                None,
            )
            .unwrap();
        client.mail_push_set(&agent, true).unwrap();
        let endpoint = dmsg_client::testing::mail_push_callback(&mut client, &agent);
        let mut watch = client.mail_push_watches().unwrap().remove(&agent).unwrap();
        client.agent_resident(&agent, true).unwrap();
        client.lock();
        let daemon = Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            client,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        );
        let desired: BTreeMap<_, _> = ["first", "second"]
            .into_iter()
            .map(|id| {
                (
                    format!("{origin}/{id}"),
                    Subscription {
                        service: origin.clone(),
                        mailbox_id: id.into(),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let result = daemon
            .renew_mail_push(
                &daemon.hold(),
                &agent,
                &mut watch,
                &endpoint,
                desired.clone(),
            )
            .await;
        assert!(
            result.is_err(),
            "the failed subscription must still be reported"
        );
        let saved = daemon
            .client()
            .lock()
            .await
            .mail_push_watches()
            .unwrap()
            .remove(&agent)
            .unwrap();
        assert_eq!(
            saved
                .subscriptions
                .get(&format!("{origin}/second"))
                .and_then(|s| s.id.as_deref()),
            Some("subscription-second")
        );
        assert!(saved.subscriptions[&format!("{origin}/first")].id.is_none());
        failed.store(false, Ordering::SeqCst);
        daemon
            .renew_mail_push(
                &daemon.hold(),
                &agent,
                &mut watch,
                &endpoint,
                desired.clone(),
            )
            .await
            .unwrap();
        assert_eq!(*requests.lock().unwrap(), ["first", "second", "first"]);
        // A valid weekly lease stays untouched outside the one-hour window.
        watch
            .subscriptions
            .get_mut(&format!("{origin}/second"))
            .unwrap()
            .refresh_before = now_ms() + RENEW_BEFORE_MS + 60_000;
        daemon
            .renew_mail_push(
                &daemon.hold(),
                &agent,
                &mut watch,
                &endpoint,
                desired.clone(),
            )
            .await
            .unwrap();
        assert_eq!(requests.lock().unwrap().len(), 3);
        watch
            .subscriptions
            .get_mut(&format!("{origin}/second"))
            .unwrap()
            .refresh_before = now_ms() + RENEW_BEFORE_MS;
        daemon
            .renew_mail_push(
                &daemon.hold(),
                &agent,
                &mut watch,
                &endpoint,
                desired.clone(),
            )
            .await
            .unwrap();
        assert_eq!(
            *requests.lock().unwrap(),
            ["first", "second", "first", "second"]
        );
        assert_eq!(
            watch.subscriptions[&format!("{origin}/second")]
                .cursor
                .as_deref(),
            Some("head")
        );
        // Pausing a resident Agent blocks even a due renewal while locked.
        daemon
            .client()
            .lock()
            .await
            .agent_pause(&agent, true)
            .unwrap();
        watch
            .subscriptions
            .get_mut(&format!("{origin}/second"))
            .unwrap()
            .refresh_before = 0;
        assert_eq!(
            daemon
                .renew_mail_push(&daemon.hold(), &agent, &mut watch, &endpoint, desired)
                .await,
            Err(Error::Forbidden)
        );
        assert_eq!(requests.lock().unwrap().len(), 4);
        server.abort();
        let _ = server.await;
    }
}
