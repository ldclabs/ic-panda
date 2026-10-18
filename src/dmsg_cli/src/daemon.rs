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
use crate::params::{absolute, account, hash, optional, parse, source, text, to_json};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        DefaultBodyLimit, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{any, get, post},
};
use base64::Engine;
use dmsg_client::{
    Client,
    account::login_targets,
    attest::{Bundle, StatementInput},
    error_code,
    library::{Check, VersionCheck, blobs::Blobs},
    login::{DEFAULT_TTL_MS, LoginSession},
    now_ms,
    profile::ProfileFields,
    vault::{Filter, Item, VaultEntry},
};
use dmsg_types::{Error, Result, user::ControllerRole};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::{Mutex, Notify, broadcast};

/// How often presigned submissions are flushed.
const FLUSH_MS: u64 = 10_000;
/// How long one background pass uploads a backup before requests waiting
/// for the client go first; the next pass follows at once.
const BACKUP_SLICE_MS: u64 = 5_000;
/// How often the relay mailbox is polled besides hints.
const CONNECT_POLL_MS: u64 = 30_000;
/// The library's daily self-check waits for this much quiet at the device.
const CHECK_IDLE_MS: u64 = 60_000;
/// A self-check that could not finish is tried again after this long.
const CHECK_RETRY_MS: u64 = 3_600_000;
/// The app protocol `initialize` reports.
const APP_PROTOCOL: &str = "dmsg-app/1";
/// The largest request body and WebSocket message: an agent's 8 MiB
/// `dmsg_files_write` is about 11 MiB as base64 in JSON.
const MAX_REQUEST: usize = 16 * 1024 * 1024;

/// The JSON-RPC methods of `/app/v1` and `/ws/app/v1`.
const METHODS: [&str; 54] = [
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
    "app/activity",
    "daemon/status",
    "events/poll",
];

/// The module that serves a method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Module {
    Daemon,
    More,
    Account,
    Legacy,
    Agents,
    /// The methods an agent's credential reaches.
    Agent,
    McpEvents,
    Files,
    Mount,
    Wallet,
}

/// The module that serves `method`; `None` for an unknown method.
fn route(method: &str) -> Option<Module> {
    let modules: [(Module, &[&str]); 10] = [
        (Module::Daemon, &METHODS),
        (Module::More, &crate::methods::METHODS),
        (Module::Account, &crate::account_methods::METHODS),
        (Module::Legacy, &crate::legacy_methods::METHODS),
        (Module::Agents, &crate::agents::METHODS),
        (Module::Agent, &crate::agents::AGENT_METHODS),
        (Module::McpEvents, &crate::mcp_events::METHODS),
        (Module::Files, &crate::files_methods::METHODS),
        (Module::Mount, &crate::mount::METHODS),
        (Module::Wallet, &crate::wallet_methods::METHODS),
    ];
    modules
        .into_iter()
        .find(|(_, methods)| methods.contains(&method))
        .map(|(module, _)| module)
}

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
    /// The callback is checking what the browser returned.
    checking: bool,
    outcome: Option<Result<()>>,
    /// The approval request a binding login created.
    approval: Option<Value>,
}

pub(crate) struct Daemon {
    token: String,
    instance: String,
    port: u16,
    settings: Settings,
    client: Arc<Mutex<Client>>,
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
    /// The file library's loopback WebDAV mount, while mounted.
    pub(crate) mount: std::sync::Mutex<Option<crate::mount::Mount>>,
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
    #[allow(unused_mut)]
    let mut client = Client::open(
        settings.config.clone(),
        &home.workspace(&settings.config),
        Home::secure(&settings),
    )?;
    // With the system keystore, the Secure Enclave keeps cosigner keys and
    // the fast-unlock key, and asks for Touch ID or the login password.
    #[cfg(target_os = "macos")]
    if settings.storage == crate::home::Storage::System {
        let enclave = std::sync::Arc::new(crate::enclave::SecureEnclave);
        client.set_cosign_provider(enclave.clone());
        client.set_presence_key(enclave);
    }
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
    let daemon = Arc::new(Daemon::new(token, port, settings, client, agents, mcp));
    home.publish(&Discovery {
        port,
        pid: std::process::id(),
        instance: daemon.instance.clone(),
    })?;
    tokio::spawn(background(daemon.clone()));
    tokio::spawn(crate::hints::run(daemon.clone()));
    tokio::spawn(crate::channels::run(daemon.clone()));
    tokio::spawn(crate::mcp_events::run(daemon.clone()));
    let app = app(daemon.clone());
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
            // The system unmounts while the share still answers.
            stopping.unmount_library().await;
        })
        .await;
    daemon.client.lock().await.shutdown();
    home.unpublish();
    crate::logs::write("daemon/stop", &[]);
    served.map_err(|e| Error::Unavailable(format!("serve: {e}")))
}

/// The loopback routes.
fn app(daemon: Arc<Daemon>) -> Router {
    Router::new()
        .route("/daemon/status", get(status))
        .route(
            "/app/v1",
            post(rpc).layer(DefaultBodyLimit::max(MAX_REQUEST)),
        )
        .route("/ws/app/v1", get(socket))
        .route("/auth/callback", get(callback_page).post(callback))
        .route(
            &format!("/dav/{{token}}/{}", crate::mount::VOLUME),
            any(crate::mount::serve),
        )
        .route(
            &format!("/dav/{{token}}/{}/{{*rest}}", crate::mount::VOLUME),
            any(crate::mount::serve),
        )
        .with_state(daemon)
}

/// Idle lock, the keyless flush of presigned submissions and the
/// scheduled sync. Background work is not activity.
async fn background(daemon: Arc<Daemon>) {
    let mut synced = now_ms();
    let mut polled = 0;
    // Requests waiting at the last pass; one that expires changes the count.
    let mut waiting = 0;
    // When the last self-check began, finished or not.
    let mut checking = 0;
    let mut pause = FLUSH_MS;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(pause)) => {}
            _ = daemon.wake.notified() => polled = 0,
        }
        pause = FLUSH_MS;
        if daemon.stopping() {
            continue;
        }
        let now = now_ms();
        let mut client = daemon.client.lock().await;
        let before = client.session_state().state;
        if now > daemon.activity.load(Ordering::Relaxed) + daemon.settings.idle_lock_ms {
            client.lock();
        }
        let session = client.session_state();
        if session.state != before {
            crate::logs::write("session/changed", &[("state", &session.state)]);
            daemon.emit("session/changed", json!({"state": session.state}));
        }
        drop(client);
        daemon.check_mount().await;
        let mut client = daemon.client.lock().await;
        let interval = daemon.settings.sync_interval_ms;
        let unlocked = session.state == "unlocked";
        let ready = unlocked || session.resident;
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
            // Only news is announced: windows reload on this event.
            if unlocked {
                let reconciled = client.approvals_reconcile().await.unwrap_or(0);
                let count = client.approvals_waiting().unwrap_or(waiting);
                if reconciled > 0 || count != waiting {
                    daemon.emit("approval/changed", Value::Null);
                }
                waiting = count;
            }
        }
        // Backups a person asked for keep moving while the device key is
        // there, locked or not, a slice per pass; nothing here starts one.
        if ready && client.library_jobs_running().unwrap_or(false) {
            let mut progress = |file: &str, done, total| {
                daemon.emit(
                    "files/progress",
                    json!({"job": "backup", "path": file, "done": done, "total": total}),
                );
            };
            let run = client
                .files_backup_run(Some(BACKUP_SLICE_MS), &mut progress)
                .await;
            daemon.note("backup", &outcome(&run));
            if run.as_ref().is_ok_and(|r| r.completed > 0) {
                daemon.emit("files/changed", Value::Null);
            }
            if run.as_ref().is_ok_and(|r| r.paused) {
                pause = 0;
            }
        }
        if unlocked {
            // The file library's housekeeping: expired trash and unreferenced
            // chunks every ten minutes, opened copies every pass.
            let maintained = client.files_maintain(now);
            daemon.note("files", &outcome(&maintained));
            if let Ok(Some(report)) = &maintained
                && report.purged_files + report.purged_folders > 0
            {
                daemon.emit("files/changed", json!({"maintenance": report}));
            }
            if let Ok(changed) = client.files_open_sweep(now) {
                for copy in changed {
                    daemon.emit("files/open_changed", to_json(&copy));
                }
            }
        }
        // The library's self-check, once a day while nobody is at the
        // device.
        let quiet = now > daemon.activity.load(Ordering::Relaxed) + CHECK_IDLE_MS;
        if quiet && now >= checking + CHECK_RETRY_MS && client.files_check_due(now).unwrap_or(false)
        {
            checking = now;
            drop(client);
            let checked = daemon.check_library().await;
            daemon.note("check", &outcome(&checked));
            if let Ok((check, _)) = checked {
                crate::logs::write(
                    "files/check",
                    &[
                        ("blobs", &check.blobs_checked),
                        ("versions", &check.versions_checked),
                        ("bad", &(check.blobs_bad.len() + check.versions_bad.len())),
                    ],
                );
                daemon.emit("files/changed", json!({"check": check}));
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
    fn new(
        token: String,
        port: u16,
        settings: Settings,
        client: Client,
        agents: dmsg_agent::Runtime,
        mcp: crate::mcp_events::McpEvents,
    ) -> Self {
        Self {
            token,
            instance: hex::encode(&dmsg_client::crypto::random_id()[..8]),
            port,
            settings,
            client: Arc::new(Mutex::new(client)),
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
            mount: std::sync::Mutex::new(None),
        }
    }

    /// Check the whole library: every chunk file against its name beside
    /// the client, which needs no key, then every local version through
    /// its key while unlocked; the check is recorded.
    pub(crate) async fn check_library(&self) -> Result<(Check, Vec<VersionCheck>)> {
        let dir = self.client.lock().await.files_blobs_dir()?;
        let (count, bad) = tokio::task::spawn_blocking(move || Blobs::new(dir).verify())
            .await
            .map_err(|e| Error::Unavailable(format!("check: {e}")))??;
        self.client
            .lock()
            .await
            .files_check_record(now_ms(), count, bad)
    }

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

    /// The client, to share with a task that outlives a call.
    pub(crate) fn client_handle(&self) -> Arc<Mutex<Client>> {
        self.client.clone()
    }

    /// The loopback port.
    pub(crate) fn port(&self) -> u16 {
        self.port
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
        bearer(headers).is_some_and(|presented| constant_time_eq(presented, &self.token))
    }

    /// The owner's credential, else a hosted agent's; never with an `Origin`.
    async fn session(&self, headers: &HeaderMap) -> Option<Session> {
        let presented = bearer(headers)?;
        if constant_time_eq(presented, &self.token) {
            return Some(Session::Owner);
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
            true => Some(source(params)?.to_string()),
            false => None,
        };
        *self.login.lock().expect("login state") = LoginState {
            session: Some(session),
            bind,
            ..LoginState::default()
        };
        Ok(reply)
    }

    /// Run one of the owner's methods.
    async fn call(&self, module: Module, method: &str, params: &Value) -> Result<Value> {
        match module {
            Module::Daemon => self.call_daemon(method, params).await,
            Module::More => self.call_more(method, params).await,
            Module::Account => self.call_account(method, params).await,
            Module::Legacy => self.call_legacy(method, params).await,
            Module::Agents => self.call_agents(method, params).await,
            Module::McpEvents => self.call_mcp_events(method, params).await,
            Module::Files => self.call_files(method, params).await,
            Module::Mount => self.call_mount(method, params).await,
            Module::Wallet => self.call_wallet(method, params).await,
            // An agent's credential reaches these, never the owner's.
            Module::Agent => Err(Error::Forbidden),
        }
    }

    /// Run one of [`METHODS`].
    async fn call_daemon(&self, method: &str, params: &Value) -> Result<Value> {
        let reveal = params["reveal"].as_bool().unwrap_or(false);
        match method {
            "session/status" => self.status().await,
            "session/login" => self.start_login(params).await,
            "session/login_result" => {
                let (waiting, checking, outcome, approval) = {
                    let login = self.login.lock().expect("login state");
                    let waiting = login
                        .session
                        .as_ref()
                        .is_some_and(|s| s.deadline > now_ms());
                    let outcome = login.outcome.clone();
                    (waiting, login.checking, outcome, login.approval.clone())
                };
                let pending = waiting || checking;
                let done = outcome.is_some();
                let error = outcome.and_then(|o| o.err());
                // The check holds the client until it ends, and the status
                // would wait for it.
                let status = match pending {
                    true => Value::Null,
                    false => self.status().await?,
                };
                Ok(json!({
                    "pending": pending,
                    "checking": checking,
                    "done": done,
                    "error": error.map(|e| json!({"code": error_code(&e), "message": e.to_string()})),
                    "approval": approval,
                    "status": status,
                }))
            }
            "session/accounts" => Ok(to_json(&self.client.lock().await.accounts()?)),
            "session/switch" => {
                let account = account(params, "account")?;
                let status = self.client.lock().await.switch_account(&account)?;
                *self.last_connect.lock().expect("connect report") = None;
                Ok(to_json(&status))
            }
            "session/unlock" => {
                let mut client = self.client.lock().await;
                // Fast unlock of another account here selects it first.
                if let Some(account) = optional(params, "account", account)? {
                    client.switch_account(&account)?;
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
                self.unmount_library().await;
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
                let request = client.inspect_pairing(&text(params, "token")?)?;
                // The owner names the device shown on the new device's screen.
                if hex::encode(*request.device.device_id) != text(params, "device")? {
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
                let item: Item = parse(params, "item")?;
                let entry = self.client.lock().await.vault_put(
                    params["id"].as_str(),
                    params["base"].as_str(),
                    &item,
                )?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/edit" => {
                let id = text(params, "id")?;
                let mut client = self.client.lock().await;
                let mut item = client.vault_show(&id)?.item;
                edit(&mut item, &params["changes"])?;
                let entry = client.vault_put(Some(&id), params["base"].as_str(), &item)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/delete" => {
                let entry = self
                    .client
                    .lock()
                    .await
                    .vault_delete(&text(params, "id")?)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/restore" => {
                let entry = self
                    .client
                    .lock()
                    .await
                    .vault_restore(&text(params, "id")?)?;
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
                let entry = self.client.lock().await.vault_show(&text(params, "id")?)?;
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
                    .vault_resolve(&text(params, "key")?, keep)?;
                Ok(to_json(&entry.masked(false)))
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
                let account = account(params, "account")?;
                let profile = self.client.lock().await.profile_resolve(&account).await?;
                Ok(to_json(&profile))
            }
            "profile/avatar" => {
                let image = match params["remove"] == true {
                    true => None,
                    false => {
                        let path = absolute(params, "path")?;
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
                let account = account(params, "account")?;
                let image = self
                    .client
                    .lock()
                    .await
                    .avatar(&account, &text(params, "upload")?)
                    .await?;
                // A data URL the desktop shows without fetching anything.
                Ok(image.map_or(Value::Null, |(mime, bytes)| {
                    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                    json!({"mime": mime, "url": format!("data:{mime};base64,{data}")})
                }))
            }
            "workspace/reset" => {
                self.unmount_library().await;
                let mut client = self.client.lock().await;
                // The caller names this device, as shown in the status.
                if Some(text(params, "device")?) != client.status()?.device_id {
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
            "app/activity" => Ok(Value::Null),
            "events/poll" => Ok(json!({"seq": self.seq.load(Ordering::Relaxed)})),
            "approval/list" => {
                let all = params["all"].as_bool().unwrap_or(false);
                let list = self.client.lock().await.approvals(all)?;
                Ok(Value::Array(list))
            }
            // The CLI names a request by a unique prefix.
            "approval/show" => {
                let mut client = self.client.lock().await;
                let id = client.approval_id(&text(params, "id")?)?;
                client.approval(&id)
            }
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
                let id = text(params, "id")?;
                let mut client = self.client.lock().await;
                let outcome = client
                    .approval_respond(&id, &text(params, "digest")?, approve)
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
                    source(params)?,
                    action,
                    Some(&id),
                    Some(&detail),
                    &code,
                );
                outcome
            }
            "approval/cancel" => {
                let mut client = self.client.lock().await;
                let id = client.approval_id(&text(params, "id")?)?;
                client.approval_cancel(&id)
            }
            "approval/reconcile" => {
                let mut client = self.client.lock().await;
                client.approvals_reconcile().await?;
                Ok(Value::Array(client.approvals(false)?))
            }
            "attest/request" => {
                let input: StatementInput = parse(params, "statement")?;
                let file = optional(params, "file", absolute)?;
                self.client
                    .lock()
                    .await
                    .attest_request(source(params)?, input, file.as_deref())
            }
            "attest/action" => {
                let action = dmsg_client::codec::unb64(&text(params, "action")?)?;
                self.client
                    .lock()
                    .await
                    .attest_action_request(source(params)?, &action)
                    .await
            }
            "attest/history" => Ok(Value::Array(self.client.lock().await.attest_history()?)),
            "attest/export" => {
                let bundle = self
                    .client
                    .lock()
                    .await
                    .attest_bundle(&text(params, "execution")?)?;
                Ok(to_json(&bundle))
            }
            "attest/verify" => {
                let bundle: Bundle = parse(params, "bundle")?;
                let file = optional(params, "file", absolute)?;
                let report = self
                    .client
                    .lock()
                    .await
                    .attest_verify(&bundle, file.as_deref())
                    .await?;
                Ok(to_json(&report))
            }
            "device/capabilities" => {
                let device = hash(params, "device")?;
                let capabilities = params["capabilities"]
                    .as_array()
                    .ok_or_else(|| Error::InvalidInput("capabilities".into()))?
                    .iter()
                    .map(|c| capability(c.as_str().unwrap_or_default()))
                    .collect::<Result<Vec<_>>>()?;
                self.client
                    .lock()
                    .await
                    .capabilities_request(source(params)?, device, capabilities)
                    .await
            }
            "device/formal_approve" => {
                let enable = params["enable"]
                    .as_bool()
                    .ok_or_else(|| Error::InvalidInput("enable".into()))?;
                self.client
                    .lock()
                    .await
                    .formal_approve_request(source(params)?, enable)
                    .await
            }
            "device/revoke" => {
                let device = hash(params, "device")?;
                self.client
                    .lock()
                    .await
                    .revoke_request(source(params)?, device)
                    .await
            }
            "connect/invite" => self.client.lock().await.connect_invite().await,
            "connect/candidates" => {
                self.client
                    .lock()
                    .await
                    .connect_candidates(&text(params, "invite")?)
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
                    .connect_grant(&text(params, "invite")?, &text(params, "pairing")?)
                    .await
            }
            "connect/pairings" => self.client.lock().await.connect_pairings().await,
            "connect/revoke" => {
                self.client
                    .lock()
                    .await
                    .connect_revoke(&text(params, "pairing")?)
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
            _ => Err(Error::NotFound),
        }
    }
}

fn capability(name: &str) -> Result<dmsg_types::user::Capability> {
    use dmsg_types::user::Capability;
    match name {
        "content_sign" | "ContentSign" => Ok(Capability::ContentSign),
        "vault_unlock" | "VaultUnlock" => Ok(Capability::VaultUnlock),
        "root_manage" | "RootManage" => Ok(Capability::RootManage),
        "formal_approve" | "FormalApprove" => Ok(Capability::FormalApprove),
        "payment_offer" | "PaymentOffer" => Ok(Capability::PaymentOffer),
        "wallet_approve" | "WalletApprove" => Ok(Capability::WalletApprove),
        _ => Err(Error::InvalidInput(format!("capability {name}"))),
    }
}

/// The events a method's success announces; reads announce none, since
/// windows reload on them.
fn events_of(module: Module, method: &str) -> &'static [&'static str] {
    match module {
        Module::Daemon => match method {
            "session/switch" | "session/unlock" | "session/lock" | "workspace/reset"
            | "account/create" => &["session/changed", "approval/changed"],
            "vault/put" | "vault/edit" | "vault/delete" | "vault/restore" | "vault/resolve" => {
                &["vault/changed"]
            }
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
            "session/fast_unlock" | "device/approve" => &["session/changed"],
            _ => &[],
        },
        Module::More => crate::methods::events_of(method),
        Module::Account => crate::account_methods::events_of(method),
        Module::Legacy => crate::legacy_methods::events_of(method),
        Module::Agents | Module::Agent => crate::agents::events_of(method),
        Module::McpEvents => &[],
        Module::Files => crate::files_methods::events_of(method),
        Module::Mount => crate::mount::events_of(method),
        Module::Wallet => crate::wallet_methods::events_of(method),
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

/// The bearer credential of a request without an `Origin`: web pages
/// cannot even learn whether the daemon runs.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    if headers.contains_key(header::ORIGIN) {
        return None;
    }
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// Compare a presented secret without leaking where it differs.
pub(crate) fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
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
        return Json(reply_error(&Value::Null, -32700, "parse error")).into_response();
    };
    Json(daemon.dispatch(request, &session, false).await).into_response()
}

/// A JSON-RPC error reply.
fn reply_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// A method's failure, with the dMsg error code in `data`.
fn reply_failure(id: &Value, error: &Error) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": -32000, "message": error.to_string(), "data": {"code": error_code(error)}},
    })
}

/// Whether a request moves the idle lock. Waiting readers do not: a stream
/// would otherwise keep the device unlocked forever. A client that
/// `reports` its person's activity moves it only with `app/activity`.
fn is_activity(request: &RpcRequest, reports: bool) -> bool {
    let method = request.method.as_str();
    if reports {
        return method == "app/activity";
    }
    let waiting = (method == "mcp/call" && request.params["tool"] == "dmsg_events_next")
        || method == "mcp/changed";
    !waiting
        && !matches!(
            method,
            "events/poll" | "daemon/status" | "app/initialize" | "mcp-events/poll"
        )
}

impl Daemon {
    /// Run one request and announce what it changed. An agent's session
    /// reaches only the agent methods, and the owner's never does.
    async fn dispatch(&self, request: RpcRequest, session: &Session, reports: bool) -> Value {
        let id = &request.id;
        if request.jsonrpc != "2.0" {
            return reply_error(id, -32600, "invalid request");
        }
        let method = request.method.as_str();
        if self.stopping()
            && !matches!(
                method,
                "daemon/status" | "session/status" | "app/initialize" | "events/poll"
            )
        {
            return reply_failure(id, &Error::Unavailable("the daemon is stopping".into()));
        }
        let Some(module) = route(method) else {
            return reply_error(id, -32601, "method not found");
        };
        let agent_method = module == Module::Agent;
        if agent_method != matches!(session, Session::Agent(_)) {
            return reply_failure(id, &Error::Forbidden);
        }
        if is_activity(&request, reports) {
            self.activity.store(now_ms(), Ordering::Relaxed);
        }
        let started = std::time::Instant::now();
        let outcome = match session {
            Session::Owner => self.call(module, method, &request.params).await,
            Session::Agent(agent) => self.call_agent(agent, method, &request.params).await,
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
                for kind in events_of(module, method) {
                    self.emit(kind, json!({"method": method}));
                }
                json!({"jsonrpc": "2.0", "id": id, "result": result})
            }
            Err(error) => {
                // A failed decision may still have changed the request.
                if method == "approval/respond" {
                    self.emit("approval/changed", Value::Null);
                }
                reply_failure(id, &error)
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
            | "app/activity"
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
        .max_message_size(MAX_REQUEST)
        .on_upgrade(move |socket| serve_socket(daemon, socket))
}

async fn serve_socket(daemon: Arc<Daemon>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let mut events = daemon.events.subscribe();
    let (replies, mut outgoing) = tokio::sync::mpsc::channel::<Value>(64);
    // `app/initialize {activity: "reported"}`: the client, such as the
    // desktop with its reads on events, reports its person's activity.
    let mut reports = false;
    loop {
        tokio::select! {
            message = stream.next() => {
                let Some(Ok(message)) = message else { break };
                let Message::Text(text) = message else { continue };
                let Ok(request) = serde_json::from_str::<RpcRequest>(&text) else {
                    let _ = replies.send(reply_error(&Value::Null, -32700, "parse error")).await;
                    continue;
                };
                if request.method == "app/initialize" {
                    reports = request.params["activity"] == "reported";
                }
                // Requests run concurrently so a slow one never stalls events.
                let daemon = daemon.clone();
                let replies = replies.clone();
                tokio::spawn(async move {
                    let _ = replies.send(daemon.dispatch(request, &Session::Owner, reports).await).await;
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

/// The check runs in its own task: a browser that leaves before the answer
/// drops the request, which would leave the login checking and half applied.
async fn callback(State(daemon): State<Arc<Daemon>>, headers: HeaderMap, body: Bytes) -> Response {
    tokio::spawn(check_callback(daemon, headers, body))
        .await
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn check_callback(daemon: Arc<Daemon>, headers: HeaderMap, body: Bytes) -> Response {
    let now = now_ms();
    // One POST per login, whatever its fate.
    let (session, bind) = {
        let mut login = daemon.login.lock().expect("login state");
        let session = login.session.take().filter(|s| s.deadline > now);
        login.checking = session.is_some();
        (session, login.bind.clone())
    };
    let Some(session) = session else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // The windows show the check, then its outcome, through
    // `session/login_result`.
    daemon.emit("login/changed", Value::Null);
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
                    let mut client = daemon.client.lock().await;
                    let login = client.login(delegated).await.map(|_| ());
                    let state = client.session_state().state;
                    drop(client);
                    // The login may have selected another account.
                    *daemon.last_connect.lock().expect("connect report") = None;
                    // No method call carries it, so the windows hear of it
                    // here, as they hear of `session/unlock`; a failed login
                    // has dropped the one before.
                    crate::logs::write("session/changed", &[("state", &state)]);
                    daemon.emit("session/changed", json!({"state": state}));
                    daemon.emit("approval/changed", Value::Null);
                    login
                }
            },
            Err(error) => Err(error),
        }
    };
    // The person acted in the browser, where no client reports activity.
    if outcome.is_ok() {
        daemon.activity.store(now_ms(), Ordering::Relaxed);
    }
    {
        let mut login = daemon.login.lock().expect("login state");
        login.checking = false;
        login.outcome = Some(outcome.clone());
    }
    daemon.emit("login/changed", Value::Null);
    match outcome {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(Error::Forbidden) => (StatusCode::FORBIDDEN, "refused").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::Remote;

    #[test]
    fn routes_methods_and_reads_announce_nothing() {
        assert_eq!(route("vault/list"), Some(Module::Daemon));
        assert_eq!(route("channel/list"), Some(Module::More));
        assert_eq!(route("mcp/call"), Some(Module::Agent));
        assert_eq!(route("no/such"), None);
        let modules = [
            &METHODS[..],
            &crate::methods::METHODS,
            &crate::account_methods::METHODS,
            &crate::legacy_methods::METHODS,
            &crate::agents::METHODS,
            &crate::agents::AGENT_METHODS,
            &crate::mcp_events::METHODS,
            &crate::files_methods::METHODS,
            &crate::mount::METHODS,
            &crate::wallet_methods::METHODS,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for method in modules.into_iter().flatten() {
            assert!(seen.insert(method), "{method} is listed twice");
        }
        // Windows reload on these events: a read announcing one would loop.
        for read in [
            "vault/list",
            "vault/show",
            "vault/conflicts",
            "files/list",
            "channel/list",
        ] {
            assert!(events_of(route(read).unwrap(), read).is_empty(), "{read}");
        }
        assert_eq!(events_of(Module::Daemon, "vault/put"), ["vault/changed"]);
        // The windows show whether fast unlock is on from the session status.
        assert_eq!(
            events_of(Module::Daemon, "session/fast_unlock"),
            ["session/changed"]
        );
    }

    #[tokio::test]
    async fn takes_large_requests_and_refuses_other_credentials() {
        let dir = dmsg_client::testing::TempDir::new();
        let home = Home {
            dir: dir.0.join("home"),
        };
        let client = dmsg_client::testing::unlocked(&dir.0.join("workspace"), false, None);
        let daemon = Arc::new(Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            client,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        ));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app(daemon)).await });
        std::fs::create_dir_all(&home.dir).unwrap();
        home.publish(&Discovery {
            port,
            pid: 0,
            instance: "test".into(),
        })
        .unwrap();
        // An agent's 8 MiB file write is about 11 MiB as base64 in JSON.
        let owner = Remote::agent(&home, "owner-token").unwrap();
        let padding = "a".repeat(11 * 1024 * 1024);
        assert_eq!(
            owner
                .call("app/activity", json!({"padding": padding}))
                .await
                .unwrap(),
            Value::Null
        );
        // A larger one is refused as such, not as a missing credential.
        let padding = "a".repeat(MAX_REQUEST);
        let refused = owner
            .call("app/activity", json!({"padding": padding}))
            .await
            .unwrap_err();
        assert_ne!(refused.code, "AUTH_REQUIRED", "{refused:?}");
        let stranger = Remote::agent(&home, "not-the-owner").unwrap();
        assert_eq!(
            stranger
                .call("app/activity", Value::Null)
                .await
                .unwrap_err()
                .code,
            "AUTH_REQUIRED"
        );
    }

    #[tokio::test]
    async fn login_result_answers_while_the_callback_checks() {
        let dir = dmsg_client::testing::TempDir::new();
        let home = Home {
            dir: dir.0.join("home"),
        };
        let client = dmsg_client::testing::unlocked(&dir.0.join("workspace"), false, None);
        let daemon = Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            client,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        );
        // The callback holds the client while it checks a login.
        daemon.login.lock().expect("login state").checking = true;
        let held = daemon.client.lock().await;
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            daemon.call_daemon("session/login_result", &Value::Null),
        )
        .await
        .expect("an answer while the check holds the client")
        .unwrap();
        assert_eq!(
            (&result["pending"], &result["checking"], &result["status"]),
            (&json!(true), &json!(true), &Value::Null)
        );
        drop(held);
        daemon.login.lock().expect("login state").checking = false;
        let result = daemon
            .call_daemon("session/login_result", &Value::Null)
            .await
            .unwrap();
        assert_eq!(
            (
                &result["pending"],
                &result["done"],
                &result["status"]["state"]
            ),
            (&json!(false), &json!(false), &json!("unlocked"))
        );
    }

    fn request(method: &str, params: Value) -> RpcRequest {
        RpcRequest {
            jsonrpc: "2.0".into(),
            id: 1.into(),
            method: method.into(),
            params,
        }
    }

    #[test]
    fn activity_of_requests() {
        assert!(is_activity(&request("vault/list", Value::Null), false));
        assert!(!is_activity(&request("daemon/status", Value::Null), false));
        let waiting = request("mcp/call", json!({"tool": "dmsg_events_next"}));
        assert!(!is_activity(&waiting, false));
        // A client that reports its person's activity: only the reports count.
        assert!(!is_activity(&request("vault/list", Value::Null), true));
        assert!(is_activity(&request("app/activity", Value::Null), true));
        assert!(is_activity(&request("app/activity", Value::Null), false));
    }
}
