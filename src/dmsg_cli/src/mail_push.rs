//! Owner-enabled Mail webhook lifecycle. The existing encrypted MCP callback
//! relay and hint channel carry notifications; mail itself stays on its relay.

use crate::daemon::Daemon;
use dmsg_client::{
    hold::Hold,
    mail_push::{Subscription, Watch},
    now_ms,
};
use dmsg_types::{Error, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::Notify;

#[derive(Default)]
pub(crate) struct MailPush {
    pub(crate) receive: Notify,
    pub(crate) maintain: Notify,
}

pub(crate) async fn run(daemon: Arc<Daemon>) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! { _=tick.tick()=>{}, _=daemon.mail_push().maintain.notified()=>{} }
        if daemon.stopping() {
            continue;
        }
        let watches = daemon.client().lock().await.mail_push_watches();
        let Ok(watches) = watches else { continue };
        for (agent, watch) in watches {
            let generation = watch.generation;
            if let Err(error) = daemon.maintain_mail_push(&agent, watch).await {
                let mut client = daemon.client().lock().await;
                if let Ok(mut all) = client.mail_push_watches()
                    && let Some(watch) = all.get_mut(&agent)
                    && watch.generation == generation
                {
                    watch.last_error = Some(match &error {
                        Error::Unavailable(message) if message.starts_with("Mail event ") => {
                            message.clone()
                        }
                        _ => dmsg_client::error_code(&error),
                    });
                    let _ = client.mail_push_save(&agent, watch);
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

    async fn maintain_mail_push(&self, agent: &str, mut watch: Watch) -> Result<()> {
        let _turn = self.turn(&format!("mail-push:{agent}")).await;
        let h = self.hold();
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
                    let _=dmsg_client::mail_push::request(&h,&http,agent,&sub.service,"events/unsubscribe",json!({"name":"dmsg.mail.available","arguments":{"mailbox_id":sub.mailbox_id},"delivery":{"mode":"webhook","url":endpoint.url}})).await;
                }
                watch.subscriptions.remove(&key);
            }
        }
        if desired.is_empty() {
            if let Some(endpoint) = endpoint {
                // Reuse the existing journaled endpoint lifecycle. This rare
                // cleanup still uses its documented under-lock implementation.
                self.under_lock(async |c| {
                    c.mcp_endpoint_delete(Some(agent), &endpoint.endpoint_id)
                        .await
                })
                .await?;
            }
            watch.endpoint_id = None;
            watch.subscriptions.clear();
            watch.last_error = None;
            h.with(|c| c.mail_push_save(agent, &watch)).await?;
            return Ok(());
        }
        if endpoint.is_none() {
            // Endpoint creation already journals unknown outcomes; its stable
            // context lets us recover a successful creation after a restart.
            self.under_lock(async |c| {
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
                .await
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
        let secret = endpoint
            .secrets
            .last()
            .ok_or(Error::IntegrityFailed)?
            .secret
            .clone();
        for (key, proposed) in desired {
            let sub = watch
                .subscriptions
                .entry(key.clone())
                .or_insert(proposed)
                .clone();
            if sub.refresh_before > now_ms() + 10 * 60 * 1000 {
                continue;
            }
            // Persist the target before subscribing so disable/restart can clean
            // up even if the response or process is lost.
            h.with(|c| c.mail_push_save(agent, &watch)).await?;
            let ttl = h
                .with(|c| c.mail_push_ttl(agent, &sub.service, now_ms()))
                .await?;
            let response=dmsg_client::mail_push::request(&h,&http,agent,&sub.service,"events/subscribe",json!({
                "name":"dmsg.mail.available","arguments":{"mailbox_id":sub.mailbox_id},
                "delivery":{"mode":"webhook","url":endpoint.url,"secret":secret},"ttlMs":ttl,"cursor":sub.cursor,
            })).await?;
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
            let current = watch.subscriptions.get_mut(&key).expect("target saved");
            current.id = Some(id);
            current.refresh_before = refresh;
            current.cursor = response["cursor"].as_str().map(str::to_owned);
            watch.last_error = None;
            h.with(|c| c.mail_push_save(agent, &watch)).await?;
            // Subscribe before the bootstrap read, covering mail that arrived
            // before or during callback verification and any replay gap.
            self.mail_push().receive.notify_one();
        }
        watch.last_error = None;
        h.with(|c| c.mail_push_save(agent, &watch)).await
    }
}
