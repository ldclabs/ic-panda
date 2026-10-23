//! Device-local, owner-enabled Mail webhook subscriptions. State is sealed
//! under the account's local data key; callback secrets stay with MCP endpoints.

use crate::{Client, hold::Hold, now_ms};
use agent_protocols::identity::{RequestBinding, RequestJwtClaims};
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const STATE: &str = "agent-mail-push";

/// One upstream mailbox subscription, including an unfinished creation.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Subscription {
    /// Configured Mail relay origin.
    pub service: String,
    /// Owned mailbox ID.
    pub mailbox_id: String,
    /// Server-assigned subscription ID after confirmation.
    pub id: Option<String>,
    /// Next required renewal time, Unix milliseconds.
    pub refresh_before: u64,
    /// Safe upstream replay watermark.
    pub cursor: Option<String>,
}

/// The desired state and recoverable work for one hosted Agent ID.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Watch {
    /// Owner's real-time receive choice.
    pub enabled: bool,
    /// Advances on owner changes, fencing in-flight network calls.
    pub generation: u64,
    /// Dedicated callback endpoint, reused after restarts.
    pub endpoint_id: Option<String>,
    /// Keys are relay origin followed by a slash and mailbox ID.
    pub subscriptions: BTreeMap<String, Subscription>,
    /// A safe diagnostic code, never server bodies or credentials.
    pub last_error: Option<String>,
}

impl Client {
    /// Subscription lifetime bounded by both required grants.
    ///
    /// # Errors
    /// Returns `Forbidden` without a live Mail or endpoint-management grant.
    pub fn mail_push_ttl(&self, agent: &str, service: &str, now: u64) -> Result<u64> {
        let grants = self.capability_live(agent)?;
        let until = |names: &[String]| {
            grants
                .iter()
                .filter(|g| g.capabilities.iter().any(|c| names.contains(c)))
                .map(|g| g.expires_at.unwrap_or(u64::MAX))
                .max()
                .unwrap_or(0)
        };
        let expires = until(&[format!("mail.read:{service}"), "mail.read:*".into()])
            .min(until(&["events.endpoint".into()]));
        ensure(expires > now, Error::Forbidden)?;
        Ok((expires - now).min(86_400_000))
    }

    /// All watches for the selected account. Requires an unlocked device.
    ///
    /// # Errors
    /// Returns key or storage errors.
    pub fn mail_push_watches(&self) -> Result<BTreeMap<String, Watch>> {
        Ok(self
            .workspace
            .private(self.keys()?, STATE)?
            .unwrap_or_default())
    }

    /// Change the owner's receive choice, preserving pending cleanup.
    ///
    /// # Errors
    /// Refuses unknown agents, locked storage or an exhausted generation.
    pub fn mail_push_set(&mut self, agent: &str, enabled: bool) -> Result<Value> {
        let agent = self.agent_find(agent)?.agent_id;
        let mut all = self.mail_push_watches()?;
        let watch = all.entry(agent.clone()).or_default();
        watch.enabled = enabled;
        watch.generation = watch
            .generation
            .checked_add(1)
            .ok_or(Error::QuotaExceeded)?;
        watch.last_error = None;
        self.workspace.put_private(self.keys()?, STATE, &all)?;
        self.mail_push_view(&agent)
    }

    /// Public status without endpoint keys.
    ///
    /// # Errors
    /// Returns key or storage errors.
    pub fn mail_push_view(&self, agent: &str) -> Result<Value> {
        let agent = self.agent_find(agent)?.agent_id;
        let watch = self.mail_push_watches()?.remove(&agent).unwrap_or_default();
        Ok(
            json!({"agent":agent,"enabled":watch.enabled,"endpoint_id":watch.endpoint_id,"subscriptions":watch.subscriptions.values().collect::<Vec<_>>(),"last_error":watch.last_error}),
        )
    }

    /// Save maintenance progress only if the owner has not changed the watch.
    ///
    /// # Errors
    /// Returns `PolicyStale` after a concurrent enable/disable, plus key/storage errors.
    pub fn mail_push_save(&mut self, agent: &str, watch: &Watch) -> Result<()> {
        let mut all = self.mail_push_watches()?;
        ensure(
            all.get(agent)
                .is_some_and(|old| old.generation == watch.generation),
            Error::PolicyStale,
        )?;
        all.insert(agent.to_owned(), watch.clone());
        self.workspace.put_private(self.keys()?, STATE, &all)
    }

    /// Resolve this manager's endpoint, including its secret for signing an
    /// upstream subscription. This value must never be exposed as tool output.
    ///
    /// # Errors
    /// Returns key or storage errors.
    pub fn mail_push_endpoint(&self, agent: &str) -> Result<Option<crate::mcp_events::Endpoint>> {
        Ok(self.mcp_endpoint_records()?.into_iter().find(|e| {
            e.agent.as_deref() == Some(agent)
                && e.state == "active"
                && e.context.get("label").map(String::as_str) == Some("dMsg Mail push")
        }))
    }
}

/// One Mail MCP request. The caller checks the configured origin allowlist;
/// this layer checks the Agent's capability and never holds Client on network I/O.
/// Unsubscribe is cleanup of an existing owner-authorized subscription.
///
/// # Errors
/// Returns scope, session, network, size or protocol errors without logging credentials.
pub async fn request(
    h: &impl Hold,
    http: &reqwest::Client,
    agent: &str,
    service: &str,
    method: &str,
    params: Value,
) -> Result<Value> {
    let token = h
        .with(|c| {
            if method != "events/unsubscribe" {
                ensure(!c.agent_find(agent)?.paused, Error::Forbidden)?;
                c.capability_check(agent, &format!("mail.read:{service}"))?;
                c.capability_check(agent, "events.endpoint")?;
            }
            let signer = c.agent_signer(agent)?;
            let now = (now_ms() / 1000).min(i64::MAX as u64) as i64;
            signer
                .sign_request_jwt(&RequestJwtClaims::new(
                    signer.agent_id(),
                    RequestBinding::new(service),
                    now,
                    60,
                ))
                .map_err(|_| Error::IntegrityFailed)
        })
        .await?;
    let mut response = http
        .post(format!("{service}/mcp"))
        .bearer_auth(token)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .map_err(|_| Error::Unavailable("Mail event service unreachable".into()))?;
    ensure(
        response.status().is_success(),
        Error::Unavailable(format!("Mail event HTTP {}", response.status().as_u16())),
    )?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::Unavailable("Mail event response".into()))?
    {
        ensure(bytes.len() + chunk.len() <= 32 * 1024, Error::QuotaExceeded)?;
        bytes.extend_from_slice(&chunk);
    }
    h.with(|c| {
        if method != "events/unsubscribe" {
            ensure(!c.agent_find(agent)?.paused, Error::Forbidden)?;
            c.capability_check(agent, &format!("mail.read:{service}"))?;
            c.capability_check(agent, "events.endpoint")?;
        }
        Ok(())
    })
    .await?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::IntegrityFailed)?;
    ensure(
        value["jsonrpc"] == "2.0" && value["id"] == 1,
        Error::IntegrityFailed,
    )?;
    if value.get("error").is_some() {
        let detail = value["error"]["data"]["detail"]
            .as_str()
            .filter(|s| s.len() < 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            .unwrap_or("unspecified");
        return Err(Error::Unavailable(format!(
            "Mail event RPC {} ({detail})",
            value["error"]["code"]
        )));
    }
    value.get("result").cloned().ok_or(Error::IntegrityFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_updates_are_fenced_and_leases_do_not_outlive_grants() {
        let dir = crate::testing::TempDir::new();
        let mut client = crate::testing::unlocked(&dir.0, false, None);
        let agent = client.agent_create("mail-watch").unwrap().agent_id;
        client.mail_push_set(&agent, true).unwrap();
        let old = client.mail_push_watches().unwrap().remove(&agent).unwrap();
        client.mail_push_set(&agent, false).unwrap();
        assert_eq!(client.mail_push_save(&agent, &old), Err(Error::PolicyStale));
        assert_eq!(client.mail_push_view(&agent).unwrap()["enabled"], false);
        client
            .capability_grant(
                "cli",
                &agent,
                vec!["mail.read:https://mail.example".into()],
                None,
            )
            .unwrap();
        assert_eq!(
            client.mail_push_ttl(&agent, "https://mail.example", now_ms()),
            Err(Error::Forbidden)
        );
        let grant = client
            .capability_grant("cli", &agent, vec!["events.endpoint".into()], Some(1))
            .unwrap();
        let now = now_ms();
        let expires = grant.expires_at.unwrap();
        assert_eq!(
            client
                .mail_push_ttl(&agent, "https://mail.example", now)
                .unwrap(),
            (expires - now).min(86_400_000)
        );
        assert_eq!(
            client.mail_push_ttl(&agent, "https://other.example", now),
            Err(Error::Forbidden)
        );
        assert_eq!(
            client.mail_push_ttl(&agent, "https://mail.example", expires),
            Err(Error::Forbidden)
        );
    }
}
