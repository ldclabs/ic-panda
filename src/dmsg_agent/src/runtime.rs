//! The connector host: one Agent Protocols local connector per hosted Agent
//! ID, restored from its snapshot for each tool call and saved after it
//! (local connector §4.1), whether the call succeeded or not.
//!
//! The operator's configuration alone decides the host allowlist and the
//! blocked mail senders: before every call each host a snapshot remembers
//! is disallowed, each sender it blocks is unblocked, and the configured
//! ones are applied again, so neither a tool nor a stored snapshot can
//! change them (plan DT02). The mail decryption keys are not in the
//! snapshot: the caller brings them from its key store and keeps what the
//! call leaves. Calls run one at a time, each inside a [`Turn`] the caller
//! holds from reading the keys to saving them, so two sessions of one
//! Agent ID never overwrite each other's state.

use agent_protocols::{
    error::SdkError,
    identity::{service_origin, AgentId, AgentSigner},
    local_connector::{AgentProtocolsHost, LocalConnector, TOOL_NAMES},
};
use dmsg_types::{ensure, ensure_valid, Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

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

/// The right to run one call, held from reading the caller's mail keys to
/// saving them.
pub type Turn<'a> = tokio::sync::MutexGuard<'a, ()>;

/// One call's outcome and the mail keys the connector holds after it.
pub struct Called {
    /// The tool's result or refusal.
    pub outcome: std::result::Result<Value, ToolError>,
    /// `{version, agent_id, keys}` for the caller's key store; keys of
    /// cards past `receive_until` are gone.
    pub mail_keys: Value,
}

/// The connectors of one workspace.
pub struct Runtime {
    path: PathBuf,
    hosts: Vec<AgentProtocolsHost>,
    blocked_senders: Vec<AgentId>,
    http: reqwest::Client,
    calls: tokio::sync::Mutex<()>,
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
    /// A host of the connectors whose state lives in `path`, configured by
    /// `options`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for a host or profile service that is
    /// not an origin (HTTPS, or loopback HTTP when local), a blocked sender
    /// that is not an Agent ID, or a trusted root outside a local deployment.
    pub fn new(path: &Path, options: &Options) -> Result<Self> {
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
            path: path.to_path_buf(),
            hosts,
            blocked_senders,
            http,
            calls: tokio::sync::Mutex::new(()),
        })
    }

    /// The configured hosts.
    pub fn hosts(&self) -> &[AgentProtocolsHost] {
        &self.hosts
    }

    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).map_err(storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS connectors (agent_id TEXT PRIMARY KEY, snapshot TEXT NOT NULL, updated_at INTEGER NOT NULL);",
        )
        .map_err(storage)?;
        Ok(conn)
    }

    /// The saved snapshot of an Agent ID's connector.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn snapshot(&self, agent_id: &str) -> Result<Option<Value>> {
        load(&self.open()?, agent_id)
    }

    fn save(&self, conn: &Connection, connector: &LocalConnector) -> Result<()> {
        let snapshot = connector
            .snapshot()
            .map_err(|e| Error::Unavailable(format!("snapshot: {e}")))?;
        conn.execute(
            "INSERT INTO connectors (agent_id, snapshot, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(agent_id) DO UPDATE SET snapshot = excluded.snapshot, updated_at = excluded.updated_at",
            params![
                connector.agent_id().as_str(),
                snapshot.to_string(),
                agent_protocols::identity::unix_ms()
            ],
        )
        .map_err(storage)?;
        Ok(())
    }

    /// Wait for the right to run one call.
    pub async fn turn(&self) -> Turn<'_> {
        self.calls.lock().await
    }

    /// Run one standard tool as `signer`'s Agent ID, with the mail
    /// decryption keys its mailboxes retain (`mail_keys`, as
    /// [`Called::mail_keys`] returned them; `None` for none yet).
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a tool that is not one of the standard
    /// ones, `Error::IntegrityFailed` for keys of another Agent ID or that
    /// do not match their public keys, and `Error::Unavailable` when storage
    /// fails; the tool's own refusal is [`Called::outcome`].
    pub async fn call(
        &self,
        _turn: &Turn<'_>,
        signer: AgentSigner,
        mail_keys: Option<&Value>,
        tool: &str,
        input: Value,
    ) -> Result<Called> {
        ensure(TOOL_NAMES.contains(&tool), Error::NotFound)?;
        let conn = self.open()?;
        let agent_id = signer.agent_id();
        let saved = load(&conn, agent_id.as_str())?;
        let mut connector = match &saved {
            Some(snapshot) => LocalConnector::from_snapshot(signer, snapshot)
                .map_err(|_| Error::IntegrityFailed)?,
            None => LocalConnector::new(signer),
        }
        .with_http_client(self.http.clone());
        if let Some(keys) = mail_keys {
            connector
                .restore_mail_keys(keys)
                .map_err(|_| Error::IntegrityFailed)?;
        }
        for host in connector.state_mut().hosts.values_mut() {
            host.allowed = false;
        }
        for host in &self.hosts {
            connector.add_host(host.clone());
        }
        let remembered = saved
            .as_ref()
            .and_then(|s| s["mail"]["blocked_senders"].as_array().cloned())
            .unwrap_or_default();
        for sender in remembered.iter().filter_map(|s| s.as_str()?.parse().ok()) {
            connector.set_mail_sender_blocked(sender, false);
        }
        for sender in &self.blocked_senders {
            connector.set_mail_sender_blocked(sender.clone(), true);
        }
        let outcome = connector.call_tool(tool, input).await;
        self.save(&conn, &connector)?;
        Ok(Called {
            outcome: outcome.map_err(ToolError::from),
            mail_keys: connector.mail_keys_snapshot(),
        })
    }
}

fn load(conn: &Connection, agent_id: &str) -> Result<Option<Value>> {
    let text: Option<String> = conn
        .query_row(
            "SELECT snapshot FROM connectors WHERE agent_id = ?1",
            params![agent_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    text.map(|t| serde_json::from_str(&t).map_err(|_| Error::IntegrityFailed))
        .transpose()
}

fn storage(error: rusqlite::Error) -> Error {
    Error::Unavailable(format!("agents: {error}"))
}
