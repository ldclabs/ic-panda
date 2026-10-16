//! `dmsg daemon`: the only process that holds keys and state.
//!
//! It listens on a loopback port. `/daemon/status`, `/app/v1` (JSON-RPC 2.0
//! over HTTP POST) and `/ws/app/v1` (the same methods over a WebSocket, with
//! events) need the owner credential and refuse any request with an
//! `Origin`, so web pages cannot even learn whether it runs. The login
//! callback is open only while a login is pending, accepts one POST from its
//! own loopback origin and then closes (`docs/protocol/login-v1_zh.md` §4).
//!
//! Idle for `idle_lock_minutes`, the daemon locks; versions already signed
//! for submission are still sent by a keyless flush, and with
//! `background_online` the device key stays for sending and fetching
//! ciphertext. `sync_interval_minutes` adds a scheduled sync.
//!
//! `daemon/stop` with `if_idle` (updates and restarts) first refuses new
//! requests and background passes, waits for the running operation, and
//! then stops only if nothing approved, no account update and no root
//! rotation is left unsettled; otherwise it answers what is in flight and
//! keeps running (plan DT10). Without `if_idle` it stops after the running
//! operation; unsettled work resumes after the restart either way.
//!
//! Approval requests are answered only through `approval/respond` with
//! `human: true`, which only the desktop approval window and an interactive
//! terminal send. An agent's MCP session (`dmsg mcp`) presents its own
//! credential instead of the owner's, which reaches only `mcp/tools` and
//! `mcp/call` as that Agent ID. The daemon keeps a WebSocket to the relay's
//! hint channel with a bearer token that receives content-free hints, locked
//! or not: a `connect` hint fetches the mailbox, and a `channel` or `inbox`
//! hint, locked, tells the desktop to notify the user.
//!
//! Its run log ([`crate::logs`]) names each call's method, outcome and
//! duration, and each change of a background task's outcome.

use crate::home::{Discovery, Home, Settings};
use axum::{
    body::Bytes,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use dmsg_client::{
    account::login_targets,
    attest::{Bundle, StatementInput},
    error_code,
    login::{LoginSession, DEFAULT_TTL_MS},
    now_ms,
    profile::ProfileFields,
    vault::{Filter, Item, VaultEntry},
    Client,
};
use dmsg_types::{user::ControllerRole, AccountId, Error, Hash, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use tokio::sync::{broadcast, Mutex, Notify};

/// How often presigned submissions are flushed.
const FLUSH_MS: u64 = 10_000;
/// How often the relay mailbox is polled besides hints.
const CONNECT_POLL_MS: u64 = 30_000;
/// The app protocol `initialize` reports.
const APP_PROTOCOL: &str = "dmsg-app/1";

/// The JSON-RPC methods of `/app/v1` and `/ws/app/v1`.
const METHODS: [&str; 56] = [
    "session/status",
    "session/login",
    "session/login_result",
    "session/accounts",
    "session/switch",
    "session/unlock",
    "session/lock",
    "session/fast_unlock",
    "account/create",
    "account/show",
    "device/pair_request",
    "device/approve",
    "vault/put",
    "vault/edit",
    "vault/delete",
    "vault/restore",
    "vault/list",
    "vault/show",
    "vault/conflicts",
    "vault/resolve",
    "file/import",
    "file/export",
    "file/verify",
    "sync/run",
    "sync/status",
    "profile/show",
    "profile/publish",
    "profile/resolve",
    "profile/avatar",
    "profile/avatar_image",
    "workspace/reset",
    "daemon/stop",
    "approval/list",
    "approval/show",
    "approval/respond",
    "approval/cancel",
    "approval/reconcile",
    "attest/request",
    "attest/action",
    "attest/history",
    "attest/export",
    "attest/verify",
    "device/capabilities",
    "device/formal_approve",
    "device/revoke",
    "connect/invite",
    "connect/candidates",
    "connect/grant",
    "connect/pairings",
    "connect/revoke",
    "connect/sync",
    "connect/status",
    "hint/status",
    "app/initialize",
    "daemon/status",
    "events/poll",
];

const CALLBACK_PAGE: &str = r#"<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width">
<title>dMsg</title>
<p id="m">Connecting to dMsg…</p>
<script>
(async () => {
  const m = document.getElementById('m')
  const text = location.hash.slice(1).replace(/-/g, '+').replace(/_/g, '/')
  history.replaceState(null, '', location.pathname)
  try {
    const body = Uint8Array.from(atob(text), (c) => c.charCodeAt(0))
    const r = await fetch('/auth/callback', {
      method: 'POST',
      headers: { 'content-type': 'application/cbor' },
      body
    })
    m.textContent = r.ok ? 'Signed in to dMsg. You can close this page.' : 'Login failed: ' + (await r.text())
  } catch {
    m.textContent = 'Login failed.'
  }
})()
</script>
"#;

#[derive(Default)]
struct LoginState {
    session: Option<LoginSession>,
    /// A login started to bind another identity to the unlocked account,
    /// with the source of the approval it asks for.
    bind: Option<String>,
    outcome: Option<std::result::Result<(), String>>,
    /// The approval request a binding login created.
    approval: Option<Value>,
}

pub(crate) struct Daemon {
    token: String,
    instance: String,
    port: u16,
    settings: Settings,
    client: Mutex<Client>,
    login: std::sync::Mutex<LoginState>,
    activity: AtomicU64,
    /// Set while a stop decides; new requests and background passes wait it out.
    stopping: AtomicBool,
    stop: Notify,
    events: broadcast::Sender<Value>,
    seq: AtomicU64,
    hints: std::sync::Mutex<HintState>,
    wake: Notify,
    /// The latest mailbox pass, refusal codes included; nothing secret.
    last_connect: std::sync::Mutex<Option<Value>>,
    /// The Agent Protocols connectors of the hosted Agent IDs.
    agents: dmsg_agent::Runtime,
    /// Counts the connector calls, for agents watching their resources.
    connectors: tokio::sync::watch::Sender<u64>,
    /// MCP Events: the fetch loop's signals and upstream keys.
    mcp: crate::mcp_events::McpEvents,
    /// The last outcome each background task logged.
    outcomes: std::sync::Mutex<std::collections::BTreeMap<&'static str, String>>,
}

/// Who presented a credential.
enum Session {
    /// The owner: the CLI and the desktop.
    Owner,
    /// A hosted Agent ID's MCP session.
    Agent(dmsg_client::agents::AgentEntry),
}

/// Where the hint channel stands; nothing secret.
#[derive(Default, Clone, serde::Serialize)]
pub(crate) struct HintState {
    pub(crate) connected: bool,
    pub(crate) received: u64,
    pub(crate) last_error: Option<String>,
}

#[derive(Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

/// Run the daemon in the foreground until stopped.
///
/// # Errors
/// Returns `Error::Locked` when another daemon serves this home.
pub async fn run(home: Home, port: u16) -> Result<()> {
    // Two TLS providers are linked; the hint channel takes the default one.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let settings = home.settings()?;
    let _lock = home.lock()?;
    crate::logs::open(&home.dir).map_err(|e| Error::Unavailable(format!("logs: {e}")))?;
    let token = home.owner_token(&settings, true)?;
    let client = Client::open(
        settings.config.clone(),
        &home.workspace(&settings.config),
        Home::secure(&settings),
    )?;
    let local_roots = match &settings.agent_ca {
        Some(path) => {
            vec![std::fs::read(path).map_err(|e| Error::InvalidInput(format!("agent_ca: {e}")))?]
        }
        None => Vec::new(),
    };
    let mcp = crate::mcp_events::McpEvents::new(&local_roots)?;
    let agents = dmsg_agent::Runtime::new(&dmsg_agent::Options {
        hosts: settings.agent_hosts.clone(),
        blocked_senders: settings.agent_blocked_senders.clone(),
        local_roots,
        local: settings.config.environment == dmsg_types::Environment::Local,
    })?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| Error::Unavailable(format!("listen: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| Error::Unavailable(format!("listen: {e}")))?
        .port();
    let daemon = Arc::new(Daemon {
        token,
        instance: hex::encode(&dmsg_client::crypto::random_id()[..8]),
        port,
        settings,
        client: Mutex::new(client),
        login: std::sync::Mutex::new(LoginState::default()),
        activity: AtomicU64::new(now_ms()),
        stopping: AtomicBool::new(false),
        stop: Notify::new(),
        events: broadcast::channel(256).0,
        seq: AtomicU64::new(0),
        hints: std::sync::Mutex::new(HintState::default()),
        wake: Notify::new(),
        last_connect: std::sync::Mutex::new(None),
        agents,
        connectors: tokio::sync::watch::channel(0).0,
        mcp,
        outcomes: Default::default(),
    });
    home.publish(&Discovery {
        port,
        pid: std::process::id(),
        instance: daemon.instance.clone(),
    })?;
    tokio::spawn(background(daemon.clone()));
    tokio::spawn(crate::hints::run(daemon.clone()));
    tokio::spawn(crate::channels::run(daemon.clone()));
    tokio::spawn(crate::mcp_events::run(daemon.clone()));
    let app = Router::new()
        .route("/daemon/status", get(status))
        .route("/app/v1", post(rpc))
        .route("/ws/app/v1", get(socket))
        .route("/auth/callback", get(callback_page).post(callback))
        .with_state(daemon.clone());
    eprintln!("dmsg daemon listening on 127.0.0.1:{port}");
    crate::logs::write(
        "daemon/start",
        &[
            ("version", &env!("CARGO_PKG_VERSION")),
            ("port", &port),
            ("pid", &std::process::id()),
        ],
    );
    let stopping = daemon.clone();
    let served = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminated() => {}
                _ = stopping.stop.notified() => {}
            }
        })
        .await;
    daemon.client.lock().await.shutdown();
    home.unpublish();
    crate::logs::write("daemon/stop", &[]);
    served.map_err(|e| Error::Unavailable(format!("serve: {e}")))
}

/// Idle lock, the keyless flush of presigned submissions and the
/// scheduled sync. Background work is not activity.
async fn background(daemon: Arc<Daemon>) {
    let mut synced = now_ms();
    let mut polled = 0;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(FLUSH_MS)) => {}
            _ = daemon.wake.notified() => polled = 0,
        }
        if daemon.stopping() {
            continue;
        }
        let now = now_ms();
        let mut client = daemon.client.lock().await;
        let before = client.status().map(|s| s.state).unwrap_or_default();
        if now > daemon.activity.load(Ordering::Relaxed) + daemon.settings.idle_lock_ms {
            client.lock();
        }
        let status = client.status().ok();
        if status.as_ref().map(|s| s.state) != Some(before) {
            let state = status.as_ref().map_or("unknown", |s| s.state);
            crate::logs::write("session/changed", &[("state", &state)]);
            daemon.emit(
                "session/changed",
                json!({"state": status.as_ref().map(|s| s.state)}),
            );
        }
        let interval = daemon.settings.sync_interval_ms;
        let unlocked = status.as_ref().is_some_and(|s| s.state == "unlocked");
        let ready = status.as_ref().is_some_and(|s| unlocked || s.resident);
        if interval > 0 && now >= synced + interval && ready {
            synced = now;
            // Recorded in the status; a failure waits for the next round.
            let synced = client.sync().await;
            daemon.note("sync", &outcome(&synced));
            if synced.is_ok() {
                daemon.emit("sync/changed", Value::Null);
            }
        } else {
            let flushed = client.flush().await;
            daemon.note("flush", &outcome(&flushed));
            if let Ok(settled @ 1..) = flushed {
                crate::logs::write("flush", &[("settled", &settled)]);
            }
        }
        if ready && now >= polled + CONNECT_POLL_MS {
            polled = now;
            let report = client.connect_sync().await;
            daemon.note("connect", &outcome(&report));
            if let Ok(report) = report {
                if report.fetched > 0 {
                    *daemon.last_connect.lock().expect("connect report") = Some(to_json(&report));
                }
                if report.queued + report.refused + report.replied > 0 || report.sealed > 0 {
                    daemon.emit("approval/changed", json!({"connect": report}));
                }
            }
            if unlocked && client.approvals_reconcile().await.is_ok() {
                daemon.emit("approval/changed", Value::Null);
            }
        }
    }
}

/// SIGTERM from a service manager; never on other platforms.
async fn terminated() {
    #[cfg(unix)]
    if let Ok(mut signal) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        signal.recv().await;
        return;
    }
    std::future::pending::<()>().await
}

impl Daemon {
    /// Publish an event to every WebSocket client.
    pub(crate) fn emit(&self, kind: &str, data: Value) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self
            .events
            .send(json!({"seq": seq, "type": kind, "data": data}));
    }

    /// Whether a stop is deciding or decided.
    pub(crate) fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    /// Stop after the running operation. With `if_idle`, keep running and
    /// answer what is in flight when anything is.
    async fn stop(&self, if_idle: bool) -> Result<Value> {
        self.stopping.store(true, Ordering::SeqCst);
        // Waiting readers return now instead of at their deadline.
        self.mcp.arrived.notify_waiters();
        let in_flight = match self.client.lock().await.in_flight() {
            Ok(in_flight) => in_flight,
            Err(error) => {
                self.stopping.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };
        if if_idle && in_flight.any() {
            self.stopping.store(false, Ordering::SeqCst);
            return Ok(json!({"stopped": false, "in_flight": in_flight}));
        }
        self.stop.notify_one();
        Ok(json!({"stopped": true, "in_flight": in_flight}))
    }

    /// Log a background task's outcome when it differs from the last one,
    /// so a task failing every round while offline logs once.
    pub(crate) fn note(&self, task: &'static str, outcome: &str) {
        let mut outcomes = self.outcomes.lock().expect("outcomes");
        if outcomes.get(task).map(String::as_str) != Some(outcome) {
            outcomes.insert(task, outcome.to_string());
            crate::logs::write(task, &[("outcome", &outcome)]);
        }
    }

    /// Wake the background loop, such as on a hint.
    pub(crate) fn wake(&self) {
        self.wake.notify_one();
    }

    pub(crate) fn client(&self) -> &Mutex<Client> {
        &self.client
    }

    pub(crate) fn settings(&self) -> &Settings {
        &self.settings
    }

    pub(crate) fn agents(&self) -> &dmsg_agent::Runtime {
        &self.agents
    }

    pub(crate) fn connectors(&self) -> &tokio::sync::watch::Sender<u64> {
        &self.connectors
    }

    pub(crate) fn mcp(&self) -> &crate::mcp_events::McpEvents {
        &self.mcp
    }

    pub(crate) fn set_hints(&self, f: impl FnOnce(&mut HintState)) {
        f(&mut self.hints.lock().expect("hint state"));
    }

    /// The owner credential and no `Origin`.
    fn owner(&self, headers: &HeaderMap) -> bool {
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or_default();
        headers.get(header::ORIGIN).is_none()
            && presented.len() == self.token.len()
            && presented
                .bytes()
                .zip(self.token.bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    }

    /// The owner's credential, else a hosted agent's; never with an `Origin`.
    async fn session(&self, headers: &HeaderMap) -> Option<Session> {
        if self.owner(headers) {
            return Some(Session::Owner);
        }
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))?;
        if headers.get(header::ORIGIN).is_some() {
            return None;
        }
        let agent = self
            .client
            .lock()
            .await
            .agent_by_credential(presented)
            .ok()??;
        Some(Session::Agent(agent))
    }

    fn pending_login(&self) -> bool {
        let login = self.login.lock().expect("login state");
        login
            .session
            .as_ref()
            .is_some_and(|s| s.deadline > now_ms())
    }

    async fn status(&self) -> Result<Value> {
        let status = self.client.lock().await.status()?;
        let mut value = serde_json::to_value(status).expect("status serializes");
        value["version"] = env!("CARGO_PKG_VERSION").into();
        value["instance"] = self.instance.clone().into();
        value["port"] = self.port.into();
        value["hints"] = serde_json::to_value(self.hints.lock().expect("hint state").clone())
            .expect("hint state serializes");
        // While unlocked, when the idle lock comes; the background pass
        // checks it every FLUSH_MS.
        if value["state"] == "unlocked" {
            value["lock_at"] =
                (self.activity.load(Ordering::Relaxed) + self.settings.idle_lock_ms).into();
            value["idle_lock_ms"] = self.settings.idle_lock_ms.into();
        }
        Ok(value)
    }

    async fn start_login(&self, params: &Value) -> Result<Value> {
        let origin = params["origin"].as_str().unwrap_or("https://dmsg.net");
        let config = self.client.lock().await.config().clone();
        let targets = login_targets(&config).await?;
        let session = LoginSession::new(&targets, origin, self.port, DEFAULT_TTL_MS, now_ms())?;
        let reply = json!({"url": session.url(&config), "expires_at": session.deadline});
        let bind = match params["bind"] == true {
            true => Some(crate::account_methods::source(params)?.to_string()),
            false => None,
        };
        *self.login.lock().expect("login state") = LoginState {
            session: Some(session),
            bind,
            outcome: None,
            approval: None,
        };
        Ok(reply)
    }

    async fn call(&self, method: &str, params: &Value) -> Result<Value> {
        let text = |name: &str| {
            params[name]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| Error::InvalidInput(format!("missing {name}")))
        };
        let path = |name: &str| {
            let path = std::path::PathBuf::from(text(name)?);
            match path.is_absolute() {
                true => Ok(path),
                false => Err(Error::InvalidInput(format!("{name} must be absolute"))),
            }
        };
        let reveal = params["reveal"].as_bool().unwrap_or(false);
        match method {
            "session/status" => self.status().await,
            "session/login" => self.start_login(params).await,
            "session/login_result" => {
                let (pending, outcome, approval) = {
                    let login = self.login.lock().expect("login state");
                    let pending = login
                        .session
                        .as_ref()
                        .is_some_and(|s| s.deadline > now_ms());
                    (pending, login.outcome.clone(), login.approval.clone())
                };
                Ok(json!({
                    "pending": pending,
                    "error": outcome.and_then(|o| o.err()),
                    "approval": approval,
                    "status": self.status().await?,
                }))
            }
            "session/accounts" => Ok(to_json(&self.client.lock().await.accounts()?)),
            "session/switch" => {
                let account = account_id(&text("account")?)?;
                let status = self.client.lock().await.switch_account(&account)?;
                *self.last_connect.lock().expect("connect report") = None;
                Ok(to_json(&status))
            }
            "session/unlock" => {
                let mut client = self.client.lock().await;
                // Fast unlock of another account here selects it first.
                if let Some(account) = params["account"].as_str() {
                    client.switch_account(&account_id(account)?)?;
                    *self.last_connect.lock().expect("connect report") = None;
                }
                let status = match params["fast"].as_bool().unwrap_or(false) {
                    true => client.unlock_fast().await?,
                    false => client.unlock().await?,
                };
                Ok(to_json(&status))
            }
            "session/lock" => {
                let mut client = self.client.lock().await;
                client.lock();
                // Versions already signed for submission still go out.
                let _ = client.flush().await;
                drop(client);
                self.status().await
            }
            "session/fast_unlock" => {
                let mut client = self.client.lock().await;
                let status = match params["enable"].as_bool() {
                    Some(true) => client.enable_fast_unlock()?,
                    Some(false) => client.disable_fast_unlock()?,
                    None => return Err(Error::InvalidInput("missing enable".into())),
                };
                Ok(to_json(&status))
            }
            "account/create" => {
                let mut client = self.client.lock().await;
                let account = client.create_account().await?;
                let generation = client.rotate_root().await?;
                Ok(json!({"account": account.id.to_string(), "root_generation": generation}))
            }
            "account/show" => {
                let view = self.client.lock().await.account_view().await?;
                Ok(to_json(&view))
            }
            "device/pair_request" => {
                let role = match params["role"].as_str() {
                    Some("member") => ControllerRole::Member,
                    _ => ControllerRole::Administrator,
                };
                let mut client = self.client.lock().await;
                let token = client.pair_request(role).await?;
                Ok(json!({"token": token, "device_id": client.status()?.device_id}))
            }
            "device/approve" => {
                let mut client = self.client.lock().await;
                let request = client.inspect_pairing(&text("token")?)?;
                // The owner names the device shown on the new device's screen.
                if hex::encode(*request.device.device_id) != text("device")? {
                    return Err(Error::InvalidInput(
                        "device does not match the request".into(),
                    ));
                }
                client.approve_pairing(&request).await?;
                let generation = client.rotate_root().await?;
                Ok(
                    json!({"device_id": hex::encode(*request.device.device_id), "root_generation": generation}),
                )
            }
            "vault/put" => {
                let item: Item = serde_json::from_value(params["item"].clone())
                    .map_err(|e| Error::InvalidInput(format!("item: {e}")))?;
                let entry = self.client.lock().await.vault_put(
                    params["id"].as_str(),
                    params["base"].as_str(),
                    &item,
                )?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/edit" => {
                let id = text("id")?;
                let mut client = self.client.lock().await;
                let mut item = client.vault_show(&id)?.item;
                edit(&mut item, &params["changes"])?;
                let entry = client.vault_put(Some(&id), params["base"].as_str(), &item)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/delete" => {
                let entry = self.client.lock().await.vault_delete(&text("id")?)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/restore" => {
                let entry = self.client.lock().await.vault_restore(&text("id")?)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/list" => {
                let filter = Filter {
                    trash: params["trash"].as_bool().unwrap_or(false),
                    query: params["query"].as_str().map(str::to_string),
                };
                let entries = self.client.lock().await.vault_list(&filter)?;
                Ok(masked_all(entries))
            }
            "vault/show" => {
                let entry = self.client.lock().await.vault_show(&text("id")?)?;
                Ok(to_json(&entry.masked(reveal)))
            }
            "vault/conflicts" => {
                let entries = self.client.lock().await.vault_conflicts()?;
                Ok(masked_all(entries))
            }
            "vault/resolve" => {
                let keep = match params["keep"].as_str() {
                    Some("current") => true,
                    Some("theirs") => false,
                    _ => return Err(Error::InvalidInput("keep: current or theirs".into())),
                };
                let entry = self
                    .client
                    .lock()
                    .await
                    .vault_resolve(&text("key")?, keep)?;
                Ok(to_json(&entry.masked(false)))
            }
            "file/import" => {
                let entry = self.client.lock().await.file_import(&path("path")?)?;
                Ok(to_json(&entry.masked(false)))
            }
            "file/export" => {
                let manifest = self
                    .client
                    .lock()
                    .await
                    .file_export(&text("id")?, &path("path")?)?;
                Ok(to_json(&manifest))
            }
            "file/verify" => {
                let manifest = self.client.lock().await.file_verify(&text("id")?)?;
                Ok(to_json(&manifest))
            }
            "sync/run" => {
                let report = self.client.lock().await.sync().await?;
                Ok(to_json(&report))
            }
            "sync/status" => {
                let client = self.client.lock().await;
                let status = client.status()?;
                Ok(json!({
                    "pending": status.pending_sync,
                    "conflicts": status.conflicts,
                    "last_sync": status.last_sync,
                    "resident": status.resident,
                    "interval_ms": self.settings.sync_interval_ms,
                }))
            }
            "profile/show" => {
                let profile = self.client.lock().await.profile().await?;
                Ok(to_json(&profile))
            }
            "profile/publish" => {
                let fields: ProfileFields = serde_json::from_value(params.clone())
                    .map_err(|e| Error::InvalidInput(format!("profile: {e}")))?;
                let profile = self.client.lock().await.publish_profile(&fields).await?;
                Ok(to_json(&profile))
            }
            "profile/resolve" => {
                let account = account_id(&text("account")?)?;
                let profile = self.client.lock().await.profile_resolve(&account).await?;
                Ok(to_json(&profile))
            }
            "profile/avatar" => {
                let image = match params["remove"] == true {
                    true => None,
                    false => {
                        let path = path("path")?;
                        let size = std::fs::metadata(&path)
                            .map_err(|e| Error::InvalidInput(format!("path: {e}")))?
                            .len();
                        if size > dmsg_client::profile::MAX_AVATAR as u64 {
                            return Err(Error::InvalidInput("avatar".into()));
                        }
                        Some(
                            std::fs::read(&path)
                                .map_err(|e| Error::InvalidInput(format!("path: {e}")))?,
                        )
                    }
                };
                let profile = self
                    .client
                    .lock()
                    .await
                    .publish_avatar(image.as_deref())
                    .await?;
                Ok(to_json(&profile))
            }
            "profile/avatar_image" => {
                let account = account_id(&text("account")?)?;
                let image = self
                    .client
                    .lock()
                    .await
                    .avatar(&account, &text("upload")?)
                    .await?;
                // A data URL the desktop shows without fetching anything.
                Ok(image.map_or(Value::Null, |(mime, bytes)| {
                    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                    json!({"mime": mime, "url": format!("data:{mime};base64,{data}")})
                }))
            }
            "workspace/reset" => {
                let mut client = self.client.lock().await;
                // The caller names this device, as shown in the status.
                if Some(text("device")?) != client.status()?.device_id {
                    return Err(Error::InvalidInput("device does not match".into()));
                }
                Ok(to_json(&client.reset()?))
            }
            "daemon/stop" => self.stop(params["if_idle"] == true).await,
            "daemon/status" => self.status().await,
            "app/initialize" => Ok(json!({
                "protocol": APP_PROTOCOL,
                "daemonVersion": env!("CARGO_PKG_VERSION"),
                "instanceId": self.instance,
                "status": self.status().await?,
            })),
            "events/poll" => Ok(json!({"seq": self.seq.load(Ordering::Relaxed)})),
            "approval/list" => {
                let all = params["all"].as_bool().unwrap_or(false);
                let list = self.client.lock().await.approvals(all)?;
                Ok(Value::Array(list))
            }
            "approval/show" => self.client.lock().await.approval(&text("id")?),
            "approval/respond" => {
                // Only a person answers: the desktop approval window or an
                // interactive terminal says so; scripts, agents and web
                // pages never do.
                if params["human"] != true {
                    return Err(Error::Forbidden);
                }
                let approve = params["approve"]
                    .as_bool()
                    .ok_or_else(|| Error::InvalidInput("approve".into()))?;
                let id = text("id")?;
                let mut client = self.client.lock().await;
                let outcome = client
                    .approval_respond(&id, &text("digest")?, approve)
                    .await;
                // Who decided, through which way in, on whose request.
                let request = client.approval(&id).unwrap_or_default();
                let detail = format!(
                    "{} from {}",
                    request["type"].as_str().unwrap_or("?"),
                    request["source"].as_str().unwrap_or("?")
                );
                let action = if approve {
                    "approval/approve"
                } else {
                    "approval/reject"
                };
                let code = outcome.as_ref().err().map_or("ok".into(), error_code);
                client.audit(
                    "owner",
                    &source(params)?,
                    action,
                    Some(&id),
                    Some(&detail),
                    &code,
                );
                outcome
            }
            "approval/cancel" => self.client.lock().await.approval_cancel(&text("id")?),
            "approval/reconcile" => {
                let mut client = self.client.lock().await;
                client.approvals_reconcile().await?;
                Ok(Value::Array(client.approvals(false)?))
            }
            "attest/request" => {
                let input: StatementInput = serde_json::from_value(params["statement"].clone())
                    .map_err(|e| Error::InvalidInput(format!("statement: {e}")))?;
                let file = match params["file"].as_str() {
                    Some(_) => Some(path("file")?),
                    None => None,
                };
                self.client
                    .lock()
                    .await
                    .attest_request(&source(params)?, input, file.as_deref())
            }
            "attest/action" => {
                let action = dmsg_client::codec::unb64(&text("action")?)?;
                self.client
                    .lock()
                    .await
                    .attest_action_request(&source(params)?, &action)
                    .await
            }
            "attest/history" => Ok(Value::Array(self.client.lock().await.attest_history()?)),
            "attest/export" => {
                let bundle = self
                    .client
                    .lock()
                    .await
                    .attest_bundle(&text("execution")?)?;
                Ok(to_json(&bundle))
            }
            "attest/verify" => {
                let bundle: Bundle = serde_json::from_value(params["bundle"].clone())
                    .map_err(|e| Error::InvalidInput(format!("bundle: {e}")))?;
                let file = match params["file"].as_str() {
                    Some(_) => Some(path("file")?),
                    None => None,
                };
                let report = self
                    .client
                    .lock()
                    .await
                    .attest_verify(&bundle, file.as_deref())
                    .await?;
                Ok(to_json(&report))
            }
            "device/capabilities" => {
                let device = hash(&text("device")?)?;
                let capabilities = params["capabilities"]
                    .as_array()
                    .ok_or_else(|| Error::InvalidInput("capabilities".into()))?
                    .iter()
                    .map(|c| capability(c.as_str().unwrap_or_default()))
                    .collect::<Result<Vec<_>>>()?;
                self.client
                    .lock()
                    .await
                    .capabilities_request(&source(params)?, device, capabilities)
                    .await
            }
            "device/formal_approve" => {
                let enable = params["enable"]
                    .as_bool()
                    .ok_or_else(|| Error::InvalidInput("enable".into()))?;
                self.client
                    .lock()
                    .await
                    .formal_approve_request(&source(params)?, enable)
                    .await
            }
            "device/revoke" => {
                let device = hash(&text("device")?)?;
                self.client
                    .lock()
                    .await
                    .revoke_request(&source(params)?, device)
                    .await
            }
            "connect/invite" => self.client.lock().await.connect_invite().await,
            "connect/candidates" => {
                self.client
                    .lock()
                    .await
                    .connect_candidates(&text("invite")?)
                    .await
            }
            "connect/grant" => {
                // The person confirmed the label shown on this device.
                if params["human"] != true {
                    return Err(Error::Forbidden);
                }
                self.client
                    .lock()
                    .await
                    .connect_grant(&text("invite")?, &text("pairing")?)
                    .await
            }
            "connect/pairings" => self.client.lock().await.connect_pairings().await,
            "connect/revoke" => {
                self.client
                    .lock()
                    .await
                    .connect_revoke(&text("pairing")?)
                    .await
            }
            "connect/sync" => {
                let report = to_json(&self.client.lock().await.connect_sync().await?);
                *self.last_connect.lock().expect("connect report") = Some(report.clone());
                Ok(report)
            }
            "connect/status" => {
                let status = self.client.lock().await.status()?;
                Ok(json!({
                    "approvals_waiting": status.approvals_waiting,
                    "sealed_requests": status.sealed_requests,
                    "hints": self.hints.lock().expect("hint state").clone(),
                    "last_sync": self.last_connect.lock().expect("connect report").clone(),
                }))
            }
            "hint/status" => Ok(to_json(&self.hints.lock().expect("hint state").clone())),
            m if crate::account_methods::METHODS.contains(&m) => {
                self.call_account(method, params).await
            }
            m if crate::legacy_methods::METHODS.contains(&m) => {
                self.call_legacy(method, params).await
            }
            m if crate::agents::METHODS.contains(&m) => self.call_agents(method, params).await,
            m if crate::mcp_events::METHODS.contains(&m) => {
                self.call_mcp_events(method, params).await
            }
            _ => self.call_more(method, params).await,
        }
    }
}

/// The local source a request names: the CLI or the desktop.
fn source(params: &Value) -> Result<String> {
    match params["source"].as_str() {
        Some("gui") => Ok("gui".into()),
        Some("cli") | None => Ok("cli".into()),
        Some(_) => Err(Error::InvalidInput("source".into())),
    }
}

fn account_id(text: &str) -> Result<AccountId> {
    text.parse()
        .map_err(|_| Error::InvalidInput("account ID".into()))
}

fn hash(text: &str) -> Result<Hash> {
    hex::decode(text)
        .ok()
        .and_then(|b| b.try_into().ok())
        .map(Hash::new)
        .ok_or_else(|| Error::InvalidInput("hex identifier".into()))
}

fn capability(name: &str) -> Result<dmsg_types::user::Capability> {
    use dmsg_types::user::Capability;
    match name {
        "content_sign" | "ContentSign" => Ok(Capability::ContentSign),
        "vault_unlock" | "VaultUnlock" => Ok(Capability::VaultUnlock),
        "root_manage" | "RootManage" => Ok(Capability::RootManage),
        "formal_approve" | "FormalApprove" => Ok(Capability::FormalApprove),
        "payment_offer" | "PaymentOffer" => Ok(Capability::PaymentOffer),
        _ => Err(Error::InvalidInput(format!("capability {name}"))),
    }
}

/// The events a method's success announces.
fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "session/status"
        | "session/login"
        | "session/login_result"
        | "session/accounts"
        | "session/fast_unlock" => &[],
        m if m.starts_with("session/") || m == "workspace/reset" || m == "account/create" => {
            &["session/changed", "approval/changed"]
        }
        m if m.starts_with("vault/") || m.starts_with("file/") => &["vault/changed"],
        "sync/run" => &["sync/changed", "vault/changed"],
        "approval/respond"
        | "approval/cancel"
        | "approval/reconcile"
        | "attest/request"
        | "attest/action"
        | "device/capabilities"
        | "device/formal_approve"
        | "device/revoke"
        | "connect/sync" => &["approval/changed"],
        "connect/grant" | "connect/revoke" => &["connect/changed"],
        "device/approve" => &["session/changed"],
        m if crate::account_methods::METHODS.contains(&m) => crate::account_methods::events_of(m),
        m if crate::legacy_methods::METHODS.contains(&m) => crate::legacy_methods::events_of(m),
        m if m.starts_with("agent/")
            || m.starts_with("grants/")
            || m.starts_with("ap/")
            || m.starts_with("mcp/") =>
        {
            crate::agents::events_of(m)
        }
        m => crate::methods::events_of(m),
    }
}

/// Apply the fields present in `changes` to an item.
fn edit(item: &mut Item, changes: &Value) -> Result<()> {
    let changes = changes
        .as_object()
        .ok_or_else(|| Error::InvalidInput("changes".into()))?;
    for (name, value) in changes {
        let text = || {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| Error::InvalidInput(name.clone()))
        };
        match name.as_str() {
            "title" => item.title = text()?,
            "body" => item.body = text()?,
            "username" => item.username = text()?,
            "secret" => item.secret = text()?,
            "url" => item.url = text()?,
            "favorite" => {
                item.favorite = value
                    .as_bool()
                    .ok_or_else(|| Error::InvalidInput(name.clone()))?
            }
            "tags" => {
                item.tags = serde_json::from_value(value.clone())
                    .map_err(|_| Error::InvalidInput(name.clone()))?
            }
            _ => return Err(Error::InvalidInput(name.clone())),
        }
    }
    item.updated_at = now_ms();
    Ok(())
}

/// Entries as they leave the daemon: no secret, no file key.
fn masked_all(entries: Vec<VaultEntry>) -> Value {
    to_json(
        &entries
            .into_iter()
            .map(|e| e.masked(false))
            .collect::<Vec<_>>(),
    )
}

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializable")
}

async fn status(State(daemon): State<Arc<Daemon>>, headers: HeaderMap) -> Response {
    if !daemon.owner(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match daemon.status().await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::SERVICE_UNAVAILABLE, error.to_string()).into_response(),
    }
}

async fn rpc(State(daemon): State<Arc<Daemon>>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(session) = daemon.session(&headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(request) = serde_json::from_slice::<RpcRequest>(&body) else {
        return Json(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}))
            .into_response();
    };
    Json(daemon.dispatch(request, &session).await).into_response()
}

impl Daemon {
    /// Run one request and announce what it changed. An agent's session
    /// reaches only the agent methods, and the owner's never does.
    async fn dispatch(&self, request: RpcRequest, session: &Session) -> Value {
        if request.jsonrpc != "2.0" {
            return json!({"jsonrpc": "2.0", "id": request.id, "error": {"code": -32600, "message": "invalid request"}});
        }
        let method = request.method.as_str();
        let agent_method = crate::agents::AGENT_METHODS.contains(&method);
        if self.stopping()
            && !matches!(
                method,
                "daemon/status" | "session/status" | "app/initialize" | "events/poll"
            )
        {
            let error = Error::Unavailable("the daemon is stopping".into());
            return json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": {"code": -32000, "message": error.to_string(), "data": {"code": error_code(&error)}},
            });
        }
        if !METHODS.contains(&method)
            && !crate::methods::METHODS.contains(&method)
            && !crate::account_methods::METHODS.contains(&method)
            && !crate::legacy_methods::METHODS.contains(&method)
            && !crate::agents::METHODS.contains(&method)
            && !crate::mcp_events::METHODS.contains(&method)
            && !agent_method
        {
            return json!({"jsonrpc": "2.0", "id": request.id, "error": {"code": -32601, "message": "method not found"}});
        }
        if agent_method != matches!(session, Session::Agent(_)) {
            let error = Error::Forbidden;
            return json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": {"code": -32000, "message": error.to_string(), "data": {"code": error_code(&error)}},
            });
        }
        // Waiting readers are not activity: a stream would otherwise keep
        // the device unlocked forever.
        let waiting = (request.method == "mcp/call"
            && request.params["tool"] == "dmsg_events_next")
            || request.method == "mcp/changed";
        if !waiting
            && !matches!(
                request.method.as_str(),
                "events/poll" | "daemon/status" | "app/initialize" | "mcp-events/poll"
            )
        {
            self.activity.store(now_ms(), Ordering::Relaxed);
        }
        let started = std::time::Instant::now();
        let outcome = match session {
            Session::Owner => self.call(&request.method, &request.params).await,
            Session::Agent(agent) => {
                self.call_agent(agent, &request.method, &request.params)
                    .await
            }
        };
        log_call(
            method,
            &request.params,
            agent_method,
            &outcome,
            started.elapsed(),
        );
        match outcome {
            Ok(result) => {
                for kind in events_of(&request.method) {
                    self.emit(kind, json!({"method": request.method}));
                }
                json!({"jsonrpc": "2.0", "id": request.id, "result": result})
            }
            Err(error) => {
                // A failed decision may still have changed the request.
                if request.method == "approval/respond" {
                    self.emit("approval/changed", Value::Null);
                }
                json!({
                    "jsonrpc": "2.0",
                    "id": request.id,
                    "error": {"code": -32000, "message": error.to_string(), "data": {"code": error_code(&error)}},
                })
            }
        }
    }
}

/// `ok`, or the error code.
pub(crate) fn outcome<T>(result: &Result<T>) -> String {
    result.as_ref().err().map_or("ok".into(), error_code)
}

/// Log a call by its method, the tool an agent ran, its outcome and how
/// long it took; the steady polls only when they fail.
fn log_call(
    method: &str,
    params: &Value,
    agent: bool,
    result: &Result<Value>,
    elapsed: std::time::Duration,
) {
    let code = outcome(result);
    let tool = params["tool"]
        .as_str()
        .filter(|_| matches!(method, "mcp/call" | "ap/call"))
        .filter(|t| t.len() <= 64 && t.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
    let quiet = matches!(
        method,
        "session/status"
            | "session/login_result"
            | "daemon/status"
            | "app/initialize"
            | "events/poll"
            | "hint/status"
            | "connect/status"
            | "sync/status"
            | "mcp-events/poll"
            | "mcp/changed"
    ) || tool == Some("dmsg_events_next");
    if quiet && code == "ok" {
        return;
    }
    let by = if agent { "agent" } else { "owner" };
    let ms = elapsed.as_millis();
    match tool {
        Some(tool) => crate::logs::write(
            "call",
            &[
                ("method", &method),
                ("tool", &tool),
                ("by", &by),
                ("outcome", &code),
                ("ms", &ms),
            ],
        ),
        None => crate::logs::write(
            "call",
            &[
                ("method", &method),
                ("by", &by),
                ("outcome", &code),
                ("ms", &ms),
            ],
        ),
    }
}

/// `/ws/app/v1`: JSON-RPC requests in, replies and `event` notifications out.
async fn socket(
    State(daemon): State<Arc<Daemon>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !daemon.owner(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    upgrade
        .max_message_size(4 * 1024 * 1024)
        .on_upgrade(move |socket| serve_socket(daemon, socket))
}

async fn serve_socket(daemon: Arc<Daemon>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let mut events = daemon.events.subscribe();
    let (replies, mut outgoing) = tokio::sync::mpsc::channel::<Value>(64);
    loop {
        tokio::select! {
            message = stream.next() => {
                let Some(Ok(message)) = message else { break };
                let Message::Text(text) = message else { continue };
                let Ok(request) = serde_json::from_str::<RpcRequest>(&text) else {
                    let _ = replies.send(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}})).await;
                    continue;
                };
                // Requests run concurrently so a slow one never stalls events.
                let daemon = daemon.clone();
                let replies = replies.clone();
                tokio::spawn(async move {
                    let _ = replies.send(daemon.dispatch(request, &Session::Owner).await).await;
                });
            }
            Some(reply) = outgoing.recv() => {
                if sink.send(Message::Text(reply.to_string().into())).await.is_err() {
                    break;
                }
            }
            event = events.recv() => {
                let note = match event {
                    Ok(event) => json!({"jsonrpc": "2.0", "method": "event", "params": event}),
                    // Missed events: the client fetches a fresh snapshot.
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        json!({"jsonrpc": "2.0", "method": "event", "params": {"type": "resync"}})
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if sink.send(Message::Text(note.to_string().into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn callback_page(State(daemon): State<Arc<Daemon>>) -> Response {
    if !daemon.pending_login() {
        return StatusCode::NOT_FOUND.into_response();
    }
    (
        [
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; script-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'",
            ),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(CALLBACK_PAGE),
    )
        .into_response()
}

async fn callback(State(daemon): State<Arc<Daemon>>, headers: HeaderMap, body: Bytes) -> Response {
    let now = now_ms();
    // One POST per login, whatever its fate.
    let Some(session) = daemon
        .login
        .lock()
        .expect("login state")
        .session
        .take()
        .filter(|s| s.deadline > now)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let bind = daemon.login.lock().expect("login state").bind.clone();
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let own = [
        format!("http://127.0.0.1:{}", daemon.port),
        format!("http://[::1]:{}", daemon.port),
    ];
    let cbor = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        == Some("application/cbor");
    let outcome = if !origin.is_some_and(|o| own.iter().any(|x| x == o)) || !cbor {
        Err(Error::Forbidden)
    } else {
        match session.accept(&body, now) {
            // A second login for the unlocked account: it waits for the
            // person to approve binding it.
            Ok(delegated) => match bind {
                Some(source) => daemon
                    .client
                    .lock()
                    .await
                    .bind_login(&source, delegated.identity, delegated.expires_at)
                    .await
                    .map(|approval| {
                        daemon.login.lock().expect("login state").approval = Some(approval);
                        daemon.emit("approval/changed", Value::Null);
                    }),
                None => {
                    let login = daemon
                        .client
                        .lock()
                        .await
                        .login(delegated.identity, delegated.expires_at)
                        .await
                        .map(|_| ());
                    // The login may have selected another account.
                    *daemon.last_connect.lock().expect("connect report") = None;
                    login
                }
            },
            Err(error) => Err(error),
        }
    };
    daemon.login.lock().expect("login state").outcome =
        Some(outcome.clone().map_err(|e| e.to_string()));
    match outcome {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(Error::Forbidden) => (StatusCode::FORBIDDEN, "refused").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}
