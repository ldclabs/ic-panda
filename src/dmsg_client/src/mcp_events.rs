//! MCP Events relay endpoints (plan §6.11). A local agent has no public
//! webhook address, so the relay gives this device callback URLs: an
//! upstream MCP server delivers Standard Webhooks there, the relay checks
//! the `v1` signature, seals each delivery to this device's HPKE key and
//! keeps it until the device fetches it.
//!
//! The device opens a delivery, checks `v1` again with its own copy of the
//! secret and, when the upstream server publishes keys, the end-to-end
//! `v1a` signature, then keeps it sealed under the LDK until the agent
//! acknowledges it. A delivery whose `v1a` fails is dropped: the relay
//! holds the `v1` secret and could have forged it.
//!
//! Endpoints, their secrets and the agents' context stay on this device;
//! nothing here syncs.

use crate::client::Client;
use crate::codec::{canonical, decode};
use crate::crypto::{hpke_open, random_key, HpkeEnvelope};
use crate::now_ms;
use base64::{engine::general_purpose::STANDARD, Engine};
use dmsg_types::{ensure, Error, Hash, Result, DAY, MINUTE};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_json::{json, Value as Json};
use sha2::Sha256;
use std::collections::BTreeMap;

/// Create an endpoint with its first secret.
pub const CREATE_ACTION: &str = "dmsg/mcp-events/endpoint/create/v1";
/// Add a secret; the replaced ones keep verifying for a day.
pub const ROTATE_ACTION: &str = "dmsg/mcp-events/endpoint/rotate/v1";
/// Delete an endpoint; the relay then answers 410.
pub const DELETE_ACTION: &str = "dmsg/mcp-events/endpoint/delete/v1";
/// Let the relay forget the records fetched so far.
pub const ACK_ACTION: &str = "dmsg/mcp-events/ack/v1";
/// The one event type `dmsg mcp` serves.
pub const EVENT_NAME: &str = "dmsg.relay.delivery";
/// HPKE info and AAD domain of a relay record.
const INFO: &str = "dmsg/mcp-event/1";
/// The endpoints of this device, sealed under the LDK.
const ENDPOINTS: &str = "mcp-endpoints";
/// Creations whose outcome is not known yet, sealed under the LDK.
const CREATING: &str = "mcp-endpoints-creating";
/// A creation is tried this many times before its reply counts as lost.
const CREATE_TRIES: usize = 3;
/// A lost creation is settled for this long, then forgotten.
const CREATING_MS: u64 = 7 * DAY;
/// At most this many live endpoints per account (the relay's bound).
pub const MAX_ENDPOINTS: usize = 16;
/// Local records are kept this long, and at most [`KEEP`] of them.
const RETAIN_MS: u64 = 30 * DAY;
const KEEP: u64 = 10_000;
/// A replaced secret keeps verifying this long.
const OVERLAP_MS: u64 = DAY;
/// Standard Webhooks tolerance between the signed and the received time.
const SKEW_MS: u64 = 5 * MINUTE;
/// `refresh_due` comes at most this long before `refreshBefore`.
const REFRESH_LEAD_MS: u64 = 10 * MINUTE;
const MAX_TEXT: usize = 256;
const MAX_INSTRUCTIONS: usize = 4096;
/// A sealed relay record: a delivery of at most 256 KiB and its framing.
const MAX_SEALED: usize = 512 * 1024;

/// A webhook secret and when it stops verifying.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Secret {
    /// `whsec_` and the base64 of 32 random bytes.
    pub secret: String,
    /// Unix milliseconds.
    pub created_at: u64,
    /// When a rotation retired it, in Unix milliseconds.
    pub retire_at: Option<u64>,
}

/// The upstream subscription an agent registered for an endpoint.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Binding {
    /// The upstream MCP server's URL; its origin may publish `v1a` keys.
    pub server: Option<String>,
    /// The upstream subscription ID.
    pub subscription_id: Option<String>,
    /// The upstream's `refreshBefore`, in Unix milliseconds.
    pub refresh_before: Option<u64>,
    /// When it was registered, in Unix milliseconds.
    pub bound_at: u64,
    /// The `refreshBefore` a `refresh_due` record was written for.
    pub due_for: Option<u64>,
}

/// One callback endpoint of this device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// The callback ID, 32 hex characters.
    pub endpoint_id: String,
    /// The callback URL the upstream delivers to.
    pub url: String,
    /// Live and retired secrets, newest last.
    pub secrets: Vec<Secret>,
    /// The Agent ID that created it; `None` for the owner.
    pub agent: Option<String>,
    /// The MCP session it is bound to, if any.
    pub mcp_session: Option<String>,
    /// The agent's opaque `{agent, session, label, instructions}`.
    pub context: BTreeMap<String, String>,
    /// The registered upstream subscription.
    pub binding: Binding,
    /// `active`, `suspended` or `deleted`.
    pub state: String,
    /// The relay records fetched so far.
    pub relay_seq: u64,
    /// Unix milliseconds.
    pub created_at: u64,
    /// When it left `active`, in Unix milliseconds.
    pub ended_at: Option<u64>,
}

/// An endpoint creation as first submitted, kept until its outcome is
/// known: the relay replays a command with the same request ID and payload,
/// so a resubmission learns whether the lost one created the endpoint.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct Creating {
    callback_id: String,
    secret: String,
    request_id: String,
    created_at: u64,
}

/// Which endpoints a reader means.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// The calling Agent ID; `None` for the owner, who sees every endpoint.
    pub agent: Option<String>,
    /// Explicit endpoints, each the caller's own.
    pub endpoint_ids: Vec<String>,
    /// Endpoints whose `context.session` is this.
    pub session: Option<String>,
    /// Without the two above, the endpoints created in or bound to this
    /// MCP session.
    pub mcp_session: Option<String>,
}

/// What one fetch pass did.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct FetchReport {
    /// Relay records read.
    pub fetched: u64,
    /// New local records.
    pub stored: u64,
    /// Deliveries dropped because their `v1a` signature failed.
    pub rejected: u64,
    /// Endpoints left for a later pass, with the error that held them.
    pub deferred: Vec<String>,
}

#[derive(Serialize)]
struct CreateCommand<'a> {
    callback_id: &'a ByteBuf,
    secret: &'a str,
}

#[derive(Serialize)]
struct DeleteCommand<'a> {
    callback_id: &'a ByteBuf,
}

#[derive(Serialize)]
struct AckCommand<'a> {
    callback_id: &'a ByteBuf,
    through_seq: u64,
}

/// A relay record as sealed: `{enc, ciphertext}`.
#[derive(Serialize, Deserialize)]
struct Sealed {
    enc: ByteBuf,
    ciphertext: ByteBuf,
}

/// What the relay sealed: the delivery as it arrived.
#[derive(Serialize, Deserialize)]
struct Delivery {
    headers: BTreeMap<String, String>,
    body: ByteBuf,
    received_at: u64,
}

/// A local record before sealing.
#[derive(Serialize, Deserialize)]
struct Record {
    kind: String,
    received_at: u64,
    subscription_id: Option<String>,
    upstream: Json,
    verified: Option<Verified>,
    detail: Json,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
struct Verified {
    v1: bool,
    v1a: Option<bool>,
}

/// Whether a relay refusal of a creation is final: it could not have
/// created the endpoint, then or on a resubmission. Any other failure, such
/// as a lost reply or stale device evidence, leaves the outcome open.
fn refused(error: &Error) -> bool {
    matches!(
        error,
        Error::QuotaExceeded | Error::IdempotencyConflict | Error::InvalidInput(_)
    )
}

fn new_secret() -> String {
    format!("whsec_{}", STANDARD.encode(random_key()))
}

fn callback_bytes(id: &str) -> Result<ByteBuf> {
    let bytes = hex::decode(id).map_err(|_| Error::InvalidInput("endpoint_id".into()))?;
    ensure(bytes.len() == 16, Error::InvalidInput("endpoint_id".into()))?;
    Ok(ByteBuf::from(bytes))
}

/// Check an agent's endpoint context: `agent`, `session` and `label` of at
/// most 256 bytes, `instructions` of at most 4 KiB, nothing else.
///
/// # Errors
/// Returns `Error::InvalidInput` otherwise.
pub fn check_context(context: &BTreeMap<String, String>) -> Result<()> {
    for (key, value) in context {
        let bound = match key.as_str() {
            "agent" | "session" | "label" => MAX_TEXT,
            "instructions" => MAX_INSTRUCTIONS,
            _ => return Err(Error::InvalidInput(format!("context.{key}"))),
        };
        ensure(
            value.len() <= bound,
            Error::InvalidInput(format!("context.{key}")),
        )?;
    }
    Ok(())
}

/// The origin of an upstream MCP server URL, where its `v1a` keys are
/// published: HTTPS, or loopback HTTP in a local deployment.
///
/// # Errors
/// Returns `Error::InvalidInput` for anything else.
pub fn server_origin(server: &str, local: bool) -> Result<String> {
    let url = url::Url::parse(server).map_err(|_| Error::InvalidInput("server".into()))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    ensure(
        url.scheme() == "https" || (local && loopback && url.scheme() == "http"),
        Error::InvalidInput("server".into()),
    )?;
    Ok(url.origin().ascii_serialization())
}

/// The Ed25519 keys of a JWKS (`kty: OKP`, `crv: Ed25519`); others are ignored.
pub fn jwks_keys(jwks: &Json) -> Vec<Hash> {
    let Some(keys) = jwks["keys"].as_array() else {
        return Vec::new();
    };
    keys.iter()
        .filter(|k| k["kty"] == "OKP" && k["crv"] == "Ed25519")
        .filter_map(|k| crate::codec::unb64(k["x"].as_str()?).ok())
        .filter_map(|x| <[u8; 32]>::try_from(x).ok().map(Hash::new))
        .collect()
}

/// `id.timestamp.body`, what both signatures cover.
fn signed_content(id: &str, timestamp: &str, body: &[u8]) -> Vec<u8> {
    let mut content = format!("{id}.{timestamp}.").into_bytes();
    content.extend_from_slice(body);
    content
}

/// The `v1` HMAC-SHA256 of a delivery under one secret, base64.
///
/// # Errors
/// A secret that is not `whsec_` and base64 returns `Error::InvalidInput`.
pub fn webhook_signature(secret: &str, id: &str, timestamp: &str, body: &[u8]) -> Result<String> {
    let key = secret
        .strip_prefix("whsec_")
        .and_then(|b| STANDARD.decode(b).ok())
        .ok_or_else(|| Error::InvalidInput("secret".into()))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("HMAC takes any key length");
    mac.update(&signed_content(id, timestamp, body));
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

/// Check a delivery's signatures: `v1` with the secrets live when it was
/// received and within five minutes of its timestamp, `v1a` with the
/// upstream's keys when they are known. `None` means a `v1a` that fails.
fn verify(secrets: &[Secret], delivery: &Delivery, keys: Option<&[Hash]>) -> Option<Verified> {
    let header = |name: &str| delivery.headers.get(name).map(String::as_str);
    let (Some(id), Some(timestamp), Some(signature)) = (
        header("webhook-id"),
        header("webhook-timestamp"),
        header("webhook-signature"),
    ) else {
        return Some(Verified {
            v1: false,
            v1a: keys.map(|_| false),
        });
    };
    let signed_at = timestamp.parse::<u64>().ok().map(|s| s * 1000);
    let fresh = signed_at.is_some_and(|at| at.abs_diff(delivery.received_at) <= SKEW_MS);
    let offered: Vec<(&str, &str)> = signature
        .split(' ')
        .filter_map(|s| s.split_once(','))
        .collect();
    let v1 = fresh
        && secrets
            .iter()
            .filter(|s| s.retire_at.is_none_or(|at| at > delivery.received_at))
            .filter_map(|s| webhook_signature(&s.secret, id, timestamp, &delivery.body).ok())
            .any(|expected| {
                offered.iter().any(|(v, sig)| {
                    *v == "v1"
                        && sig.len() == expected.len()
                        && sig
                            .bytes()
                            .zip(expected.bytes())
                            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                            == 0
                })
            });
    let asymmetric: Vec<Vec<u8>> = offered
        .iter()
        .filter(|(v, _)| *v == "v1a")
        .filter_map(|(_, sig)| STANDARD.decode(sig).ok())
        .collect();
    let v1a = match keys {
        None => None,
        Some(_) if asymmetric.is_empty() => Some(false),
        Some(keys) => {
            let content = signed_content(id, timestamp, &delivery.body);
            let valid = keys.iter().any(|key| {
                asymmetric
                    .iter()
                    .any(|sig| dmsg_protocol::verify(key, &content, sig).is_ok())
            });
            if !valid {
                return None;
            }
            Some(true)
        }
    };
    Some(Verified { v1, v1a })
}

/// Unix milliseconds as an RFC 3339 UTC timestamp, as MCP events carry it.
pub fn timestamp(ms: u64) -> String {
    let secs = ms / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        ms % 1000
    )
}

impl Client {
    fn endpoints(&self) -> Result<Vec<Endpoint>> {
        Ok(self
            .workspace
            .private(self.keys()?, ENDPOINTS)?
            .unwrap_or_default())
    }

    fn save_endpoints(&self, endpoints: &[Endpoint]) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, ENDPOINTS, &endpoints)
    }

    /// The caller's endpoint `id`; another agent's is `NotFound`.
    fn endpoint_of(endpoints: &[Endpoint], agent: Option<&str>, id: &str) -> Result<usize> {
        endpoints
            .iter()
            .position(|e| e.endpoint_id == id && (agent.is_none() || e.agent.as_deref() == agent))
            .ok_or(Error::NotFound)
    }

    /// The endpoint IDs a filter selects.
    fn selected(&self, filter: &Filter) -> Result<Vec<String>> {
        let endpoints = self.endpoints()?;
        let own: Vec<&Endpoint> = endpoints
            .iter()
            .filter(|e| filter.agent.is_none() || e.agent == filter.agent)
            .collect();
        if !filter.endpoint_ids.is_empty() {
            return filter
                .endpoint_ids
                .iter()
                .map(|id| {
                    own.iter()
                        .find(|e| &e.endpoint_id == id)
                        .map(|e| e.endpoint_id.clone())
                        .ok_or(Error::NotFound)
                })
                .collect();
        }
        Ok(own
            .into_iter()
            .filter(|e| match (&filter.session, &filter.mcp_session) {
                (Some(session), _) => e.context.get("session") == Some(session),
                (None, Some(mcp)) => e.mcp_session.as_ref() == Some(mcp),
                (None, None) => true,
            })
            .map(|e| e.endpoint_id.clone())
            .collect())
    }

    fn endpoint_view(endpoint: &Endpoint) -> Json {
        json!({
            "endpoint_id": endpoint.endpoint_id,
            "url": endpoint.url,
            "state": endpoint.state,
            "agent": endpoint.agent,
            "context": endpoint.context,
            "created_at": endpoint.created_at,
            "ended_at": endpoint.ended_at,
            "binding": {
                "server": endpoint.binding.server,
                "subscription_id": endpoint.binding.subscription_id,
                "refresh_before": endpoint.binding.refresh_before,
            },
        })
    }

    fn creating(&self) -> Result<Vec<Creating>> {
        Ok(self
            .workspace
            .private(self.keys()?, CREATING)?
            .unwrap_or_default())
    }

    fn save_creating(&self, creating: &[Creating]) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, CREATING, &creating)
    }

    /// Submit a creation, again on a lost reply up to [`CREATE_TRIES`]
    /// times; each try is the same command, so at most one endpoint results.
    async fn submit_create(&mut self, creating: &Creating) -> Result<Json> {
        let path = format!("{}/mcp-events/endpoints", self.base()?);
        let callback = callback_bytes(&creating.callback_id)?;
        let request_id = Hash::new(
            hex::decode(&creating.request_id)
                .ok()
                .and_then(|b| b.try_into().ok())
                .ok_or(Error::IntegrityFailed)?,
        );
        let mut tries = 0;
        loop {
            tries += 1;
            let outcome = self
                .relay_post(
                    &path,
                    CREATE_ACTION,
                    &CreateCommand {
                        callback_id: &callback,
                        secret: &creating.secret,
                    },
                    &request_id,
                )
                .await;
            match outcome {
                Err(error) if !refused(&error) && tries < CREATE_TRIES => continue,
                outcome => return outcome,
            }
        }
    }

    /// Create a callback endpoint on the relay for `agent` (`None` for the
    /// owner): a fresh callback ID and `whsec_` secret, returned once.
    ///
    /// The creation is kept before it is sent. When its reply stays lost the
    /// caller gets `Error::ExecutionUnknown` and never the secret, so a later
    /// fetch pass resubmits it and deletes whatever it created; no endpoint
    /// is left on the relay that nobody knows.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked, `Error::QuotaExceeded` at
    /// [`MAX_ENDPOINTS`] live endpoints, and relay errors.
    pub async fn mcp_endpoint_create(
        &mut self,
        agent: Option<&str>,
        mcp_session: Option<&str>,
        context: BTreeMap<String, String>,
    ) -> Result<Json> {
        check_context(&context)?;
        let mut creating = self.creating()?;
        ensure(
            self.endpoints()?
                .iter()
                .filter(|e| e.state == "active")
                .count()
                + creating.len()
                < MAX_ENDPOINTS,
            Error::QuotaExceeded,
        )?;
        let now = now_ms();
        let pending = Creating {
            callback_id: hex::encode(&random_key()[..16]),
            secret: new_secret(),
            request_id: hex::encode(crate::crypto::random_id()),
            created_at: now,
        };
        creating.push(pending.clone());
        self.save_creating(&creating)?;
        let outcome = self.submit_create(&pending).await;
        let created = match outcome {
            // Kept for the next fetch pass to settle.
            Err(error) if !refused(&error) => return Err(error),
            outcome => {
                creating.retain(|c| c != &pending);
                self.save_creating(&creating)?;
                outcome?
            }
        };
        let id = pending.callback_id;
        let url = created["url"]
            .as_str()
            .filter(|u| u.ends_with(&format!("/mcp-events/{id}")))
            .ok_or(Error::IntegrityFailed)?;
        let endpoint = Endpoint {
            endpoint_id: id,
            url: url.to_string(),
            secrets: vec![Secret {
                secret: pending.secret.clone(),
                created_at: now,
                retire_at: None,
            }],
            agent: agent.map(str::to_string),
            mcp_session: mcp_session.map(str::to_string),
            context,
            binding: Binding::default(),
            state: "active".into(),
            relay_seq: 0,
            created_at: now,
            ended_at: None,
        };
        let mut view = Self::endpoint_view(&endpoint);
        view["secret"] = pending.secret.into();
        let mut endpoints = self.endpoints()?;
        endpoints.push(endpoint);
        self.save_endpoints(&endpoints)?;
        Ok(view)
    }

    /// Settle the creations whose reply was lost: resubmit each, and delete
    /// the endpoint it turns out to have created, since its caller never
    /// got the secret. Returns how many are still open.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub async fn mcp_endpoint_settle(&mut self) -> Result<usize> {
        let creating = self.creating()?;
        if creating.is_empty() {
            return Ok(0);
        }
        let now = now_ms();
        let mut open = Vec::new();
        for pending in creating {
            let settled = match self.submit_create(&pending).await {
                Ok(_) => {
                    let path = format!(
                        "{}/mcp-events/endpoints/{}/delete",
                        self.base()?,
                        pending.callback_id
                    );
                    let deleted = self
                        .relay_post(
                            &path,
                            DELETE_ACTION,
                            &DeleteCommand {
                                callback_id: &callback_bytes(&pending.callback_id)?,
                            },
                            &Hash::new(crate::crypto::random_id()),
                        )
                        .await;
                    matches!(deleted, Ok(_) | Err(Error::NotFound))
                }
                Err(error) => refused(&error),
            };
            if !settled && pending.created_at + CREATING_MS > now {
                open.push(pending);
            }
        }
        self.save_creating(&open)?;
        Ok(open.len())
    }

    /// The caller's endpoints, with the relay's view of each when it answers.
    /// The owner also sees the creations still settling, and the endpoints
    /// of this device on the relay that it never handed out (`unknown`): they
    /// hold quota and receive nothing until a fetch pass settles them.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub async fn mcp_endpoint_list(&mut self, agent: Option<&str>) -> Result<Json> {
        let endpoints = self.endpoints()?;
        let relay = match self
            .relay_get(&format!("{}/mcp-events/endpoints", self.base()?))
            .await
        {
            Ok(list) => list["endpoints"].as_array().cloned().unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        let listed: Vec<Json> = endpoints
            .iter()
            .filter(|e| agent.is_none() || e.agent.as_deref() == agent)
            .map(|e| {
                let mut view = Self::endpoint_view(e);
                if let Some(r) = relay.iter().find(|r| r["callback_id"] == e.endpoint_id) {
                    view["relay"] = json!({
                        "state": r["state"],
                        "verified_at": r["verified_at"],
                        "subscription_id": r["subscription_id"],
                        "pending": r["pending"],
                        "last_delivery_at": r["last_delivery_at"],
                    });
                }
                view["local_pending"] = self
                    .workspace
                    .store
                    .count_mcp_events(&e.endpoint_id)
                    .unwrap_or_default()
                    .into();
                view
            })
            .collect();
        if agent.is_some() {
            return Ok(json!({"endpoints": listed}));
        }
        let device = hex::encode(*self.workspace.meta.device_id);
        let unknown: Vec<Json> = relay
            .iter()
            .filter(|r| r["device"] == device.as_str() && r["state"] == "active")
            .filter(|r| !endpoints.iter().any(|e| r["callback_id"] == e.endpoint_id))
            .map(|r| json!({"callback_id": r["callback_id"], "created_at": r["created_at"]}))
            .collect();
        Ok(json!({
            "endpoints": listed,
            "settling": self.creating()?.len(),
            "unknown": unknown,
        }))
    }

    /// Give an endpoint a new secret, returned once; the replaced ones keep
    /// verifying for a day so the upstream can renew with the new one.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for another caller's endpoint and relay errors.
    pub async fn mcp_endpoint_rotate(&mut self, agent: Option<&str>, id: &str) -> Result<Json> {
        let mut endpoints = self.endpoints()?;
        let index = Self::endpoint_of(&endpoints, agent, id)?;
        ensure(endpoints[index].state == "active", Error::Expired)?;
        let secret = new_secret();
        let path = format!("{}/mcp-events/endpoints/{id}/rotate", self.base()?);
        self.relay_post(
            &path,
            ROTATE_ACTION,
            &CreateCommand {
                callback_id: &callback_bytes(id)?,
                secret: &secret,
            },
            &Hash::new(crate::crypto::random_id()),
        )
        .await?;
        let now = now_ms();
        let endpoint = &mut endpoints[index];
        for old in &mut endpoint.secrets {
            old.retire_at = Some(old.retire_at.unwrap_or(u64::MAX).min(now + OVERLAP_MS));
        }
        endpoint
            .secrets
            .retain(|s| s.retire_at.is_none_or(|at| at > now));
        endpoint.secrets.push(Secret {
            secret: secret.clone(),
            created_at: now,
            retire_at: None,
        });
        self.save_endpoints(&endpoints)?;
        Ok(json!({"endpoint_id": id, "secret": secret, "previous_valid_until": now + OVERLAP_MS}))
    }

    /// Delete an endpoint: the relay answers 410 from now on, and the
    /// endpoint's readers get a final `terminated` record.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for another caller's endpoint and relay errors.
    pub async fn mcp_endpoint_delete(&mut self, agent: Option<&str>, id: &str) -> Result<Json> {
        let mut endpoints = self.endpoints()?;
        let index = Self::endpoint_of(&endpoints, agent, id)?;
        let path = format!("{}/mcp-events/endpoints/{id}/delete", self.base()?);
        match self
            .relay_post(
                &path,
                DELETE_ACTION,
                &DeleteCommand {
                    callback_id: &callback_bytes(id)?,
                },
                &Hash::new(crate::crypto::random_id()),
            )
            .await
        {
            Ok(_) | Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
        self.end_endpoint(&mut endpoints[index], "deleted", now_ms())?;
        self.save_endpoints(&endpoints)?;
        Ok(json!({"endpoint_id": id, "state": "deleted"}))
    }

    /// Mark an endpoint ended and tell its readers with a `terminated` record.
    fn end_endpoint(&self, endpoint: &mut Endpoint, state: &str, now: u64) -> Result<()> {
        if endpoint.state != "active" {
            return Ok(());
        }
        endpoint.state = state.into();
        endpoint.ended_at = Some(now);
        endpoint.secrets.clear();
        self.store_record(
            &endpoint.endpoint_id,
            &format!("terminated:{state}"),
            &Record {
                kind: "terminated".into(),
                received_at: now,
                subscription_id: endpoint.binding.subscription_id.clone(),
                upstream: Json::Null,
                verified: None,
                detail: json!({"reason": format!("endpoint_{state}")}),
            },
        )
        .map(|_| ())
    }

    /// Register the upstream subscription behind an endpoint, and bind the
    /// endpoint to the caller's MCP session; a `session` replaces
    /// `context.session`.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for another caller's endpoint and
    /// `Error::InvalidInput` for a server that is not HTTPS.
    #[allow(clippy::too_many_arguments)]
    pub fn mcp_endpoint_bind(
        &mut self,
        agent: Option<&str>,
        mcp_session: Option<&str>,
        id: &str,
        server: Option<String>,
        subscription_id: Option<String>,
        refresh_before: Option<u64>,
        session: Option<String>,
    ) -> Result<Json> {
        let local = self.config.environment == dmsg_types::Environment::Local;
        if let Some(server) = &server {
            ensure(server.len() <= 2048, Error::InvalidInput("server".into()))?;
            server_origin(server, local)?;
        }
        ensure(
            subscription_id.as_ref().is_none_or(|s| s.len() <= MAX_TEXT),
            Error::InvalidInput("subscription_id".into()),
        )?;
        let mut endpoints = self.endpoints()?;
        let index = Self::endpoint_of(&endpoints, agent, id)?;
        let endpoint = &mut endpoints[index];
        endpoint.binding = Binding {
            server,
            subscription_id,
            refresh_before,
            bound_at: now_ms(),
            due_for: None,
        };
        if let Some(mcp) = mcp_session {
            endpoint.mcp_session = Some(mcp.to_string());
        }
        if let Some(session) = session {
            endpoint.context.insert("session".into(), session);
            check_context(&endpoint.context)?;
        }
        let view = Self::endpoint_view(endpoint);
        self.save_endpoints(&endpoints)?;
        Ok(view)
    }

    /// The origins of the bound upstream servers, whose `v1a` keys a fetch
    /// pass checks against.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn mcp_servers(&self) -> Result<Vec<String>> {
        let local = self.config.environment == dmsg_types::Environment::Local;
        let mut origins: Vec<String> = self
            .endpoints()?
            .iter()
            .filter(|e| e.state == "active")
            .filter_map(|e| e.binding.server.as_deref())
            .filter_map(|s| server_origin(s, local).ok())
            .collect();
        origins.sort();
        origins.dedup();
        Ok(origins)
    }

    /// Seal a local record under the LDK and store it once.
    fn store_record(&self, endpoint: &str, webhook_id: &str, record: &Record) -> Result<bool> {
        let name = format!("mcp-event:{endpoint}:{webhook_id}");
        let sealed = self
            .workspace
            .seal_record(self.keys()?, &name, &canonical(record));
        self.workspace.store.put_mcp_event(
            endpoint,
            webhook_id,
            &record.kind,
            record.received_at,
            &sealed,
        )
    }

    /// Fetch, open, check and store what the relay holds for every live
    /// endpoint, then let the relay forget it. `jwks` maps a server origin
    /// to the `v1a` keys it publishes.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked; an endpoint the relay refuses is
    /// reported in [`FetchReport::deferred`].
    pub async fn mcp_events_fetch(
        &mut self,
        jwks: &BTreeMap<String, Vec<Hash>>,
    ) -> Result<FetchReport> {
        // Creations whose reply was lost are settled on every pass.
        self.mcp_endpoint_settle().await?;
        let mut endpoints = self.endpoints()?;
        let account = self.account()?.id.to_string();
        let device = hex::encode(*self.workspace.meta.device_id);
        let local = self.config.environment == dmsg_types::Environment::Local;
        let mut report = FetchReport::default();
        for index in 0..endpoints.len() {
            if endpoints[index].state != "active" {
                continue;
            }
            let id = endpoints[index].endpoint_id.clone();
            let keys = endpoints[index]
                .binding
                .server
                .as_deref()
                .and_then(|s| server_origin(s, local).ok())
                .and_then(|origin| jwks.get(&origin))
                .filter(|keys| !keys.is_empty());
            loop {
                let after = endpoints[index].relay_seq;
                let path = format!(
                    "{}/mcp-events?after={after}&endpoint={id}&limit=100",
                    self.base()?
                );
                let page = match self.relay_get(&path).await {
                    Ok(page) => page,
                    Err(error) => {
                        report.deferred.push(format!("{id}: {error}"));
                        break;
                    }
                };
                let records = page["events"].as_array().cloned().unwrap_or_default();
                for record in &records {
                    report.fetched += 1;
                    // A record that does not open or verify is dropped: only
                    // the relay could have made it so.
                    match self.store_relay_record(
                        &endpoints[index],
                        record,
                        &account,
                        &device,
                        keys.map(Vec::as_slice),
                    ) {
                        Ok(Some(true)) => report.stored += 1,
                        Ok(Some(false)) => {}
                        Ok(None) | Err(Error::IntegrityFailed) => report.rejected += 1,
                        Err(error) => return Err(error),
                    }
                }
                let cursor = page["cursor"].as_u64().unwrap_or(after);
                if cursor > after {
                    endpoints[index].relay_seq = cursor;
                    self.save_endpoints(&endpoints)?;
                    let path = format!("{}/mcp-events/ack", self.base()?);
                    // A lost acknowledgement is repeated by the next pass.
                    let _ = self
                        .relay_post(
                            &path,
                            ACK_ACTION,
                            &AckCommand {
                                callback_id: &callback_bytes(&id)?,
                                through_seq: cursor,
                            },
                            &Hash::new(crate::crypto::random_id()),
                        )
                        .await;
                }
                if page["state"] != "active" && page["state"].is_string() {
                    let state = if page["state"] == "deleted" {
                        "deleted"
                    } else {
                        "suspended"
                    };
                    self.end_endpoint(&mut endpoints[index], state, now_ms())?;
                    self.save_endpoints(&endpoints)?;
                    break;
                }
                if records.is_empty() || cursor == after {
                    break;
                }
            }
        }
        self.workspace
            .store
            .prune_mcp_events(now_ms().saturating_sub(RETAIN_MS), KEEP)?;
        // Ended endpoints are forgotten with their last records.
        let before = endpoints.len();
        let cutoff = now_ms().saturating_sub(RETAIN_MS);
        endpoints.retain(|e| e.ended_at.is_none_or(|at| at > cutoff));
        if endpoints.len() != before {
            self.save_endpoints(&endpoints)?;
        }
        Ok(report)
    }

    /// Store one relay record; `Some(false)` when already stored and `None`
    /// when its `v1a` signature fails.
    fn store_relay_record(
        &self,
        endpoint: &Endpoint,
        record: &Json,
        account: &str,
        device: &str,
        keys: Option<&[Hash]>,
    ) -> Result<Option<bool>> {
        let seq = record["seq"].as_u64().ok_or(Error::IntegrityFailed)?;
        let kind = record["kind"].as_str().ok_or(Error::IntegrityFailed)?;
        let id = &endpoint.endpoint_id;
        if kind == "relay_gap" {
            return self
                .store_record(
                    id,
                    &format!("relay_gap:{seq}"),
                    &Record {
                        kind: "relay_gap".into(),
                        received_at: record["received_at"].as_u64().unwrap_or_default(),
                        subscription_id: endpoint.binding.subscription_id.clone(),
                        upstream: Json::Null,
                        verified: None,
                        detail: json!({"dropped": record["dropped"]}),
                    },
                )
                .map(Some);
        }
        let webhook_id = record["webhook_id"]
            .as_str()
            .ok_or(Error::IntegrityFailed)?;
        if self.workspace.store.has_mcp_event(id, webhook_id)? {
            return Ok(Some(false));
        }
        let sealed: Sealed = decode(
            &crate::codec::unb64(record["sealed"].as_str().ok_or(Error::IntegrityFailed)?)?,
            MAX_SEALED,
        )?;
        let plain = hpke_open(
            &self.keys()?.hpke,
            &HpkeEnvelope {
                enc: sealed.enc.into_vec(),
                ciphertext: sealed.ciphertext.into_vec(),
            },
            &(INFO, account, device, id, seq),
        )?;
        let delivery: Delivery = decode(&plain, MAX_SEALED)?;
        ensure(
            delivery.headers.get("webhook-id").map(String::as_str) == Some(webhook_id),
            Error::IntegrityFailed,
        )?;
        let Some(verified) = verify(&endpoint.secrets, &delivery, keys) else {
            return Ok(None);
        };
        let upstream: Json = serde_json::from_slice(&delivery.body).unwrap_or(Json::Null);
        // A control envelope names its type; the relay took nothing else.
        ensure(
            upstream["type"].as_str().unwrap_or("event") == kind,
            Error::IntegrityFailed,
        )?;
        let subscription_id = delivery
            .headers
            .get("x-mcp-subscription-id")
            .cloned()
            .or_else(|| endpoint.binding.subscription_id.clone());
        self.store_record(
            id,
            webhook_id,
            &Record {
                kind: kind.into(),
                received_at: delivery.received_at,
                subscription_id,
                upstream,
                verified: Some(verified),
                detail: Json::Null,
            },
        )
        .map(Some)
    }

    /// Write a `refresh_due` record for every binding whose `refreshBefore`
    /// is near: `min(10 minutes, 20%)` of the time it was granted. The
    /// daemon renews nothing itself.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn mcp_refresh_due(&mut self, now: u64) -> Result<u64> {
        let mut endpoints = self.endpoints()?;
        let mut written = 0;
        for endpoint in endpoints.iter_mut().filter(|e| e.state == "active") {
            let Some(before) = endpoint.binding.refresh_before else {
                continue;
            };
            if endpoint.binding.due_for == Some(before) {
                continue;
            }
            let lead = REFRESH_LEAD_MS.min(before.saturating_sub(endpoint.binding.bound_at) / 5);
            if now + lead < before {
                continue;
            }
            self.store_record(
                &endpoint.endpoint_id,
                &format!("refresh_due:{before}"),
                &Record {
                    kind: "refresh_due".into(),
                    received_at: now,
                    subscription_id: endpoint.binding.subscription_id.clone(),
                    upstream: Json::Null,
                    verified: None,
                    detail: json!({
                        "refresh_before": before,
                        "server": endpoint.binding.server,
                    }),
                },
            )?;
            endpoint.binding.due_for = Some(before);
            written += 1;
        }
        if written > 0 {
            self.save_endpoints(&endpoints)?;
        }
        Ok(written)
    }

    /// Records after the cursor `after`, as MCP event occurrences of
    /// [`EVENT_NAME`] whose `data` is the delivery: `{endpoint_id,
    /// subscription_id, kind, upstream, received_at, verified, context}`.
    /// The cursor is the local sequence; an acknowledged record is gone.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` for an
    /// endpoint that is not the caller's.
    pub fn mcp_events_read(&self, filter: &Filter, after: u64, limit: usize) -> Result<Json> {
        let ids = self.selected(filter)?;
        let endpoints = self.endpoints()?;
        let limit = limit.clamp(1, 100);
        let mut rows = self.workspace.store.mcp_events(&ids, after, limit + 1)?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let mut events = Vec::with_capacity(rows.len());
        for row in &rows {
            let name = format!("mcp-event:{}:{}", row.endpoint, row.webhook_id);
            let record: Record = decode(
                &self
                    .workspace
                    .open_record(self.keys()?, &name, &row.value)?,
                MAX_SEALED,
            )?;
            let context = endpoints
                .iter()
                .find(|e| e.endpoint_id == row.endpoint)
                .map(|e| json!(e.context));
            let mut data = json!({
                "endpoint_id": row.endpoint,
                "subscription_id": record.subscription_id,
                "kind": record.kind,
                "upstream": record.upstream,
                "received_at": record.received_at,
                "verified": record.verified.map(|v| json!({"v1": v.v1, "v1a": v.v1a})),
                "context": context,
            });
            if let Json::Object(detail) = record.detail {
                for (key, value) in detail {
                    data[key] = value;
                }
            }
            events.push(json!({
                "eventId": format!("{}:{}", row.endpoint, row.webhook_id),
                "name": EVENT_NAME,
                "timestamp": timestamp(row.received_at),
                "data": data,
                "cursor": row.seq.to_string(),
            }));
        }
        let cursor = rows.last().map_or(after, |r| r.seq);
        Ok(json!({
            "events": events,
            "cursor": cursor.to_string(),
            "hasMore": more,
            "truncated": false,
        }))
    }

    /// Let this device forget the selected records up to `through`; a cursor
    /// past them stays valid.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` for an
    /// endpoint that is not the caller's.
    pub fn mcp_events_ack(&self, filter: &Filter, through: u64) -> Result<Json> {
        let ids = self.selected(filter)?;
        let acked = self.workspace.store.delete_mcp_events(&ids, through)?;
        Ok(json!({"acked": acked, "through": through.to_string()}))
    }
}

#[cfg(test)]
#[path = "mcp_events_tests.rs"]
mod tests;
