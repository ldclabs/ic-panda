//! Device-local, owner-enabled Mail webhook subscriptions. State is sealed
//! per Agent under a purpose-scoped local key. Resident Agents retain only that
//! key in the credential store so renewal can continue while the account is locked.

use crate::{Client, codec, crypto, hold::Hold, now_ms, secure::SecureStorage};
use agent_protocols::identity::{RequestBinding, RequestJwtClaims};
use dmsg_types::{DAY, Error, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

const STATE: &str = "agent-mail-push";
const PREFIX: &str = "agent-mail-push:";

/// Only the callback material needed to renew a Mail subscription.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Callback {
    /// Account-owned callback endpoint ID, reused across renewals.
    pub endpoint_id: String,
    /// Existing relay callback URL.
    pub url: String,
    /// Webhook authentication secret; never include it in status or logs.
    pub secret: String,
}

#[derive(Serialize, Deserialize)]
struct State {
    watch: Watch,
    callback: Option<Callback>,
}

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
    /// Advances on owner or callback changes, fencing in-flight network calls.
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
        Ok((expires - now).min(7 * DAY))
    }

    fn mail_push_context(&self, agent: &str) -> Result<(&'static str, String, String)> {
        Ok((
            "dmsg/agent-mail-push/1",
            self.workspace.name()?,
            agent.into(),
        ))
    }

    fn resident_mail_push_name(&self, agent: &str) -> Result<String> {
        Ok(format!(
            "{}:agent-mail-push:{agent}",
            self.workspace.name()?
        ))
    }

    fn mail_push_key(&self, agent: &str) -> Result<Zeroizing<crypto::Key>> {
        if let Ok(keys) = self.keys() {
            return Ok(keys.derive(&self.mail_push_context(agent)?));
        }
        ensure(self.agent_find(agent)?.resident, Error::Locked)?;
        let bytes = self
            .secure()
            .ok_or(Error::Locked)?
            .get(&self.resident_mail_push_name(agent)?)?
            .ok_or(Error::Locked)?;
        Ok(Zeroizing::new(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        ))
    }

    pub(crate) fn mail_push_keep_resident(&self, agent: &str) -> Result<()> {
        let key = self.keys()?.derive(&self.mail_push_context(agent)?);
        self.secure()
            .ok_or(Error::Locked)?
            .set(&self.resident_mail_push_name(agent)?, key.as_slice())
    }

    pub(crate) fn mail_push_forget_resident(
        &self,
        secure: &dyn SecureStorage,
        agent: &str,
    ) -> Result<()> {
        secure.delete(&self.resident_mail_push_name(agent)?)
    }

    fn mail_push_state(&self, agent: &str) -> Result<Option<State>> {
        let Some(sealed) = self.workspace.store.sealed(&format!("{PREFIX}{agent}"))? else {
            return Ok(None);
        };
        let key = self.mail_push_key(agent)?;
        let bytes = crypto::open(&key, &sealed, &self.mail_push_context(agent)?)?;
        codec::decode(&bytes, codec::MAX_RECORD).map(Some)
    }

    fn mail_push_store(&self, agent: &str, state: &State) -> Result<()> {
        let key = self.mail_push_key(agent)?;
        if self.keys().is_ok() && self.agent_find(agent).is_ok_and(|a| a.resident) {
            self.mail_push_keep_resident(agent)?;
        }
        let sealed = crypto::seal(
            &key,
            &Zeroizing::new(codec::canonical(state)),
            &self.mail_push_context(agent)?,
        );
        self.workspace
            .store
            .set_sealed(&format!("{PREFIX}{agent}"), &sealed)
    }

    // Preserve existing watches when moving the former account-wide record
    // into separately encrypted Agent records. Locked clients wait for unlock.
    fn mail_push_import(&self) -> Result<()> {
        let Ok(keys) = self.keys() else { return Ok(()) };
        let Some(watches) = self
            .workspace
            .private::<BTreeMap<String, Watch>>(keys, STATE)?
        else {
            return Ok(());
        };
        for (agent, watch) in watches {
            if self.mail_push_state(&agent)?.is_none() {
                let callback = self.mail_push_endpoint(&agent)?;
                self.mail_push_store(&agent, &State { watch, callback })?;
            }
        }
        self.workspace.delete_private(STATE)
    }

    /// All accessible watches for the selected account: all while unlocked,
    /// only explicitly resident Agents while locked.
    ///
    /// # Errors
    /// Returns key or storage errors.
    pub fn mail_push_watches(&self) -> Result<BTreeMap<String, Watch>> {
        self.mail_push_import()?;
        let mut watches = BTreeMap::new();
        for name in self.workspace.store.sealed_keys(PREFIX)? {
            let agent = &name[PREFIX.len()..];
            if self.keys().is_err() && !self.agent_find(agent).is_ok_and(|a| a.resident) {
                continue;
            }
            if let Some(state) = self.mail_push_state(agent)? {
                watches.insert(agent.into(), state.watch);
            }
        }
        Ok(watches)
    }

    /// Change the owner's receive choice, preserving pending cleanup.
    ///
    /// # Errors
    /// Refuses unknown agents, locked storage or an exhausted generation.
    pub fn mail_push_set(&mut self, agent: &str, enabled: bool) -> Result<Value> {
        let agent = self.agent_find(agent)?.agent_id;
        self.keys()?;
        self.mail_push_import()?;
        let mut watch = self
            .mail_push_state(&agent)?
            .map(|s| s.watch)
            .unwrap_or_default();
        watch.enabled = enabled;
        watch.generation = watch
            .generation
            .checked_add(1)
            .ok_or(Error::QuotaExceeded)?;
        watch.last_error = None;
        let callback = self.mail_push_endpoint(&agent)?;
        self.mail_push_store(&agent, &State { watch, callback })?;
        self.mail_push_view(&agent)
    }

    /// Public status without endpoint keys.
    ///
    /// # Errors
    /// Returns key or storage errors.
    pub fn mail_push_view(&self, agent: &str) -> Result<Value> {
        let agent = self.agent_find(agent)?.agent_id;
        self.mail_push_import()?;
        self.mail_push_key(&agent)?;
        let watch = self
            .mail_push_state(&agent)?
            .map(|s| s.watch)
            .unwrap_or_default();
        Ok(
            json!({"agent":agent,"enabled":watch.enabled,"endpoint_id":watch.endpoint_id,"subscriptions":watch.subscriptions.values().collect::<Vec<_>>(),"last_error":watch.last_error}),
        )
    }

    /// Save progress only if neither the owner nor the callback changed the watch.
    ///
    /// # Errors
    /// Returns `PolicyStale` after a concurrent watch/callback change, plus key/storage errors.
    pub fn mail_push_save(&mut self, agent: &str, watch: &Watch) -> Result<()> {
        self.mail_push_import()?;
        let mut state = self.mail_push_state(agent)?.ok_or(Error::NotFound)?;
        ensure(
            state.watch.generation == watch.generation,
            Error::PolicyStale,
        )?;
        state.watch = watch.clone();
        self.mail_push_store(agent, &state)
    }

    /// Resolve this manager's endpoint, including its secret for signing an
    /// upstream subscription. This value must never be exposed as tool output.
    ///
    /// # Errors
    /// Returns key or storage errors.
    pub fn mail_push_endpoint(&self, agent: &str) -> Result<Option<Callback>> {
        if self.keys().is_err() {
            return Ok(self.mail_push_state(agent)?.and_then(|s| s.callback));
        }
        Self::mail_push_callback(&self.mcp_endpoint_records()?, agent)
    }

    fn mail_push_callback(
        endpoints: &[crate::mcp_events::Endpoint],
        agent: &str,
    ) -> Result<Option<Callback>> {
        endpoints
            .iter()
            .find(|e| {
                e.agent.as_deref() == Some(agent)
                    && e.state == "active"
                    && e.context.get("label").map(String::as_str) == Some("dMsg Mail push")
            })
            .map(|e| {
                Ok(Callback {
                    endpoint_id: e.endpoint_id.clone(),
                    url: e.url.clone(),
                    secret: e
                        .secrets
                        .last()
                        .ok_or(Error::IntegrityFailed)?
                        .secret
                        .clone(),
                })
            })
            .transpose()
    }

    /// Keep the scoped callback in step with creation, rotation and deletion.
    pub(crate) fn mail_push_endpoints_changed(
        &self,
        endpoints: &[crate::mcp_events::Endpoint],
    ) -> Result<()> {
        for name in self.workspace.store.sealed_keys(PREFIX)? {
            let agent = &name[PREFIX.len()..];
            let Some(mut state) = self.mail_push_state(agent)? else {
                continue;
            };
            let callback = Self::mail_push_callback(endpoints, agent)?;
            if state.callback != callback {
                state.callback = callback;
                state.watch.generation = state
                    .watch
                    .generation
                    .checked_add(1)
                    .ok_or(Error::QuotaExceeded)?;
                for sub in state.watch.subscriptions.values_mut() {
                    sub.refresh_before = 0;
                }
                self.mail_push_store(agent, &state)?;
            }
        }
        Ok(())
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
        client
            .capability_grant("cli", &agent, vec!["events.endpoint".into()], None)
            .unwrap();
        assert_eq!(
            client
                .mail_push_ttl(&agent, "https://mail.example", now_ms())
                .unwrap(),
            7 * DAY
        );
    }

    #[test]
    fn resident_watches_survive_lock_restart_and_unlock_without_account_keys() {
        use crate::testing::{MemoryStorage, TempDir, config, mail_push_callback, unlocked};
        use std::sync::Arc;
        let dir = TempDir::new();
        let secure = Arc::new(MemoryStorage::default());
        let mut client = unlocked(&dir.0, false, Some(secure.clone()));
        let agent = client.agent_create("resident").unwrap().agent_id;
        let ordinary = client.agent_create("ordinary").unwrap().agent_id;
        client.agent_resident(&agent, true).unwrap();
        for id in [&agent, &ordinary] {
            client.mail_push_set(id, true).unwrap();
        }
        let grant = client
            .capability_grant(
                "cli",
                &agent,
                vec!["mail.read:*".into(), "events.endpoint".into()],
                None,
            )
            .unwrap();
        let callback = mail_push_callback(&mut client, &agent);
        client.lock();
        assert!(client.keys().is_err());
        assert!(
            !client.session_state().resident,
            "no account device key is retained"
        );
        assert_eq!(client.mail_push_watches().unwrap().len(), 1);
        assert!(matches!(
            client.mail_push_view(&ordinary),
            Err(Error::Locked)
        ));
        assert!(client.mail_push_endpoint(&agent).unwrap() == Some(callback.clone()));
        assert_eq!(
            client
                .mail_push_ttl(&agent, "https://mail.example", now_ms())
                .unwrap(),
            7 * DAY
        );
        let mut watch = client.mail_push_watches().unwrap().remove(&agent).unwrap();
        watch.subscriptions.insert(
            "mailbox".into(),
            Subscription {
                id: Some("lease".into()),
                refresh_before: now_ms() + 7 * DAY,
                cursor: Some("renewed".into()),
                ..Default::default()
            },
        );
        client.mail_push_save(&agent, &watch).unwrap();
        drop(client);
        let mut client = Client::open(config(false), &dir.0, Some(secure.clone())).unwrap();
        assert!(client.keys().is_err());
        assert_eq!(
            client.mail_push_view(&agent).unwrap()["subscriptions"][0]["cursor"],
            "renewed"
        );
        assert!(client.mail_push_endpoint(&agent).unwrap() == Some(callback));
        client.keys = Some(client.workspace.unlock(&crate::testing::SECRET).unwrap());
        assert_eq!(
            client.mail_push_view(&agent).unwrap()["subscriptions"][0]["cursor"],
            "renewed"
        );
        client.capability_revoke("cli", &grant.id).unwrap();
        client.lock();
        assert_eq!(
            client.mail_push_ttl(&agent, "https://mail.example", now_ms()),
            Err(Error::Forbidden)
        );
        client.keys = Some(client.workspace.unlock(&crate::testing::SECRET).unwrap());
        client.agent_resident(&agent, false).unwrap();
        assert!(
            secure
                .get(&client.resident_mail_push_name(&agent).unwrap())
                .unwrap()
                .is_none()
        );
        client.lock();
        assert!(client.mail_push_watches().unwrap().is_empty());
        assert!(matches!(
            client.mail_push_endpoint(&agent),
            Err(Error::Locked)
        ));
    }

    #[test]
    fn imports_existing_watches_and_tracks_callback_rotation_and_deletion() {
        let dir = crate::testing::TempDir::new();
        let mut client = crate::testing::unlocked(&dir.0, false, None);
        let agent = client.agent_create("mail-watch").unwrap().agent_id;
        let original = Watch {
            enabled: true,
            generation: 4,
            ..Default::default()
        };
        client
            .workspace
            .put_private(
                client.keys().unwrap(),
                STATE,
                &BTreeMap::from([(agent.clone(), original)]),
            )
            .unwrap();
        crate::testing::mail_push_callback(&mut client, &agent);
        let mut watch = client.mail_push_watches().unwrap().remove(&agent).unwrap();
        assert!(watch.enabled);
        assert_eq!(watch.generation, 4);
        assert!(client.workspace.store.sealed(STATE).unwrap().is_none());
        watch.subscriptions.insert(
            "box".into(),
            Subscription {
                refresh_before: now_ms() + 7 * DAY,
                ..Default::default()
            },
        );
        client.mail_push_save(&agent, &watch).unwrap();
        let mut endpoints = client.mcp_endpoint_records().unwrap();
        endpoints[0].secrets[0].secret = "rotated-secret".into();
        client.mail_push_endpoints_changed(&endpoints).unwrap();
        let state = client.mail_push_state(&agent).unwrap().unwrap();
        assert_eq!(state.callback.unwrap().secret, "rotated-secret");
        assert_eq!(state.watch.subscriptions["box"].refresh_before, 0);
        // Maintenance may have read the watch before rotation and resolve the
        // new callback afterwards. Its old lease must not erase the renewal.
        assert_eq!(
            client.mail_push_save(&agent, &watch),
            Err(Error::PolicyStale)
        );
        assert_eq!(
            client
                .mail_push_state(&agent)
                .unwrap()
                .unwrap()
                .watch
                .subscriptions["box"]
                .refresh_before,
            0
        );
        endpoints[0].state = "deleted".into();
        client.mail_push_endpoints_changed(&endpoints).unwrap();
        assert_eq!(
            client.mail_push_save(&agent, &state.watch),
            Err(Error::PolicyStale)
        );
        assert!(
            client
                .mail_push_state(&agent)
                .unwrap()
                .unwrap()
                .callback
                .is_none()
        );
    }
}
