//! The connector host: one Agent Protocols local connector per hosted Agent
//! ID, restored from its snapshot for each tool call and saved after it
//! (local connector §4.1), whether the call succeeded or not.
//!
//! The presented head of each room is per MCP session (local connector
//! §4.2): a session sees a room move only by reading it, whatever other
//! sessions of the same Agent ID read. Each call names its [`Scope`]; the
//! session's presented heads go into the connector before the call and
//! come out after it, and the shared snapshot keeps none. Held drafts and
//! inbox claims stay with the Agent ID, so a restarted agent finishes what
//! the previous session held, and a claim's lease keeps a second session
//! off an item one is handling.
//!
//! The operator's configuration alone decides the host allowlist and the
//! blocked mail senders: the snapshot keeps no host, and before every call
//! the configured hosts and senders replace what it remembers, so neither a
//! tool nor a stored snapshot can change them (plan DT02). A call's [`Scope`] may narrow the allowlist to the origins
//! an agent's grant names: the connector then refuses every other origin,
//! and the runtime refuses a room, or a held draft of one, it knows on
//! another host, which a read from local state would not ask, and leaves
//! the other hosts' rooms and held drafts out of the lists. The mail
//! decryption keys are not in the snapshot: the caller brings them from its
//! key store and keeps what the call leaves. Calls of one Agent ID run one
//! at a time, each inside a [`Turn`] the caller holds from reading the keys
//! to saving them, so two sessions never overwrite each other's state;
//! calls of different Agent IDs do not wait for each other. The connectors
//! share one discovery cache, so each host's discovery document is read once.

use crate::resources::{Resource, RoomPart};
use agent_protocols::{
    error::SdkError,
    http_client::ProfileClient,
    identity::{AgentId, AgentSigner, service_origin},
    local_connector::{
        AgentProtocolsHost, DiscoveryCache, LocalConnector, TOOL_DRAFTS_LIST, TOOL_NAMES,
        TOOL_ROOMS_LIST,
    },
    profile::{ProfileCache, materialize_profile},
};
use dmsg_types::{Error, Result, ensure, ensure_valid};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Sessions not used for this long are forgotten.
const SESSION_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// Most sessions kept per Agent ID.
const MAX_SESSIONS: i64 = 256;

/// One allowed Agent Protocols host, as `config.toml` lists it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Host {
    /// Host origin, such as `https://api.al.ink`.
    pub host: String,
    /// Display label.
    #[serde(default)]
    pub label: Option<String>,
    /// Agent Profile service the host's agents publish to.
    #[serde(default)]
    pub profile_service: Option<String>,
}

/// A tool's refusal, with the protocol's code and data kept.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolError {
    /// Protocol error code, such as `permission_denied` or `room_head_mismatch`.
    pub code: String,
    /// Human-readable detail.
    pub message: String,
    /// The host's error data, when it sent any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl From<SdkError> for ToolError {
    fn from(error: SdkError) -> Self {
        let code = match &error {
            SdkError::HttpStatus {
                status, code: None, ..
            } => format!("http_{status}"),
            SdkError::Http(_) => "unavailable".into(),
            other => other.code().unwrap_or("invalid_request").into(),
        };
        let data = match &error {
            SdkError::HttpStatus {
                status,
                data,
                max_seen_nonce,
                ..
            } => Some(json!({"status": status, "data": data, "max_seen_nonce": max_seen_nonce})),
            other => other.data().cloned(),
        };
        Self {
            code,
            message: error.to_string(),
            data,
        }
    }
}

/// The operator's configuration of the connectors.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The hosts the agents may sign for and read from.
    pub hosts: Vec<Host>,
    /// Agent IDs whose mail is deleted unopened (Agent Mail §7.2).
    pub blocked_senders: Vec<String>,
    /// PEM certificates that replace the system roots for the hosts: a
    /// local deployment's own CA. The system verifier would refuse a test
    /// certificate a local deployment can use (macOS limits its lifetime).
    pub local_roots: Vec<Vec<u8>>,
    /// A local deployment: loopback HTTP hosts and `local_roots` allowed.
    pub local: bool,
}

/// Where a call runs: the workspace's state file, the session it is for,
/// such as one MCP session, the CLI's named one or the desktop's, and the
/// origins it may reach.
#[derive(Clone, Copy, Debug)]
pub struct Scope<'a> {
    /// The state file of the workspace.
    pub path: &'a Path,
    /// The session, at most 128 bytes.
    pub session: &'a str,
    /// Only these of [`Runtime::origins`], each a host of its own; `None`
    /// for the configured hosts.
    pub reach: Option<&'a [String]>,
}

/// The right to run one call of an Agent ID, held from reading its mail keys
/// to saving them.
pub struct Turn {
    agent_id: String,
    _guard: tokio::sync::OwnedMutexGuard<()>,
}

/// One call's outcome and the mail keys the connector holds after it.
pub struct Called {
    /// The tool's result or refusal.
    pub outcome: std::result::Result<Value, ToolError>,
    /// `{version, agent_id, keys}` for the caller's key store; keys of
    /// cards past `receive_until` are gone.
    pub mail_keys: Value,
    /// Whether the call changed the Agent ID's shared connector state, which
    /// every session's resources show.
    pub changed: bool,
}

/// Key material and its host-provided encryption for crash-safe Mail calls.
pub struct DurableMail<'a> {
    /// The latest committed plaintext key snapshot, kept only in memory.
    pub keys: &'a Value,
    /// Encrypts a key snapshot for this workspace and Agent; never returns plaintext.
    pub seal: MailSealer,
}

/// Host-only encryption of a Mail key snapshot.
pub type MailSealer = Box<dyn Fn(&Value) -> Result<String> + Send + Sync>;
struct MailCall<'a> {
    keys: &'a Value,
    seal: Option<MailSealer>,
}

/// The connectors of the hosted Agent IDs; each call names the state file
/// of the workspace it runs for.
pub struct Runtime {
    hosts: Vec<AgentProtocolsHost>,
    blocked_senders: Vec<AgentId>,
    http: reqwest::Client,
    discovery: DiscoveryCache,
    turns: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

/// An origin without path: HTTPS, or loopback HTTP in a local deployment.
fn check_origin(value: &str, local: bool, name: &str) -> Result<String> {
    let origin = service_origin(value).map_err(|_| dmsg_types::invalid(name))?;
    let url = url::Url::parse(&origin).map_err(|_| dmsg_types::invalid(name))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    ensure_valid(
        origin == value.trim_end_matches('/')
            && (url.scheme() == "https" || (local && loopback && url.scheme() == "http")),
        name,
    )?;
    Ok(origin)
}

impl Runtime {
    /// A host of the connectors, configured by `options`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for a host or profile service that is
    /// not an origin (HTTPS, or loopback HTTP when local), a blocked sender
    /// that is not an Agent ID, or a trusted root outside a local deployment.
    pub fn new(options: &Options) -> Result<Self> {
        let local = options.local;
        ensure_valid(options.hosts.len() <= 32, "agent_hosts")?;
        ensure_valid(
            options.blocked_senders.len() <= 1024,
            "agent_blocked_senders",
        )?;
        ensure_valid(local || options.local_roots.is_empty(), "agent_ca")?;
        let hosts = options
            .hosts
            .iter()
            .map(|h| {
                Ok(AgentProtocolsHost {
                    host: check_origin(&h.host, local, "agent_hosts.host")?,
                    label: h.label.clone(),
                    allowed: true,
                    features: Vec::new(),
                    profile_service: h
                        .profile_service
                        .as_deref()
                        .map(|s| check_origin(s, local, "agent_hosts.profile_service"))
                        .transpose()?,
                    last_checked_at: None,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let blocked_senders = options
            .blocked_senders
            .iter()
            .map(|s| {
                s.parse()
                    .map_err(|_| dmsg_types::invalid("agent_blocked_senders"))
            })
            .collect::<Result<Vec<AgentId>>>()?;
        let roots = options
            .local_roots
            .iter()
            .map(|pem| {
                reqwest::Certificate::from_pem(pem).map_err(|_| dmsg_types::invalid("agent_ca"))
            })
            .collect::<Result<Vec<_>>>()?;
        // Agent Mail §8.1: no redirect is followed, so no owner JWT or
        // signed write reaches an origin the operator did not allow.
        let mut http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none());
        if !roots.is_empty() {
            http = http.tls_certs_only(roots);
        }
        let http = http
            .build()
            .map_err(|e| Error::Unavailable(format!("http: {e}")))?;
        Ok(Self {
            hosts,
            blocked_senders,
            http,
            discovery: DiscoveryCache::new(),
            turns: std::sync::Mutex::new(HashMap::new()),
        })
    }

    /// The configured hosts.
    pub fn hosts(&self) -> &[AgentProtocolsHost] {
        &self.hosts
    }

    /// The configured senders whose mail is deleted unopened.
    pub fn blocked_senders(&self) -> Vec<String> {
        self.blocked_senders
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// The sessions the state file at `path` remembers, by Agent ID, each
    /// newest first: `{agent_id: [{session, updated_at}]}`.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn sessions(&self, path: &Path) -> Result<HashMap<String, Vec<Value>>> {
        let conn = open(path)?;
        let mut statement = conn
            .prepare("SELECT agent_id, session, updated_at FROM sessions ORDER BY updated_at DESC")
            .map_err(storage)?;
        let rows = statement
            .query_map([], |r| {
                let session =
                    json!({"session": r.get::<_, String>(1)?, "updated_at": r.get::<_, i64>(2)?});
                Ok((r.get::<_, String>(0)?, session))
            })
            .map_err(storage)?;
        let mut sessions: HashMap<String, Vec<Value>> = HashMap::new();
        for row in rows {
            let (agent_id, session) = row.map_err(storage)?;
            sessions.entry(agent_id).or_default().push(session);
        }
        Ok(sessions)
    }

    /// Delete everything the state file at `path` keeps of the Agent ID
    /// whose turn this is: its connector and its sessions.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn forget(&self, turn: &Turn, path: &Path) -> Result<()> {
        let conn = open(path)?;
        let tx = conn.unchecked_transaction().map_err(storage)?;
        tx.execute(
            "DELETE FROM mail_checkpoints WHERE agent_id=?1",
            [&turn.agent_id],
        )
        .map_err(storage)?;
        tx.execute(
            "DELETE FROM profile_pins WHERE agent_id=?1",
            [&turn.agent_id],
        )
        .map_err(storage)?;
        tx.execute(
            "DELETE FROM connectors WHERE agent_id = ?1",
            params![turn.agent_id],
        )
        .map_err(storage)?;
        tx.execute(
            "DELETE FROM sessions WHERE agent_id = ?1",
            params![turn.agent_id],
        )
        .map_err(storage)?;
        tx.execute(
            "DELETE FROM mail_events WHERE agent = ?1",
            params![turn.agent_id],
        )
        .map_err(storage)?;
        tx.execute(
            "DELETE FROM mail_event_heads WHERE agent = ?1",
            params![turn.agent_id],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)
    }

    /// Read an encrypted key checkpoint. Mutation and clearing still require the Agent turn.
    /// # Errors
    /// Returns storage errors.
    pub fn mail_checkpoint(&self, path: &Path, agent_id: &str) -> Result<Option<String>> {
        let Some(conn) = open_existing(path)? else {
            return Ok(None);
        };
        conn.query_row(
            "SELECT sealed FROM mail_checkpoints WHERE agent_id=?1",
            [agent_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)
    }

    /// Clear a checkpoint only after the caller has durably saved its current keys.
    /// # Errors
    /// Returns storage errors.
    pub fn clear_mail_checkpoint(&self, turn: &Turn, path: &Path) -> Result<()> {
        let Some(conn) = open_existing(path)? else {
            return Ok(());
        };
        conn.execute(
            "DELETE FROM mail_checkpoints WHERE agent_id=?1",
            [&turn.agent_id],
        )
        .map_err(storage)?;
        Ok(())
    }

    /// The Agent Profile `agent_id` published, at each profile service the
    /// configured hosts name: `[{service, profile}]`, or `{service, error}`
    /// where reading failed. A profile is the latest accepted update, its
    /// signature checked here (Agent Profile §8); a service without one
    /// answers `profile: null`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an invalid Agent ID.
    pub async fn profiles(&self, path: &Path, agent_id: &str) -> Result<Vec<Value>> {
        let id: AgentId = agent_id.parse().map_err(|_| dmsg_types::invalid("agent"))?;
        let _turn = self.turn(agent_id).await;
        let conn = open(path)?;
        let stored: Option<String> = conn
            .query_row(
                "SELECT envelope FROM profile_pins WHERE agent_id=?1",
                [agent_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        let mut pins = ProfileCache::new();
        if let Some(stored) = stored {
            let envelope = serde_json::from_str(&stored).map_err(|_| Error::IntegrityFailed)?;
            pins.observe(&envelope, &id)
                .map_err(|_| Error::IntegrityFailed)?;
        }
        let mut durable_hash = pins.get(&id).map(|envelope| envelope.hash.clone());
        let mut services: Vec<String> = Vec::new();
        for service in self.hosts.iter().filter_map(|h| h.profile_service.clone()) {
            if !services.contains(&service) {
                services.push(service);
            }
        }
        let mut found = Vec::new();
        for service in services {
            let client = ProfileClient::discover_with_client(&service, self.http.clone()).await;
            if service_origin(client.profiles_url()).ok().as_deref() != Some(service.as_str()) {
                found.push(
                    json!({"service":service,"error":ToolError::from(SdkError::PermissionDenied)}),
                );
                continue;
            }
            let read = match client.latest_profile_update(&id).await {
                Ok(envelope) => pins
                    .observe(&envelope, &id)
                    .map_err(ToolError::from)
                    .and_then(|envelope| {
                        if durable_hash.as_deref() != Some(envelope.hash.as_str()) {
                            let text = serde_json::to_string(&envelope)
                                .expect("profile envelope is serializable");
                            conn.execute(
                                "INSERT INTO profile_pins VALUES(?1,?2)
                                 ON CONFLICT(agent_id) DO UPDATE SET envelope=excluded.envelope",
                                params![agent_id, text],
                            )
                            .map_err(|_| {
                                ToolError::from(SdkError::protocol(
                                    "persistence_failed",
                                    "Could not retain the verified profile",
                                ))
                            })?;
                            durable_hash = Some(envelope.hash.clone());
                        }
                        materialize_profile(&envelope)
                            .map(|profile| json!(profile))
                            .map_err(ToolError::from)
                    }),
                Err(SdkError::HttpStatus { status: 404, .. }) => Ok(Value::Null),
                Err(error) if error.code() == Some("not_found") => Ok(Value::Null),
                Err(error) => Err(ToolError::from(error)),
            };
            found.push(match read {
                Ok(profile) => json!({"service": service, "profile": profile}),
                Err(error) => json!({"service": service, "error": error}),
            });
        }
        if let Some(latest) = pins.get(&id) {
            for item in &mut found {
                if item["profile"].is_object() && item["profile"]["event_id"] != latest.hash {
                    let service = item["service"].clone();
                    let error = ToolError::from(SdkError::protocol(
                        "stale_profile",
                        "A newer verified profile was observed",
                    ));
                    *item = json!({"service": service, "error": error});
                }
            }
        }
        Ok(found)
    }

    /// The origins a grant of `capability` may reach: the configured hosts,
    /// and for Profile reads/writes, the profile services they name.
    /// The daemon also uses that candidate set for mail_resolve, while
    /// checking mail.read grants rather than implicitly granting a profile.
    pub fn origins(&self, capability: &str) -> Vec<String> {
        let mut origins: Vec<String> = self.hosts.iter().map(|h| h.host.clone()).collect();
        if matches!(capability, "agent.profile.read" | "agent.profile.write") {
            for service in self.hosts.iter().filter_map(|h| h.profile_service.clone()) {
                if !origins.contains(&service) {
                    origins.push(service);
                }
            }
        }
        origins
    }

    /// The redirect-free HTTPS client with this deployment's configured roots.
    pub fn http_client(&self) -> reqwest::Client {
        self.http.clone()
    }

    /// The connector of `signer`'s Agent ID as `scope` sees it, from its
    /// saved snapshot (`Null` for none), which gets the session's presented
    /// heads: allowed only the configured hosts within its reach, and
    /// blocking only the configured senders.
    fn restore(
        &self,
        conn: &Connection,
        scope: Scope<'_>,
        signer: AgentSigner,
        saved: &mut Value,
    ) -> Result<LocalConnector> {
        ensure_valid(
            !scope.session.is_empty() && scope.session.len() <= 128,
            "session",
        )?;
        let mut connector = match saved.is_null() {
            true => LocalConnector::new(signer),
            false => {
                let heads = load_session(conn, signer.agent_id().as_str(), scope.session)?;
                present(saved, &heads);
                LocalConnector::from_snapshot(signer, saved).map_err(|_| Error::IntegrityFailed)?
            }
        }
        .with_http_client(self.http.clone())
        .with_discovery_cache(self.discovery.clone());
        connector.state_mut().hosts.clear();
        match scope.reach {
            None => {
                for host in &self.hosts {
                    connector.add_host(host.clone());
                }
            }
            // Of every origin a grant may name, each as a host of its own:
            // a configured host brings no profile service along.
            Some(reach) => {
                let origins = self.origins("agent.profile.write");
                for origin in origins.into_iter().filter(|o| reach.contains(o)) {
                    let label = self.hosts.iter().find(|h| h.host == origin);
                    connector.add_host(AgentProtocolsHost {
                        label: label.and_then(|h| h.label.clone()),
                        host: origin,
                        allowed: true,
                        features: Vec::new(),
                        profile_service: None,
                        last_checked_at: None,
                    });
                }
            }
        }
        connector.set_mail_blocked_senders(self.blocked_senders.iter().cloned());
        Ok(connector)
    }

    /// Wait for the right to run one call of `agent_id`.
    pub async fn turn(&self, agent_id: &str) -> Turn {
        let lock = self
            .turns
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(agent_id.to_string())
            .or_default()
            .clone();
        Turn {
            agent_id: agent_id.to_string(),
            _guard: lock.lock_owned().await,
        }
    }

    /// Run one standard tool as `signer`'s Agent ID in `scope`, with the
    /// mail decryption keys its mailboxes retain (`mail_keys`, as
    /// [`Called::mail_keys`] returned them).
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a tool that is not one of the standard
    /// ones, `Error::InvalidInput` for a turn of another Agent ID,
    /// `Error::IntegrityFailed` for keys of another Agent ID or that do not
    /// match their public keys, and `Error::Unavailable` when storage fails;
    /// the tool's own refusal is [`Called::outcome`].
    pub async fn call(
        &self,
        turn: &Turn,
        scope: Scope<'_>,
        signer: AgentSigner,
        mail_keys: &Value,
        tool: &str,
        input: Value,
    ) -> Result<Called> {
        self.call_inner(
            turn,
            scope,
            signer,
            MailCall {
                keys: mail_keys,
                seal: None,
            },
            tool,
            input,
        )
        .await
    }

    /// Run a production call, committing encrypted keys and connector state before Mail publication.
    /// # Errors
    /// Returns the errors of `call`; a checkpoint failure prevents network publication.
    pub async fn call_durable(
        &self,
        turn: &Turn,
        scope: Scope<'_>,
        signer: AgentSigner,
        mail: DurableMail<'_>,
        tool: &str,
        input: Value,
    ) -> Result<Called> {
        self.call_inner(
            turn,
            scope,
            signer,
            MailCall {
                keys: mail.keys,
                seal: Some(mail.seal),
            },
            tool,
            input,
        )
        .await
    }

    // The upstream persistence hook requires its unboxed SDK error type.
    #[allow(clippy::result_large_err)]
    async fn call_inner(
        &self,
        turn: &Turn,
        scope: Scope<'_>,
        signer: AgentSigner,
        mail: MailCall<'_>,
        tool: &str,
        input: Value,
    ) -> Result<Called> {
        ensure(TOOL_NAMES.contains(&tool), Error::NotFound)?;
        let agent_id = signer.agent_id();
        ensure_valid(turn.agent_id == agent_id.as_str(), "turn")?;
        // Keep the original database handle: a reset must not let an in-flight
        // checkpoint recreate agents.sqlite at the removed path.
        let conn = Arc::new(std::sync::Mutex::new(open(scope.path)?));
        let (saved, mut connector) = {
            let connection = conn
                .lock()
                .map_err(|_| Error::Unavailable("connector checkpoint lock poisoned".into()))?;
            let stored = load(&connection, agent_id.as_str())?;
            let mut saved = parse(stored.as_deref())?;
            let connector = self.restore(&connection, scope, signer, &mut saved)?;
            (saved, connector)
        };
        connector
            .restore_mail_keys(mail.keys)
            .map_err(|_| Error::IntegrityFailed)?;
        let failed = Arc::new(AtomicBool::new(false));
        let checkpoint_changed = Arc::new(AtomicBool::new(false));
        if let Some(seal) = mail.seal {
            let checkpoint_conn = conn.clone();
            let session = scope.session.to_owned();
            let actor = agent_id.clone();
            let failure = failed.clone();
            let changed = checkpoint_changed.clone();
            connector = connector.with_mail_persistence(move |snapshot, keys| {
                let commit = || -> Result<()> {
                    let sealed = seal(keys)?;
                    let conn = checkpoint_conn.lock().map_err(|_| {
                        Error::Unavailable("connector checkpoint lock poisoned".into())
                    })?;
                    let old = load(&conn, actor.as_str())?;
                    let written = save_snapshot(
                        &conn,
                        &session,
                        &actor,
                        snapshot.clone(),
                        old.as_deref(),
                        Some(&sealed),
                    )?;
                    changed.fetch_or(written, Ordering::SeqCst);
                    Ok(())
                };
                commit().map_err(|_| {
                    failure.store(true, Ordering::SeqCst);
                    SdkError::protocol("persistence_failed", "Could not commit the Mail checkpoint")
                })
            });
        }
        let outcome = match out_of_reach(scope.reach, &saved, &input) {
            true => Err(SdkError::PermissionDenied),
            false => connector
                .call_tool(tool, input)
                .await
                .map(|value| in_reach(scope.reach, tool, value)),
        };
        // Never save an uncheckpointed card after its durable hook failed. The last
        // successful checkpoint remains the recovery point, including unknown replies.
        ensure(
            !failed.load(Ordering::SeqCst),
            Error::Unavailable("Mail checkpoint could not be committed".into()),
        )?;
        let conn = conn
            .lock()
            .map_err(|_| Error::Unavailable("connector checkpoint lock poisoned".into()))?;
        let current = load(&conn, agent_id.as_str())?;
        let changed = save(&conn, scope.session, &mut connector, current.as_deref())?
            || checkpoint_changed.load(Ordering::SeqCst);
        Ok(Called {
            outcome: outcome.map_err(ToolError::from),
            mail_keys: connector.mail_keys_snapshot(),
            changed,
        })
    }

    /// Inspect committed mail without waiting for an in-flight connector
    /// call or rewriting its snapshot. Supply keys for accurate mailbox availability. Marking mail
    /// read still needs the normal serialized call.
    /// # Errors
    /// Returns `Error::Forbidden` for other tools or a marking read, and
    /// storage errors as [`Self::read`].
    pub async fn inspect_mail(
        &self,
        scope: Scope<'_>,
        signer: AgentSigner,
        tool: &str,
        input: Value,
        mail_keys: Option<&Value>,
    ) -> Result<std::result::Result<Value, ToolError>> {
        ensure(
            crate::tools::is_mail_inspection(tool, &input),
            Error::Forbidden,
        )?;
        let conn = open(scope.path)?;
        let mut saved = parse(load(&conn, signer.agent_id().as_str())?.as_deref())?;
        let mut connector = self.restore(&conn, scope, signer, &mut saved)?;
        if let Some(keys) = mail_keys {
            connector
                .restore_mail_keys(keys)
                .map_err(|_| Error::IntegrityFailed)?;
        }
        Ok(connector
            .call_tool(tool, input)
            .await
            .map(|value| in_reach(scope.reach, tool, value))
            .map_err(ToolError::from))
    }

    /// Read one resource of `signer`'s Agent ID as `scope` sees it (local
    /// connector §4.4), from local state only: nothing is fetched, and
    /// nothing the read touches is saved, so reading never moves a
    /// presented head or a read mark.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown resource or room,
    /// `Error::Forbidden` for a room on a host out of reach and
    /// `Error::Unavailable` when storage fails.
    pub async fn read(
        &self,
        scope: Scope<'_>,
        signer: AgentSigner,
        uri: &str,
        mail_keys: Option<&Value>,
    ) -> Result<Value> {
        let resource = Resource::parse(uri).ok_or(Error::NotFound)?;
        let conn = open(scope.path)?;
        let mut saved = parse(load(&conn, signer.agent_id().as_str())?.as_deref())?;
        let mut connector = self.restore(&conn, scope, signer, &mut saved)?;
        if let Some(keys) = mail_keys {
            connector
                .restore_mail_keys(keys)
                .map_err(|_| Error::IntegrityFailed)?;
        }
        let room = match &resource {
            Resource::Room(id, _) => {
                let known = saved["rooms"].as_array().into_iter().flatten().find(|r| {
                    r["room"]["id"] == id.as_str() && r["synced_seq"].as_u64().unwrap_or(0) > 0
                });
                Some(known.ok_or(Error::NotFound)?)
            }
            _ => None,
        };
        let input = match (&resource, room) {
            (Resource::Room(id, RoomPart::Timeline), Some(room)) => {
                let synced = room["synced_seq"].as_u64().unwrap_or(0);
                json!({"room_id": id, "after_seq": synced.saturating_sub(100), "limit": 100})
            }
            (Resource::Room(id, RoomPart::AgentStatus), _) => {
                let cached = saved["agent_statuses"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|s| s["room_id"] == id.as_str());
                if !cached {
                    return Ok(json!({"statuses": []}));
                }
                json!({"room_id": id})
            }
            (Resource::Room(id, _), _) => json!({"room_id": id}),
            (Resource::Inbox, _) => json!({"limit": 100}),
            (Resource::Messages, _) => json!({"limit": 100}),
            _ => json!({}),
        };
        ensure(!out_of_reach(scope.reach, &saved, &input), Error::Forbidden)?;
        let value = connector
            .call_tool(resource.tool(), input)
            .await
            .map_err(|e| Error::InvalidInput(format!("{uri}: {e}")))?;
        let value = in_reach(scope.reach, resource.tool(), value);
        Ok(match resource {
            Resource::Hosts => json!({"hosts": value["hosts"]}),
            Resource::Room(_, RoomPart::Types) => {
                let types = value["room"].get("types").cloned();
                json!({"types": types.unwrap_or_else(|| json!([])), "sync": value["sync"]})
            }
            _ => value,
        })
    }
}

/// Whether an input names a room, or a held draft of one, the snapshot
/// `saved` knows on a host out of `reach`: reading it from local state
/// would not ask the host.
fn out_of_reach(reach: Option<&[String]>, saved: &Value, input: &Value) -> bool {
    reach.is_some_and(|reach| room_host(saved, input).is_some_and(|host| !reach.contains(&host)))
}

/// A tool's result within `reach`: `rooms_list` and `drafts_list` without
/// the rooms, and the held drafts, of the hosts out of it.
fn in_reach(reach: Option<&[String]>, tool: &str, mut value: Value) -> Value {
    let Some(reach) = reach else {
        return value;
    };
    let (list, host): (&str, fn(&Value) -> &Value) = match tool {
        TOOL_ROOMS_LIST => ("rooms", |room| &room["host"]),
        TOOL_DRAFTS_LIST => ("drafts", |draft| &draft["current_sync"]["host"]),
        _ => return value,
    };
    if let Some(items) = value[list].as_array_mut() {
        items.retain(|item| {
            host(item)
                .as_str()
                .is_some_and(|host| reach.iter().any(|r| r == host))
        });
    }
    value
}

/// The host of the room an input names: its draft's, its `host`, else the
/// only one the snapshot knows with that ID.
fn room_host(saved: &Value, input: &Value) -> Option<String> {
    if let Some(draft_id) = input["draft_id"].as_str() {
        let draft = &saved["drafts"][draft_id]["draft"];
        return draft["current_sync"]["host"].as_str().map(str::to_string);
    }
    let room_id = input["room_id"].as_str()?;
    if let Some(host) = input["host"].as_str() {
        return Some(host.trim_end_matches('/').to_string());
    }
    let mut hosts = saved["rooms"]
        .as_array()?
        .iter()
        .filter(|room| room["room"]["id"] == room_id)
        .filter_map(|room| room["host"].as_str());
    match (hosts.next(), hosts.next()) {
        (Some(host), None) => Some(host.to_string()),
        _ => None,
    }
}

pub(crate) fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).map_err(storage)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    init(&conn)?;
    Ok(conn)
}

/// Recovery reads and acknowledgements must never recreate a reset workspace.
fn open_existing(path: &Path) -> Result<Option<Connection>> {
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(storage)?;
    init(&conn)?;
    Ok(Some(conn))
}

fn init(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "synchronous", "FULL")
        .map_err(storage)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mail_checkpoints (agent_id TEXT PRIMARY KEY, sealed TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS profile_pins (agent_id TEXT PRIMARY KEY, envelope TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS connectors (agent_id TEXT PRIMARY KEY, snapshot TEXT NOT NULL, updated_at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS sessions (agent_id TEXT NOT NULL, session TEXT NOT NULL, presented TEXT NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY (agent_id, session));",
    )
    .map_err(storage)?;
    crate::mail_events::init(conn)
}

/// The stored snapshot of an Agent ID's connector.
fn load(conn: &Connection, agent_id: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT snapshot FROM connectors WHERE agent_id = ?1",
        params![agent_id],
        |r| r.get(0),
    )
    .optional()
    .map_err(storage)
}

/// A stored snapshot as JSON, `Null` for none.
fn parse(stored: Option<&str>) -> Result<Value> {
    stored.map_or(Ok(Value::Null), |text| {
        serde_json::from_str(text).map_err(|_| Error::IntegrityFailed)
    })
}

/// Save a connector: its presented heads for `session`, and the rest for
/// every session unless it equals `stored`. The hosts are left out: every
/// restore applies the configured ones. Returns whether the rest changed.
fn save(
    conn: &Connection,
    session: &str,
    connector: &mut LocalConnector,
    stored: Option<&str>,
) -> Result<bool> {
    let snapshot = connector
        .snapshot()
        .map_err(|e| Error::Unavailable(format!("snapshot: {e}")))?;
    save_snapshot(conn, session, &connector.agent_id(), snapshot, stored, None)
}

fn save_snapshot(
    conn: &Connection,
    session: &str,
    agent_id: &AgentId,
    mut snapshot: Value,
    stored: Option<&str>,
    checkpoint: Option<&str>,
) -> Result<bool> {
    ensure(
        snapshot["agent_id"] == agent_id.as_str(),
        Error::IntegrityFailed,
    )?;
    snapshot["hosts"] = json!({});
    let presented = presented(&snapshot);
    present(&mut snapshot, &[]);
    let snapshot_text = snapshot.to_string();
    let changed = stored != Some(snapshot_text.as_str());
    let now = agent_protocols::identity::unix_ms();
    let tx = conn.unchecked_transaction().map_err(storage)?;
    if let Some(sealed) = checkpoint {
        tx.execute(
            "INSERT INTO mail_checkpoints(agent_id,sealed) VALUES(?1,?2)
             ON CONFLICT(agent_id) DO UPDATE SET sealed=excluded.sealed",
            params![agent_id.as_str(), sealed],
        )
        .map_err(storage)?;
    }
    if changed {
        crate::mail_events::record(&tx, agent_id.as_str(), &parse(stored)?, &snapshot, now)?;
        tx.execute(
            "INSERT INTO connectors (agent_id, snapshot, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(agent_id) DO UPDATE SET snapshot = excluded.snapshot, updated_at = excluded.updated_at",
            params![agent_id.as_str(), snapshot_text, now],
        )
        .map_err(storage)?;
    }
    tx.execute(
        "INSERT INTO sessions (agent_id, session, presented, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(agent_id, session) DO UPDATE SET presented = excluded.presented, updated_at = excluded.updated_at",
        params![
            agent_id.as_str(),
            session,
            Value::Array(presented).to_string(),
            now
        ],
    )
    .map_err(storage)?;
    tx.execute(
        "DELETE FROM sessions WHERE agent_id = ?1 AND (updated_at < ?2 OR session NOT IN
         (SELECT session FROM sessions WHERE agent_id = ?1 ORDER BY updated_at DESC LIMIT ?3))",
        params![agent_id.as_str(), now - SESSION_MS, MAX_SESSIONS],
    )
    .map_err(storage)?;
    tx.commit().map_err(storage)?;
    Ok(changed)
}

/// The presented heads `session` saw: `[{host, room_id, seq, hash}]`.
fn load_session(conn: &Connection, agent_id: &str, session: &str) -> Result<Vec<Value>> {
    let text: Option<String> = conn
        .query_row(
            "SELECT presented FROM sessions WHERE agent_id = ?1 AND session = ?2",
            params![agent_id, session],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    match text {
        Some(text) => serde_json::from_str(&text).map_err(|_| Error::IntegrityFailed),
        None => Ok(Vec::new()),
    }
}

/// The presented head of every room of a snapshot that has one.
fn presented(snapshot: &Value) -> Vec<Value> {
    snapshot["rooms"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|room| room["presented_seq"].is_u64())
        .map(|room| {
            json!({
                "host": room["host"],
                "room_id": room["room"]["id"],
                "seq": room["presented_seq"],
                "hash": room["presented_hash"],
            })
        })
        .collect()
}

/// Give every room of a snapshot the presented head of `heads`, or none.
fn present(snapshot: &mut Value, heads: &[Value]) {
    let Some(rooms) = snapshot["rooms"].as_array_mut() else {
        return;
    };
    for room in rooms.iter_mut() {
        let head = heads
            .iter()
            .find(|h| h["host"] == room["host"] && h["room_id"] == room["room"]["id"]);
        let Some(fields) = room.as_object_mut() else {
            continue;
        };
        fields.remove("presented_seq");
        fields.remove("presented_hash");
        if let Some(head) = head {
            fields.insert("presented_seq".into(), head["seq"].clone());
            if head["hash"].is_string() {
                fields.insert("presented_hash".into(), head["hash"].clone());
            }
        }
    }
}

fn storage(error: rusqlite::Error) -> Error {
    Error::Unavailable(format!("agents: {error}"))
}
