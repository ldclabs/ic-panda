//! `dmsg daemon`: the only process that holds keys and state.
//!
//! It listens on a loopback port. `/daemon/status`, `/app/v1` (JSON-RPC 2.0
//! over HTTP POST) and `/ws/app/v1` (the same methods over a WebSocket, with
//! events) need the owner credential and refuse any request with an
//! `Origin`, so web pages cannot even learn whether it runs. The login
//! callback is open only while a login is pending, accepts one POST from its
//! own loopback origin and then closes (`docs/protocol/login-v1_zh.md` §4).
//!
//! Idle for `idle_lock_minutes`, the daemon locks (agents' calls are not
//! the person's activity and never hold it off); versions already signed
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
    approval::{Ask, Decision},
    attest::{Bundle, FileSource, StatementInput},
    client::Warm,
    error_code,
    hold::{Hold, Unlocked},
    library::{Check, VersionCheck, blobs::Blobs},
    login::{DEFAULT_TTL_MS, LoginSession},
    now_ms,
    principal::{self as profile, PrincipalDocument},
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
/// Retry transient account-sync failures even when periodic sync is off.
const SYNC_RETRY_MS: u64 = 60_000;
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
const METHODS: [&str; 56] = [
    "session/status",
    "session/login",
    "session/login_result",
    "session/accounts",
    "session/switch",
    "session/unlock",
    "session/lock",
    "session/logout",
    "session/fast_unlock",
    "session/device_bound",
    "account/create",
    "account/show",
    "device/pair_request",
    "device/approve",
    "sync/run",
    "sync/status",
    "profile/show",
    "profile/display",
    "cache/status",
    "cache/clear",
    "profile/pending",
    "profile/publish",
    "profile/retry",
    "profile/resolve",
    "profile/avatar",
    "profile/avatar_image",
    "profile/avatar_legacy",
    "workspace/reset",
    "daemon/stop",
    "approval/list",
    "approval/show",
    "approval/respond",
    "approval/cancel",
    "approval/reconcile",
    "approval/check_file",
    "attest/request",
    "attest/readiness",
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
    Vault,
    Secrets,
    Notifications,
}

/// The module that serves `method`; `None` for an unknown method.
fn route(method: &str) -> Option<Module> {
    let modules: [(Module, &[&str]); 13] = [
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
        (Module::Vault, &crate::vault_methods::METHODS),
        (Module::Secrets, &crate::secret_methods::METHODS),
        (Module::Notifications, &crate::notifications::METHODS),
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
    pub(crate) secret_mounts: crate::secret_mounts::Manager,
    pub(crate) ssh_path: std::sync::Mutex<Option<std::path::PathBuf>>,
    pub(crate) ssh_error: std::sync::Mutex<Option<String>>,
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
    hints_changed: Notify,
    wake: Notify,
    sync_requested: AtomicBool,
    syncing: AtomicBool,
    /// The latest mailbox pass, refusal codes included; nothing secret.
    last_connect: std::sync::Mutex<Option<Value>>,
    /// The Agent Protocols connectors of the hosted Agent IDs.
    agents: dmsg_agent::Runtime,
    /// Counts the connector calls, for agents watching their resources.
    connectors: tokio::sync::watch::Sender<u64>,
    /// MCP Events: the fetch loop's signals and upstream keys.
    mcp: crate::mcp_events::McpEvents,
    mail_push: crate::mail_push::MailPush,
    /// The last outcome each background task logged.
    outcomes: std::sync::Mutex<std::collections::BTreeMap<&'static str, String>>,
    /// The file library's loopback WebDAV mount, while mounted.
    pub(crate) mount: std::sync::Mutex<Option<crate::mount::Mount>>,
    /// Whether the mount is allowed: `config.toml` or `preferences.json`.
    pub(crate) files_mount: AtomicBool,
    /// Stops the file imports under way; set without the client's lock.
    pub(crate) import_cancel: Arc<AtomicBool>,
    /// Channels the relay hinted at, for the channel loop; `*` reads all.
    channel_hints: tokio::sync::mpsc::UnboundedSender<String>,
    /// The loop's end of `channel_hints`, until it takes it.
    channel_hinted: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<String>>>,
    /// What operations waiting on the network without the client take
    /// turns on.
    turns: crate::held::Turns,
    pub(crate) display: crate::display::Display,
    pub(crate) snapshots: crate::snapshots::Snapshots,
    pub(crate) notifications: crate::notifications::Notifications,
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
    // With the system keystore, the Secure Enclave keeps cosigner keys,
    // approval keys and the fast-unlock and device-bound keys, and asks for
    // Touch ID or the login password.
    #[cfg(target_os = "macos")]
    if settings.storage == crate::home::Storage::System {
        let enclave = std::sync::Arc::new(crate::enclave::SecureEnclave);
        client.set_cosign_provider(enclave.clone());
        client.set_approval_keys(enclave.clone());
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
    daemon.secret_mounts.bind(Arc::downgrade(&daemon));
    daemon.read_preferences().await?;
    home.publish(&Discovery {
        port,
        pid: std::process::id(),
        instance: daemon.instance.clone(),
    })?;
    tokio::spawn(background(daemon.clone()));
    tokio::spawn(crate::hints::run(daemon.clone()));
    tokio::spawn(crate::channels::run(daemon.clone()));
    tokio::spawn(crate::display::run(daemon.clone()));
    tokio::spawn(crate::snapshots::run(daemon.clone()));
    tokio::spawn(crate::recovery_watch::run(daemon.clone()));
    tokio::spawn(crate::notifications::run(daemon.clone()));
    tokio::spawn(crate::notifications::inbox_run(daemon.clone()));
    tokio::spawn(crate::notifications::services_run(daemon.clone()));
    tokio::spawn(crate::notifications::receive_run(daemon.clone()));
    tokio::spawn(crate::mcp_events::run(daemon.clone()));
    tokio::spawn(crate::agents::run_mail(daemon.clone()));
    tokio::spawn(crate::mail_push::run(daemon.clone()));
    #[cfg(unix)]
    let _ssh = match crate::ssh_agent::start(daemon.clone(), &home.dir) {
        Ok(socket) => Some(socket),
        Err(error) => {
            *daemon.ssh_error.lock().expect("SSH status") = Some(match error {
                Error::Unavailable(reason) => reason,
                _ => "SSH_SOCKET_DIRECTORY_UNSAFE".into(),
            });
            None
        }
    };
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
            stopping.stop_secret_mounts().await;
            let _ = stopping.unmount_library().await;
        })
        .await;
    daemon.client.lock().await.shutdown();
    home.unpublish();
    crate::logs::write("daemon/stop", &[]);
    served.map_err(|e| Error::Unavailable(format!("serve: {e}")))
}

/// The loopback routes.
pub(crate) fn app(daemon: Arc<Daemon>) -> Router {
    Router::new()
        .route("/daemon/status", get(status))
        .route(
            "/app/v1",
            post(rpc).layer(DefaultBodyLimit::max(MAX_REQUEST)),
        )
        .route("/ws/app/v1", get(socket))
        .route("/auth/callback", get(callback_page).post(callback))
        .route("/dav/{token}/{volume}", any(crate::mount::serve))
        .route("/dav/{token}/{volume}/", any(crate::mount::serve))
        .route("/dav/{token}/{volume}/{*rest}", any(crate::mount::serve))
        .with_state(daemon)
}

/// A failed pass stays in its account's workspace. Only transient failures
/// retry automatically, after a pause and while content is unlocked.
fn sync_retry_due(client: &Client, now: u64) -> bool {
    client.session_state().state == "unlocked"
        && client.last_sync().ok().flatten().is_some_and(|last| {
            matches!(
                last.error.as_deref(),
                Some("UNAVAILABLE" | "NETWORK_UNAVAILABLE" | "EXECUTION_UNKNOWN")
            ) && now >= last.at.saturating_add(SYNC_RETRY_MS)
        })
}

/// Idle lock, the keyless flush of presigned submissions and the
/// scheduled sync, including retries. Background work is not activity.
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
        let (session, requested, retry) = {
            let mut client = daemon.client.lock().await;
            let before = client.session_state().state;
            if now > daemon.activity.load(Ordering::Relaxed) + daemon.settings.idle_lock_ms {
                client.lock_idle();
            }
            let session = client.session_state();
            if session.state != before {
                crate::logs::write("session/changed", &[("state", &session.state)]);
                daemon.emit("session/changed", json!({"state": session.state}));
            }
            (
                session,
                daemon.sync_requested.swap(false, Ordering::Relaxed),
                sync_retry_due(&client, now),
            )
        };
        daemon.check_mount().await;
        daemon.check_secret_mounts().await;
        let interval = daemon.settings.sync_interval_ms;
        let unlocked = session.state == "unlocked";
        let ready = unlocked || session.resident;
        // The relay is waited on without holding the client where the work
        // allows it, so that the windows' reads go on meanwhile.
        if ready
            && ((requested && unlocked)
                || (retry && !daemon.syncing.load(Ordering::Relaxed))
                || (interval > 0 && now >= synced + interval))
        {
            synced = now;
            // Recorded in the status; transient failures retry after a minute.
            let synced = daemon.sync_pass().await;
            daemon.note_result("sync", &synced);
        } else {
            let flushed = daemon.flush_pass().await;
            daemon.note_result("flush", &flushed);
            if let Ok(settled @ 1..) = flushed {
                crate::logs::write("flush", &[("settled", &settled)]);
            }
        }
        if ready && now >= polled + CONNECT_POLL_MS {
            polled = now;
            let report = daemon.connect_pass().await;
            daemon.note_result("connect", &report);
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
                let reconciled = daemon.reconcile_pass().await.unwrap_or(0);
                let count = daemon
                    .client
                    .lock()
                    .await
                    .approvals_waiting()
                    .unwrap_or(waiting);
                if reconciled > 0 || count != waiting {
                    daemon.emit("approval/changed", Value::Null);
                }
                waiting = count;
            }
        }
        // Backups a person asked for keep moving while the device key is
        // there, locked or not, a slice per pass; nothing here starts one.
        // A pass under way, such as the person's own, runs on without this
        // loop, which does not wait for it.
        let backing_up = daemon
            .client
            .lock()
            .await
            .library_jobs_running()
            .unwrap_or(false);
        if ready
            && backing_up
            && let Some(turn) = daemon.try_turn("backup")
        {
            let run = daemon.backup_pass(turn, Some(BACKUP_SLICE_MS)).await;
            daemon.note_result("backup", &run);
            if run.as_ref().is_ok_and(|r| r.completed > 0) {
                daemon.emit("files/changed", Value::Null);
            }
            if run.as_ref().is_ok_and(|r| r.paused) {
                pause = 0;
            }
        }
        // The library's self-check, once a day while nobody is at the
        // device.
        let quiet = now > daemon.activity.load(Ordering::Relaxed) + CHECK_IDLE_MS;
        let check = {
            let mut client = daemon.client.lock().await;
            if unlocked {
                // The file library's housekeeping: expired trash and
                // unreferenced chunks every ten minutes, opened copies every
                // pass.
                let maintained = client.files_maintain(now);
                daemon.note_result("files", &maintained);
                if let Ok(Some(report)) = &maintained
                    && report.purged_files + report.purged_folders > 0
                {
                    daemon.emit("files/changed", json!({"maintenance": report}));
                }
            }
            quiet
                && now >= checking + CHECK_RETRY_MS
                && client.files_check_due(now).unwrap_or(false)
        };
        if unlocked && let Ok(changed) = dmsg_client::library::open::sweep(&daemon.hold()).await {
            for copy in changed {
                daemon.emit("files/open_changed", to_json(&copy));
            }
        }
        if check {
            checking = now;
            let checked = daemon.check_library().await;
            daemon.note_result("check", &checked);
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
    /// Announcements for local transports waiting on a person's decision.
    #[cfg(unix)]
    pub(crate) fn subscribe_events(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    pub(crate) fn new(
        token: String,
        port: u16,
        settings: Settings,
        mut client: Client,
        agents: dmsg_agent::Runtime,
        mcp: crate::mcp_events::McpEvents,
    ) -> Self {
        crate::secret_mounts::configure_clock(&mut client);
        let settings_mount = settings.files_mount;
        let (channel_hints, channel_hinted) = tokio::sync::mpsc::unbounded_channel();
        Self {
            token,
            instance: hex::encode(&dmsg_client::crypto::random_id()[..8]),
            port,
            settings,
            ssh_path: std::sync::Mutex::new(None),
            ssh_error: std::sync::Mutex::new(None),
            secret_mounts: Default::default(),
            client: Arc::new(Mutex::new(client)),
            login: std::sync::Mutex::new(LoginState::default()),
            activity: AtomicU64::new(now_ms()),
            stopping: AtomicBool::new(false),
            stop: Notify::new(),
            events: broadcast::channel(256).0,
            seq: AtomicU64::new(0),
            hints: std::sync::Mutex::new(HintState::default()),
            hints_changed: Notify::new(),
            wake: Notify::new(),
            sync_requested: AtomicBool::new(false),
            syncing: AtomicBool::new(false),
            last_connect: std::sync::Mutex::new(None),
            agents,
            connectors: tokio::sync::watch::channel(0).0,
            mcp,
            mail_push: Default::default(),
            outcomes: Default::default(),
            mount: std::sync::Mutex::new(None),
            files_mount: AtomicBool::new(settings_mount),
            import_cancel: Arc::new(AtomicBool::new(false)),
            channel_hints,
            channel_hinted: std::sync::Mutex::new(Some(channel_hinted)),
            turns: Default::default(),
            display: Default::default(),
            snapshots: Default::default(),
            notifications: Default::default(),
        }
    }

    /// Meaningful local use, including file reads/writes outside the app,
    /// moves the idle-lock deadline. DAV metadata polling does not.
    pub(crate) fn note_activity(&self) {
        self.activity.store(now_ms(), Ordering::Relaxed);
    }

    /// Have the channel loop read `channel`, or every channel for `*`.
    pub(crate) fn hint_channel(&self, channel: &str) {
        let _ = self.channel_hints.send(channel.to_string());
    }

    /// The channel loop's end of [`Self::hint_channel`], once.
    pub(crate) fn take_channel_hints(
        &self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<String>> {
        self.channel_hinted.lock().expect("channel hints").take()
    }

    /// Check the whole library: every chunk file against its name beside
    /// the client, which needs no key, then every local version through
    /// its key while unlocked; the check is recorded.
    pub(crate) async fn check_library(&self) -> Result<(Check, Vec<VersionCheck>)> {
        let h = self.hold();
        let dir = h.with(|c| c.files_blobs_dir()).await?;
        let (count, bad) = tokio::task::spawn_blocking(move || Blobs::new(dir).verify())
            .await
            .map_err(|e| Error::Unavailable(format!("check: {e}")))??;
        dmsg_client::library::reading::verify_library(&h, count, bad).await
    }

    /// Publish an event to every WebSocket client.
    pub(crate) fn emit(&self, kind: &str, data: Value) {
        if kind == "session/changed" {
            self.display.clear();
            self.snapshots.clear();
            self.hints_changed.notify_one();
            self.notifications.inbox.notify_one();
            self.notifications.receive.notify_one();
        }
        if matches!(
            kind,
            "session/changed"
                | "approval/changed"
                | "channel/changed"
                | "channel/read"
                | "inbox/changed"
                | "recovery/changed"
                | "agent/mail"
                | "wallet/changed"
                | "cache/changed"
                | "sync/changed"
        ) {
            self.notifications.wake.notify_one();
        }
        // A mute or hide uploads its preference now. Ordinary channel reads do
        // not poll the remote feed; its own hints wake that.
        if kind == "channel/changed" && data["method"] == "channel/settings" {
            self.notifications.receive.notify_one();
        }
        if matches!(
            kind,
            "agent/changed" | "approval/changed" | "session/changed"
        ) {
            self.mail_push.maintain.notify_one();
        }
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self
            .events
            .send(json!({"seq": seq, "type": kind, "data": data}));
    }

    pub(crate) fn session_token(&self, generation: u64) -> String {
        format!("{}:{generation}", self.instance)
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

    /// Like `note`, retaining only a fixed cause category for failures.
    pub(crate) fn note_result<T>(&self, task: &'static str, result: &Result<T>) {
        let diagnostic = result
            .as_ref()
            .err()
            .map(dmsg_client::diagnostic::Diagnostic::error);
        let signature = format!("{} {diagnostic:?}", outcome(result));
        let mut outcomes = self.outcomes.lock().expect("outcomes");
        if outcomes.get(task) != Some(&signature) {
            outcomes.insert(task, signature);
            crate::logs::result(task, &[], result);
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

    /// [`dmsg_client::sync::flush`], each submission sent without holding
    /// the client.
    async fn flush_pass(&self) -> Result<usize> {
        let Some(_turn) = self.try_turn("sync") else {
            return Ok(0);
        };
        dmsg_client::sync::flush(&self.hold()).await
    }

    /// The automatic unlock sync starts after local content can be read,
    /// even when periodic sync is disabled.
    fn request_sync(&self) {
        self.sync_requested.store(true, Ordering::Relaxed);
        self.wake();
    }

    /// Serialize content submissions, but leave the client available to
    /// local reads, edits and locking throughout every network wait.
    async fn sync_pass(&self) -> Result<dmsg_client::sync::SyncReport> {
        let h = self.hold();
        // Bind the account before waiting for another pass to finish.
        let before = h.with(|c| Ok(c.session_state())).await?;
        let _turn = self.turn("sync").await;
        self.syncing.store(true, Ordering::Relaxed);
        self.emit("sync/progress", Value::Null);
        let started = std::time::Instant::now();
        let started_at = now_ms();
        let result = dmsg_client::sync::sync(&h).await;
        let stage = h
            .with(|c| c.last_sync())
            .await
            .ok()
            .flatten()
            .filter(|last| last.at >= started_at)
            .and_then(|last| last.stage)
            .map(|stage| stage.as_str())
            .unwrap_or(if result.is_ok() {
                "complete"
            } else {
                "unrecorded"
            });
        self.syncing.store(false, Ordering::Relaxed);
        crate::logs::result(
            "sync/pass",
            &[("stage", &stage), ("ms", &started.elapsed().as_millis())],
            &result,
        );
        // Partial progress and failures also change the local list/status.
        self.emit("sync/changed", Value::Null);
        let after = self.client.lock().await.session_state();
        if before.state != after.state || before.account != after.account {
            self.emit("session/changed", Value::Null);
        }
        result
    }

    /// Read the dmsg-connect mailbox without holding the client, then handle
    /// its entries ([`Client::connect_entries`]), which reply and verify
    /// under it while there are any.
    pub(crate) async fn connect_pass(&self) -> Result<dmsg_client::connect::ConnectReport> {
        let fetched = dmsg_client::connect::fetch_mailbox(&self.hold()).await?;
        self.under_lock(async |client| client.connect_entries(fetched).await)
            .await
    }

    pub(crate) fn turns(&self) -> &crate::held::Turns {
        &self.turns
    }

    /// One pass over the backup jobs, in the turn the caller took: each
    /// chunk waits on the relay without the client; with `budget_ms`, a
    /// slice of it.
    pub(crate) async fn backup_pass(
        &self,
        _turn: tokio::sync::OwnedMutexGuard<()>,
        budget_ms: Option<u64>,
    ) -> Result<dmsg_client::library::backup::BackupRun> {
        let mut progress = |file: &str, done, total| {
            self.emit(
                "files/progress",
                json!({"job": "backup", "path": file, "done": done, "total": total}),
            );
        };
        dmsg_client::library::backup::backup_run(&self.hold(), budget_ms, &mut progress).await
    }

    /// A person's decision, from the approval window or an interactive
    /// terminal (`via`). The presence check of a high-risk approval, or the
    /// answer through this device's approval key, and an approved
    /// attestation's network steps wait outside the client, so the other
    /// windows' reads go on meanwhile; any other approved request carries
    /// itself out under the client, the key's answer signing it there.
    async fn respond(&self, id: &str, digest: &str, approve: bool, via: &str) -> Result<Value> {
        let mut confirmed = false;
        let mut authorized = None;
        if approve {
            let ask = self.client.lock().await.approval_presence(id)?;
            if let Some((ask, what)) = ask {
                // The system's prompt waits for the person, off the
                // runtime's threads.
                let answer = tokio::task::spawn_blocking(move || match ask {
                    Ask::Presence(key) => key.confirm(&what).map(|()| None),
                    Ask::Hardware(keys, biometry_only) => {
                        keys.authorize(&what, biometry_only).map(Some)
                    }
                })
                .await
                .map_err(|e| Error::Unavailable(format!("presence: {e}")))?;
                match answer {
                    Ok(answer) => authorized = answer,
                    Err(error) => {
                        self.client.lock().await.approval_unconfirmed(id, &error)?;
                        return Err(error);
                    }
                }
                confirmed = true;
            }
        }
        let decision = self
            .under_lock(async |client| {
                client
                    .approval_decide_with(id, digest, approve, via, confirmed, authorized)
                    .await
            })
            .await?;
        match decision {
            Decision::Done(view) => Ok(view),
            Decision::SecretDiscoveryImport(work) => {
                self.emit("approval/changed", Value::Null);
                let result = self.import_discovered(*work).await;
                self.emit("vault/changed", Value::Null);
                result
            }
            Decision::SecretPackageImport(run) => {
                self.emit("approval/changed", Value::Null);
                let result = run.run(&self.hold()).await;
                self.emit("vault/changed", Value::Null);
                self.emit("files/changed", Value::Null);
                result
            }
            Decision::SecretShare(run) => {
                let _turn = self.turn("secret-share-upload").await;
                self.emit("approval/changed", Value::Null);
                run.run(&self.hold()).await
            }
            Decision::SecretUse(run) => {
                self.emit("approval/changed", Value::Null);
                run.run(&self.hold()).await
            }
            Decision::Wallet(run) => {
                let _turn = self.turn("wallet-approval").await;
                self.emit("approval/changed", Value::Null);
                let result = run.run(&self.hold()).await;
                self.emit("wallet/changed", Value::Null);
                result
            }
            Decision::Attest(run) => {
                // The windows see it executing while it runs.
                self.emit("approval/changed", Value::Null);
                run.run_with_file_reader(&self.hold(), |path| {
                    blocking(move || dmsg_client::attest::file_sha256(&path))
                })
                .await
            }
        }
    }

    /// Settle the approved requests whose outcome is unknown, one pass at a
    /// time: attestations wait on their user home without the client; the
    /// other requests carry themselves out under it. Returns how many there
    /// were.
    async fn reconcile_pass(&self) -> Result<usize> {
        let _turn = self.turn("reconcile").await;
        let attested = dmsg_client::approval::reconcile_attestations(&self.hold()).await?;
        let wallets = {
            let _turn = self.turn("wallet-approval").await;
            dmsg_client::approval::reconcile_wallets(&self.hold()).await?
        };
        let others = self
            .under_lock(async |client| client.approvals_reconcile_others().await)
            .await?;
        Ok(attested + wallets + others)
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

    pub(crate) fn mail_push(&self) -> &crate::mail_push::MailPush {
        &self.mail_push
    }

    pub(crate) fn hints_changed(&self) -> &Notify {
        &self.hints_changed
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
        let client = self.client.lock().await;
        let status = client.status()?;
        let recovery = client.recovery_warning()?;
        let session = format!("{}:{}", self.instance, client.session_generation());
        drop(client);
        let mut value = serde_json::to_value(status).expect("status serializes");
        value["recovery"] = recovery;
        value["session"] = session.into();
        value["version"] = env!("CARGO_PKG_VERSION").into();
        value["instance"] = self.instance.clone().into();
        value["port"] = self.port.into();
        value["syncing"] = (self.syncing.load(Ordering::Relaxed)
            || self.sync_requested.load(Ordering::Relaxed))
        .into();
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
        if crate::snapshots::supports(method) && params.get("mode").is_some() {
            return self.snapshot(method, params).await;
        }
        let plan_refresh = method == "plan/status" && params["refresh"].as_bool().unwrap_or(true);
        let cache_session = if !crate::snapshots::invalidates(method).is_empty() || plan_refresh {
            self.hold()
                .with(|c| {
                    if c.display_session().is_err() {
                        return Ok(None);
                    }
                    if plan_refresh {
                        c.cache_invalidate(&[dmsg_client::cache::Kind::Plan])?;
                    }
                    Ok(Some(c.display_session()?))
                })
                .await?
        } else {
            None
        };
        let result = match module {
            Module::Daemon => self.call_daemon(method, params).await,
            Module::More => self.call_more(method, params).await,
            Module::Account => self.call_account(method, params).await,
            Module::Legacy => self.call_legacy(method, params).await,
            Module::Agents => self.call_agents(method, params).await,
            Module::McpEvents => self.call_mcp_events(method, params).await,
            Module::Files => self.call_files(method, params).await,
            Module::Mount => self.call_mount(method, params).await,
            Module::Wallet => self.call_wallet(method, params).await,
            Module::Vault => self.call_vault(method, params).await,
            Module::Secrets => self.call_secrets(method, params).await,
            Module::Notifications => self.call_notifications(method, params).await,
            // An agent's credential reaches these, never the owner's.
            Module::Agent => Err(Error::Forbidden),
        };
        if let (Ok(data), Some(session)) = (&result, cache_session) {
            let saved = self
                .hold()
                .with(|c| {
                    c.check_display_session(session)?;
                    if plan_refresh {
                        c.cache_invalidate(&[dmsg_client::cache::Kind::Plan])?;
                        c.cache_save(dmsg_client::cache::Kind::Plan, "", data, now_ms())?;
                        self.emit("cache/changed", json!({"kind":"plan"}));
                    } else if !crate::snapshots::invalidates(method).is_empty() {
                        c.cache_invalidate(crate::snapshots::invalidates(method))?;
                        // Profile and name refreshes have no invalidated kind.
                        self.snapshots.clear();
                        for kind in crate::snapshots::invalidates(method) {
                            self.emit("cache/changed", json!({"kind":kind.name()}));
                        }
                    }
                    Ok(())
                })
                .await;
            if let Err(error) = saved {
                dmsg_client::diagnostic::Diagnostic::error(&error).emit();
            }
        }
        result
    }

    /// Run one of [`METHODS`].
    async fn call_daemon(&self, method: &str, params: &Value) -> Result<Value> {
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
                self.sync_requested.store(false, Ordering::Relaxed);
                *self.last_connect.lock().expect("connect report") = None;
                Ok(to_json(&status))
            }
            "session/unlock" => {
                let selected = optional(params, "account", account)?;
                let fast = params["fast"].as_bool().unwrap_or(false);
                let queued = self.client.lock().await.session_generation();
                let _turn = self.turn("session/unlock").await;
                // Starting an unlock is activity too: an expired idle deadline
                // must not invalidate its off-lock system presence prompt.
                self.activity.store(now_ms(), Ordering::Relaxed);
                // Preparation can switch/lock the workspace before failing.
                // Shared emits the session change even when no unlock follows,
                // so owner windows and display schedulers discard old content.
                let (ready, prepared, selected_generation) = self
                    .hold()
                    .with(|client| {
                        let session = client.session_state();
                        if session.state == "unlocked"
                            && selected.as_ref().is_none_or(|account| {
                                session.account.as_deref() == Some(account.to_string().as_str())
                            })
                        {
                            Ok((Some(client.status()?), None, client.session_generation()))
                        } else {
                            if client.session_generation() != queued {
                                return Err(Error::Locked);
                            }
                            if let Some(account) = &selected {
                                client.switch_account(account)?;
                                *self.last_connect.lock().expect("connect report") = None;
                            }
                            let prepared = if fast {
                                Some(client.prepare_fast_unlock()?)
                            } else {
                                None
                            };
                            Ok((None, prepared, client.session_generation()))
                        }
                    })
                    .await?;
                let (status, opened) = if let Some(status) = ready {
                    (status, false)
                } else if let Some(prepared) = prepared {
                    // The OS prompt waits outside Client ownership. Lock and
                    // account switch can invalidate it while the person decides.
                    let (prepared, key) = blocking(move || {
                        let key = prepared.open();
                        Ok((prepared, key))
                    })
                    .await?;
                    let status = self
                        .under_lock(async |client| client.complete_fast_unlock(prepared, key).await)
                        .await?;
                    (status, true)
                } else {
                    (
                        self.under_lock(async |client| {
                            if client.session_generation() != selected_generation {
                                return Err(Error::Locked);
                            }
                            client.unlock().await
                        })
                        .await?,
                        true,
                    )
                };
                // Unlocking is the person's activity: a window that asks for
                // Touch ID as it comes to the front has no input to report.
                self.activity.store(now_ms(), Ordering::Relaxed);
                if opened && status.state == "unlocked" {
                    self.request_sync();
                }
                Ok(to_json(&status))
            }
            "session/lock" => {
                self.client.lock().await.lock();
                self.sync_requested.store(false, Ordering::Relaxed);
                // Versions already signed for submission still go out.
                self.wake();
                self.stop_secret_mounts().await;
                let _ = self.unmount_library().await;
                self.status().await
            }
            "session/logout" => {
                {
                    let mut client = self.client.lock().await;
                    let mut login = self.login.lock().expect("login state");
                    // A callback already accepted must finish before signing
                    // out; a browser page still waiting loses its session.
                    if login.checking {
                        return Err(Error::Pending);
                    }
                    *login = LoginState::default();
                    client.logout()?;
                }
                *self.last_connect.lock().expect("connect report") = None;
                self.stop_secret_mounts().await;
                let _ = self.unmount_library().await;
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
            // Binding rewraps the local data key under the client: it reads
            // the unlock secret and asks for the person once, so nothing
            // else unlocks or locks meanwhile.
            "session/device_bound" => match params["enable"].as_bool() {
                Some(true) => {
                    let biometry_only = params["biometry_only"].as_bool().unwrap_or(false);
                    self.under_lock(async |client| client.bind_device(biometry_only).await)
                        .await
                }
                Some(false) => Ok(to_json(
                    &self
                        .under_lock(async |client| client.unbind_device().await)
                        .await?,
                )),
                None => Err(Error::InvalidInput("missing enable".into())),
            },
            // Account and device changes still wait
            // on the IC and the relay under the client.
            "account/create" => {
                self.under_lock(async |client| {
                    // Account and root journals must settle in order, so setup
                    // stays under the account-operation lock while on the network.
                    let (account, generation) = client.initialize_account().await?;
                    Ok(json!({"account": account.id.to_string(), "root_generation": generation}))
                })
                .await
            }
            "account/show" => {
                let state = self
                    .hold()
                    .warm(Warm::Home)
                    .await?
                    .ok_or(Error::PolicyStale)?;
                let view = self.client.lock().await.account_view_of(&state)?;
                Ok(to_json(&view))
            }
            "device/pair_request" => {
                let role = match params["role"].as_str() {
                    Some("member") => ControllerRole::Member,
                    _ => ControllerRole::Administrator,
                };
                self.under_lock(async |client| {
                    let token = client.pair_request(role).await?;
                    let qr = dmsg_client::qr::png_data(&token).ok();
                    Ok(json!({"token": token, "device_id": client.status()?.device_id, "qr": qr}))
                })
                .await
            }
            "device/approve" => {
                // The owner names the device shown on the new device's
                // screen; the approval center decides the enrollment.
                let (token, device) = (text(params, "token")?, text(params, "device")?);
                dmsg_client::approval::add_device_request(
                    &self.hold(),
                    source(params)?,
                    &token,
                    &device,
                )
                .await
            }
            "sync/run" => {
                let report = self.sync_pass().await?;
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
                    "syncing": self.syncing.load(Ordering::Relaxed) || self.sync_requested.load(Ordering::Relaxed),
                }))
            }
            "cache/status" => self.hold().with(|c| c.cache_status()).await,
            "cache/clear" => {
                self.hold().with(|c| c.cache_clear()).await?;
                self.display.clear();
                self.snapshots.clear();
                self.emit("profile/changed", Value::Null);
                self.emit("names/changed", Value::Null);
                self.emit("cache/changed", json!({"kind":"all"}));
                Ok(Value::Null)
            }
            "profile/display" => self.profile_display().await,
            "profile/show" => Ok(to_json(&profile::show(&self.hold()).await?)),
            "profile/pending" => Ok(json!(self.hold().with(|c| c.document_pending()).await?)),
            "profile/publish" => {
                let document: PrincipalDocument = serde_json::from_value(params.clone())
                    .map_err(|e| Error::InvalidInput(format!("profile: {e}")))?;
                let _turn = self.turn("profile").await;
                // The account's mutation journal and approval sequence must settle
                // together; use the existing serialized account-mutation path.
                let info = self
                    .under_lock(async |client| client.publish_document(&document).await)
                    .await?;
                if info.state.version == info.published_version {
                    self.emit("profile/changed", Value::Null);
                }
                Ok(
                    json!({"version": info.state.version, "published_version": info.published_version, "published": info.state.version == info.published_version}),
                )
            }
            "profile/retry" => {
                let _turn = self.turn("profile").await;
                // Settles the same account mutation journal before republishing.
                let info = self
                    .under_lock(async |client| client.retry_document().await)
                    .await?;
                if info.state.version == info.published_version {
                    self.emit("profile/changed", Value::Null);
                }
                Ok(
                    json!({"version": info.state.version, "published_version": info.published_version, "published": info.state.version == info.published_version}),
                )
            }
            "profile/resolve" => {
                let account = account(params, "account")?;
                Ok(to_json(
                    &profile::resolve_account(&self.hold(), &account).await?,
                ))
            }
            "profile/avatar" => {
                let shared = self.hold();
                let h = Unlocked::new(&shared).await?;
                let path = absolute(params, "path")?;
                let image = blocking(move || {
                    let size = std::fs::metadata(&path)
                        .map_err(|e| Error::InvalidInput(format!("path: {e}")))?
                        .len();
                    if size > dmsg_client::principal::MAX_AVATAR as u64 {
                        return Err(Error::InvalidInput("avatar".into()));
                    }
                    std::fs::read(&path).map_err(|e| Error::InvalidInput(format!("path: {e}")))
                })
                .await?;
                let _turn = self.turn("profile").await;
                Ok(to_json(&profile::upload_avatar(&h, &image).await?))
            }
            "profile/avatar_legacy" => {
                let archive = text(params, "archive")?;
                let source = text(params, "source")?;
                // The archive read and the upload stay on the same account.
                let hold = self.hold();
                let bytes = hold.with(|c| c.legacy_avatar(&archive, &source)).await?;
                let _turn = self.turn("profile").await;
                Ok(to_json(&profile::upload_avatar(&hold, &bytes).await?))
            }
            "profile/avatar_image" => {
                let url = text(params, "avatar_url")?;
                let shared = self.hold();
                let h = Unlocked::new(&shared).await?;
                if let Some(cached) = h.with(|c| c.profile_avatar(&url)).await? {
                    return Ok(crate::display::image(&cached.mime, &cached.bytes));
                }
                let image = profile::avatar(&h, &url).await?;
                h.with(|_| Ok(())).await?;
                // A data URL the desktop shows without fetching anything.
                Ok(image.map_or(Value::Null, |(mime, bytes)| {
                    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                    json!({"mime": mime, "url": format!("data:{mime};base64,{data}")})
                }))
            }
            "workspace/reset" => {
                self.stop_secret_mounts().await;
                self.unmount_library().await?;
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
                if params["ids_only"] == true {
                    return Ok(to_json(
                        &dmsg_client::approval::waiting_ids(&self.hold()).await?,
                    ));
                }
                Ok(Value::Array(
                    dmsg_client::approval::list(&self.hold(), all).await?,
                ))
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
                let digest = text(params, "digest")?;
                let via = source(params)?;
                let outcome = self.respond(&id, &digest, approve, via).await;
                // Who decided, through which way in, on whose request.
                let mut client = self.client.lock().await;
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
                client.audit("owner", via, action, Some(&id), Some(&detail), &code);
                // A failure changes the request too: its state or its error.
                if outcome.is_err() {
                    self.emit("approval/changed", Value::Null);
                }
                outcome
            }
            "approval/cancel" => {
                let mut client = self.client.lock().await;
                let id = client.approval_id(&text(params, "id")?)?;
                client.approval_cancel(&id)
            }
            "approval/reconcile" => {
                self.reconcile_pass().await?;
                Ok(Value::Array(self.client.lock().await.approvals(false)?))
            }
            // A file compared with a request's digest: hashed here, outside
            // the client, since it may be large.
            "approval/check_file" => {
                let id = text(params, "id")?;
                let path = absolute(params, "path")?;
                let shared = self.hold();
                let h = Unlocked::new(&shared).await?;
                let id = h.with(|c| c.approval_id(&id)).await?;
                let (name, size, sha256) = blocking(move || {
                    let (size, _) = dmsg_client::approval::file_stamp(&path)?;
                    Ok((
                        dmsg_client::approval::file_name(&path),
                        size,
                        dmsg_client::attest::file_sha256(&path)?,
                    ))
                })
                .await?;
                h.with(|c| c.approval_file_checked(&id, &name, size, sha256))
                    .await
            }
            "attest/request" => {
                let shared = self.hold();
                let h = Unlocked::new(&shared).await?;
                let input: StatementInput = parse(params, "statement")?;
                let file = optional(params, "file", absolute)?;
                let library = match &params["library"] {
                    Value::Null => None,
                    value => Some((text(value, "file")?, text(value, "version")?)),
                };
                let source_file = match (&file, &library) {
                    // Hashed here, outside the client, since it may be large.
                    (Some(path), None) => {
                        let path = path.clone();
                        Some(blocking(move || dmsg_client::attest::local_file(&path)).await?)
                    }
                    (None, Some((file, version))) => Some(FileSource::Library(file, version)),
                    (None, None) => None,
                    _ => {
                        return Err(Error::InvalidInput(
                            "give a file or a library version".into(),
                        ));
                    }
                };
                h.with(|c| c.attest_request(source(params)?, input, source_file))
                    .await
            }
            "attest/readiness" => {
                dmsg_client::attest::attest_readiness(&self.hold(), params["id"].as_str()).await
            }
            "attest/action" => {
                let action = dmsg_client::codec::unb64(&text(params, "action")?)?;
                dmsg_client::attest::action_request(&self.hold(), source(params)?, &action).await
            }
            "attest/history" => Ok(Value::Array(
                dmsg_client::attest::history(&self.hold()).await?,
            )),
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
                let (ic, homes) = dmsg_client::attest::verification_context(&self.hold()).await?;
                let root = ic.root_key();
                let report = blocking(move || {
                    dmsg_client::attest::verify_bundle(&bundle, file.as_deref(), &root, &homes)
                })
                .await?;
                Ok(to_json(
                    &dmsg_client::attest::verify_current(&ic, report).await?,
                ))
            }
            "device/capabilities" => {
                let device = hash(params, "device")?;
                let capabilities = params["capabilities"]
                    .as_array()
                    .ok_or_else(|| Error::InvalidInput("capabilities".into()))?
                    .iter()
                    .map(|c| capability(c.as_str().unwrap_or_default()))
                    .collect::<Result<Vec<_>>>()?;
                let via = source(params)?;
                dmsg_client::approval::capabilities_request(&self.hold(), via, device, capabilities)
                    .await
            }
            "device/formal_approve" => {
                let enable = params["enable"]
                    .as_bool()
                    .ok_or_else(|| Error::InvalidInput("enable".into()))?;
                dmsg_client::approval::formal_approve_request(&self.hold(), source(params)?, enable)
                    .await
            }
            "device/revoke" => {
                let device = hash(params, "device")?;
                dmsg_client::approval::revoke_request(&self.hold(), source(params)?, device).await
            }
            "connect/invite" => dmsg_client::connect::invite(&self.hold()).await,
            "connect/candidates" => {
                dmsg_client::connect::candidates(&self.hold(), &text(params, "invite")?).await
            }
            "connect/grant" => {
                if params["human"] != true {
                    return Err(Error::Forbidden);
                }
                let (invite, pairing) = (text(params, "invite")?, text(params, "pairing")?);
                let shared = self.hold();
                let h = Unlocked::new(&shared).await?;
                let _turn = self.turn(&format!("connect/invite:{invite}")).await;
                dmsg_client::connect::grant(&h, &invite, &pairing).await
            }
            "connect/pairings" => dmsg_client::connect::pairings(&self.hold()).await,
            "connect/revoke" => {
                dmsg_client::connect::revoke(&self.hold(), &text(params, "pairing")?).await
            }
            "connect/sync" => {
                let report = to_json(&self.connect_pass().await?);
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
            "session/logout" => &["session/changed", "approval/changed", "login/changed"],
            "session/switch" | "session/unlock" | "session/lock" | "workspace/reset"
            | "account/create" => &["session/changed", "approval/changed"],
            // sync_pass announces both progress and completion, including errors.
            "sync/run" => &[],
            "approval/respond"
            | "approval/cancel"
            | "approval/reconcile"
            | "approval/check_file"
            | "attest/request"
            | "attest/action"
            | "device/capabilities"
            | "device/formal_approve"
            | "device/approve"
            | "device/revoke"
            | "connect/sync" => &["approval/changed"],
            "connect/grant" | "connect/revoke" => &["connect/changed"],
            "session/fast_unlock" | "session/device_bound" => &["session/changed"],
            _ => &[],
        },
        Module::More => crate::methods::events_of(method),
        Module::Account => crate::account_methods::events_of(method),
        Module::Legacy => crate::legacy_methods::events_of(method),
        Module::Agents | Module::Agent => crate::agents::events_of(method),
        Module::McpEvents => &[],
        Module::Notifications => &[],
        Module::Files => crate::files_methods::events_of(method),
        Module::Mount => crate::mount::events_of(method),
        Module::Wallet => crate::wallet_methods::events_of(method),
        Module::Vault => crate::vault_methods::events_of(method),
        Module::Secrets => match method {
            "secrets/discovery/import_request"
            | "secrets/packages/import_request"
            | "secrets/shares/request"
            | "secrets/shares/revoke"
            | "secrets/shares/reconcile"
            | "secrets/mounts/save_request"
            | "secrets/mounts/start"
            | "secrets/mounts/stop"
            | "secrets/mounts/remove"
            | "secrets/mounts/revoke_reads"
            | "secrets/ssh/binding_request"
            | "secrets/ssh/disable"
            | "secrets/run/request"
            | "secrets/run/claim"
            | "secrets/run/report"
            | "secrets/policies/save_request"
            | "secrets/policies/delete_request"
            | "secrets/policies/disable"
            | "secrets/uses/cancel"
            | "secrets/uses/mark_reviewed" => &["approval/changed", "agent/changed"],
            _ => &[],
        },
    }
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

/// Whether an owner's request moves the idle lock. Waiting readers do not:
/// a stream would otherwise keep the device unlocked forever. A client that
/// `reports` its person's activity moves it only with `app/activity`. An
/// agent's requests never do: an agent that polls is not the person, and a
/// resident Agent ID works while locked.
fn is_activity(request: &RpcRequest, reports: bool) -> bool {
    let method = request.method.as_str();
    if method.starts_with("notifications/") {
        return false;
    }
    if reports {
        return method == "app/activity";
    }
    !matches!(
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
        if matches!(
            method,
            "notifications/next"
                | "notifications/context"
                | "notifications/result"
                | "notifications/migrate"
        ) && !reports
        {
            return reply_failure(id, &Error::Forbidden);
        }
        if !agent_method && is_activity(&request, reports) {
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
    // An unknown method or tool may be arbitrary user input.
    let method = if route(method).is_some() {
        method
    } else {
        "unknown"
    };
    let tool = params["tool"]
        .as_str()
        .filter(|_| matches!(method, "mcp/call" | "ap/call"))
        .filter(|t| dmsg_agent::tools::is_standard(t) || dmsg_agent::tools::is_dmsg(t));
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
        Some(tool) => crate::logs::result(
            "call",
            &[
                ("method", &method),
                ("tool", &tool),
                ("by", &by),
                ("ms", &ms),
            ],
            result,
        ),
        None => crate::logs::result(
            "call",
            &[("method", &method), ("by", &by), ("ms", &ms)],
            result,
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
            // person to approve binding it. Either checks the login on the
            // IC under the client, which `session/login_result` answers
            // beside.
            Ok(delegated) => match bind {
                Some(source) => daemon
                    .under_lock(async |client| {
                        client
                            .bind_login(&source, delegated.identity, delegated.expires_at)
                            .await
                    })
                    .await
                    .map(|approval| {
                        daemon.login.lock().expect("login state").approval = Some(approval);
                        daemon.emit("approval/changed", Value::Null);
                    }),
                None => {
                    let (login, state) = daemon
                        .under_lock(async |client| {
                            let login = client.login(delegated).await.map(|_| ());
                            Ok((login, client.session_state().state))
                        })
                        .await
                        .unwrap_or_else(|error| (Err(error), "logged_out"));
                    // The login may have selected another account.
                    *daemon.last_connect.lock().expect("connect report") = None;
                    // No method call carries it, so the windows hear of it
                    // here, as they hear of `session/unlock`; a failed login
                    // has dropped the one before.
                    crate::logs::write("session/changed", &[("state", &state)]);
                    daemon.emit("session/changed", json!({"state": state}));
                    daemon.emit("approval/changed", Value::Null);
                    if login.is_ok() && state == "unlocked" {
                        daemon.request_sync();
                    }
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

/// CPU-heavy file work belongs on the blocking pool, outside both the
/// shared client and the async executor's worker threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| Error::Unavailable(format!("file worker: {e}")))?
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod display_tests;

#[cfg(test)]
#[path = "recovery_watch_tests.rs"]
mod recovery_watch_tests;

#[cfg(test)]
#[path = "notifications_tests.rs"]
mod notifications_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::Remote;

    #[tokio::test]
    async fn failure_logs_keep_causes_without_private_messages_or_rpc_input() {
        use dmsg_client::diagnostic::{Diagnostic, TARGET};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        const PRIVATE: &str = "private_diagnostic_canary";
        let dir = dmsg_client::testing::TempDir::new();
        let default_panic = std::panic::take_hook();
        crate::logs::open(&dir.0).unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let cloud = dmsg_client::cloud::Cloud::new(
            &format!("http://{}", listener.local_addr().unwrap()),
            true,
        )
        .unwrap();
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 8192];
                assert!(socket.read(&mut request).await.unwrap() > 0);
                let body = json!({"ok": false, "error": {"code": PRIVATE, "message": format!("Bearer {PRIVATE} /Users/{PRIVATE} {PRIVATE}@example.com")}}).to_string();
                socket.write_all(format!("HTTP/1.1 503 Unavailable\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            // Read a mutation and drop its reply: diagnostic logging must
            // not turn an uncertain submission into a definite rejection.
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 8192];
            assert!(socket.read(&mut request).await.unwrap() > 0);
        });
        let error = cloud
            .public(&format!("/public/{PRIVATE}?token={PRIVATE}"))
            .await
            .unwrap_err();
        assert_eq!(
            cloud
                .publish_security(&dmsg_client::cloud::Evidence {
                    schema: 1,
                    canister: PRIVATE.into(),
                    certificate: PRIVATE.into(),
                    entries: vec![],
                })
                .await,
            Err(Error::ExecutionUnknown)
        );
        server.await.unwrap();
        let failed: Result<Value> = Err(error);
        log_call(
            PRIVATE,
            &json!({"token": PRIVATE}),
            false,
            &failed,
            Duration::from_millis(12),
        );
        log_call(
            "mcp/call",
            &json!({"tool": PRIVATE, "input": {"secret": PRIVATE}}),
            true,
            &failed,
            Duration::from_millis(13),
        );
        crate::logs::failure(
            "files/unmount",
            &[],
            &Error::Unavailable(format!("mount: /Users/{PRIVATE}")),
        );
        Diagnostic::io(
            "hints",
            &std::io::Error::new(std::io::ErrorKind::UnexpectedEof, PRIVATE),
        )
        .emit();
        // Neither a dependency debug message nor malformed data on our own
        // target may reach the run log.
        log::error!(target: "reqwest", "Authorization: Bearer {PRIVATE}");
        log::warn!(target: TARGET, "relay http 503 {PRIVATE}");
        assert!(std::panic::catch_unwind(|| panic!("{PRIVATE}")).is_err());
        std::panic::set_hook(default_panic);
        let logged = crate::logs::tail(&dir.0, 10_000).unwrap().join("\n");
        assert!(
            !logged.contains(PRIVATE),
            "private data reached the run log"
        );
        assert!(logged.contains("failure source=relay reason=http http_status=503"));
        assert!(logged.contains("failure source=hints reason=connection_closed"));
        assert!(logged.contains("method=unknown"));
        assert!(logged.contains("source=mount reason=unavailable"));
    }

    #[test]
    fn routes_methods_and_reads_announce_nothing() {
        assert_eq!(route("vault/list"), Some(Module::Vault));
        assert_eq!(route("channel/list"), Some(Module::More));
        assert_eq!(route("channel/delete_legacy_draft"), Some(Module::More));
        assert_eq!(route("legacy/delete_channel"), Some(Module::Legacy));
        assert_eq!(route("profile/pending"), Some(Module::Daemon));
        for retired in [
            "legacy/shared_prepare",
            "legacy/shared_activate",
            "legacy/shared_claim",
            "legacy/shared_receive",
        ] {
            assert_eq!(route(retired), None);
        }
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
            &crate::vault_methods::METHODS,
            &crate::secret_methods::METHODS,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for method in modules.into_iter().flatten() {
            assert!(seen.insert(method), "{method} is listed twice");
        }
        // Windows reload on these events: a read announcing one would loop.
        for read in [
            "profile/pending",
            "vault/list",
            "vault/show",
            "vault/reveal",
            "vault/history",
            "vault/conflicts",
            "vault/otp",
            "vault/health",
            "vault/import_preview",
            "files/list",
            "files/search",
            "files/tags",
            "files/preferences",
            "channel/list",
            "secrets/ref",
            "secrets/policies/list",
            "secrets/uses/get",
            "secrets/uses/list",
        ] {
            assert!(events_of(route(read).unwrap(), read).is_empty(), "{read}");
        }
        assert_eq!(events_of(Module::Vault, "vault/create"), ["vault/changed"]);
        // A tool call announces only its audit entry, sent as it is made:
        // a polling agent has no window read the principal again.
        assert!(events_of(Module::Agent, "mcp/call").is_empty());
        assert!(events_of(Module::Agents, "ap/call").is_empty());
        assert!(events_of(Module::Agents, "ap/read").is_empty());
        assert_eq!(events_of(Module::Agents, "agent/remove"), ["agent/changed"]);
        // The windows show whether fast unlock is on from the session status.
        assert_eq!(
            events_of(Module::Daemon, "session/fast_unlock"),
            ["session/changed"]
        );
    }

    #[tokio::test]
    async fn review_secret_owner_methods_are_inaccessible_with_an_agent_credential() {
        let dir = dmsg_client::testing::TempDir::new();
        let home = Home {
            dir: dir.0.join("home"),
        };
        let mut c = dmsg_client::testing::unlocked(&dir.0.join("workspace"), false, None);
        let actor = c.agent_create("boundary").unwrap();
        let credential = c.agent_credential(&actor.agent_id).unwrap();
        let daemon = Arc::new(Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            c,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        ));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move { axum::serve(listener, app(daemon)).await });
        std::fs::create_dir_all(&home.dir).unwrap();
        home.publish(&Discovery {
            port,
            pid: 0,
            instance: "review".into(),
        })
        .unwrap();
        let agent = Remote::agent(&home, &credential).unwrap();
        for method in crate::secret_methods::METHODS
            .into_iter()
            .chain(["approval/respond", "grants/add"])
        {
            assert_eq!(
                agent
                    .call(method, json!({"human":true,"source":"gui","approve":true}))
                    .await
                    .unwrap_err()
                    .code,
                "FORBIDDEN",
                "{method}"
            );
        }
        let owner = Remote::agent(&home, "owner-token").unwrap();
        assert_eq!(
            owner
                .call("secrets/policies/list", json!({}))
                .await
                .unwrap()["policies"],
            json!([])
        );
        let origin = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/app/v1"))
            .header("authorization", "Bearer owner-token")
            .header("origin", "https://untrusted.example")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"secrets/policies/list","params":{}}))
            .send()
            .await
            .unwrap();
        assert_eq!(origin.status(), StatusCode::UNAUTHORIZED);
        server.abort();
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
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "the test holds the client as a login check does"
    )]
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
        daemon.login.lock().expect("login state").checking = true;
        assert_eq!(
            daemon.call_daemon("session/logout", &Value::Null).await,
            Err(Error::Pending)
        );
        {
            let mut login = daemon.login.lock().expect("login state");
            login.checking = false;
            login.session = Some(
                LoginSession::new(
                    &[candid::Principal::anonymous()],
                    "https://dmsg.net",
                    1234,
                    DEFAULT_TTL_MS,
                    now_ms(),
                )
                .unwrap(),
            );
        }
        assert_eq!(
            daemon
                .call_daemon("session/logout", &Value::Null)
                .await
                .unwrap()["state"],
            "logged_out"
        );
        assert!(daemon.login.lock().expect("login state").session.is_none());
        assert_eq!(
            events_of(Module::Daemon, "session/logout"),
            ["session/changed", "approval/changed", "login/changed"]
        );
    }

    /// A daemon whose relay and user homes take connections and never
    /// answer, with their listeners; its client holds fresh relay state, so
    /// its relay requests sign at once.
    async fn stalled(
        dir: &dmsg_client::testing::TempDir,
    ) -> (
        Arc<Daemon>,
        tokio::net::TcpListener,
        tokio::net::TcpListener,
    ) {
        let home = Home {
            dir: dir.0.join("home"),
        };
        let relay = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let ic = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let mut client = dmsg_client::testing::unlocked(&dir.0.join("workspace"), false, None);
        let origin = |l: &tokio::net::TcpListener| format!("http://{}", l.local_addr().unwrap());
        dmsg_client::testing::relay_to(&mut client, &origin(&relay));
        dmsg_client::testing::ic_to(&mut client, &origin(&ic));
        dmsg_client::testing::warm(&mut client);
        let daemon = Arc::new(Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            client,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        ));
        (daemon, relay, ic)
    }

    /// The library answers while `work` waits on `server`, which took its
    /// connection and never answers.
    async fn answers_beside<T>(
        daemon: &Daemon,
        server: &tokio::net::TcpListener,
        work: tokio::task::JoinHandle<T>,
    ) {
        let (_stalled, _) = server.accept().await.unwrap();
        tokio::time::timeout(
            Duration::from_secs(5),
            daemon.call_files("files/tree", &Value::Null),
        )
        .await
        .expect("an answer while the network is waited on")
        .unwrap();
        work.abort();
    }

    #[tokio::test]
    async fn the_library_answers_while_the_network_is_waited_on() {
        let account = dmsg_client::testing::account().id.to_string();
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, relay, ic) = stalled(&dir).await;
        // A public avatar download.
        let params = json!({"avatar_url": format!("http://{}/public/accounts/{account}/avatar/{}", relay.local_addr().unwrap(), "ab".repeat(32))});
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move { daemon.call_daemon("profile/avatar_image", &params).await }
        });
        answers_beside(&daemon, &relay, work).await;
        // A certified directory read, the document the shell shows on unlock.
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move { daemon.call_daemon("profile/show", &Value::Null).await }
        });
        answers_beside(&daemon, &ic, work).await;
        // The background pass reading the dmsg-connect mailbox.
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move { daemon.connect_pass().await }
        });
        answers_beside(&daemon, &relay, work).await;
        // The user home's certified security, for the account's page.
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move { daemon.call_daemon("account/show", &Value::Null).await }
        });
        answers_beside(&daemon, &ic, work).await;
    }

    #[tokio::test]
    async fn local_reads_edits_and_lock_finish_while_sync_waits_on_the_relay() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, relay, _ic) = stalled(&dir).await;
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move { daemon.call_daemon("sync/run", &Value::Null).await }
        });
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(2), relay.accept())
            .await
            .unwrap()
            .unwrap();
        let mut request = [0; 8192];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        let started = std::time::Instant::now();
        tokio::time::timeout(Duration::from_secs(2), async {
            assert_eq!(daemon.status().await.unwrap()["syncing"], true);
            daemon.call_files("files/tree", &Value::Null).await.unwrap();
            daemon.call_vault("vault/list", &Value::Null).await.unwrap();
            daemon
                .client
                .lock()
                .await
                .vault_create(
                    "note",
                    &dmsg_client::vault::Changes {
                        title: Some("written during sync".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(
                daemon
                    .call_vault("vault/list", &Value::Null)
                    .await
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                daemon
                    .call_daemon("session/lock", &Value::Null)
                    .await
                    .unwrap()["state"],
                "locked"
            );
        })
        .await
        .expect("local work and lock must not wait for the export response");
        eprintln!(
            "local reads, edit and lock during stalled sync: {:?}",
            started.elapsed()
        );
        // The late export response cannot be applied or start another request.
        let body = json!({"through": 0, "account": dmsg_client::testing::account().id.to_string(), "scope": "cloud_snapshot", "counts": []}).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), work)
                .await
                .unwrap()
                .unwrap(),
            Err(Error::Locked)
        );
        let status = daemon.status().await.unwrap();
        assert_eq!(status["syncing"], false);
        assert!(status["last_sync"].is_null());
        assert_eq!(status["pending_sync"], 1);
    }

    #[tokio::test]
    async fn failed_sync_retries_without_periodic_sync_but_stops_when_locked_or_recovered() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tokio::time::timeout(Duration::from_secs(10), async {
            for code in ["UNAVAILABLE", "EXECUTION_UNKNOWN", "INTEGRITY_FAILED"] {
                let dir = dmsg_client::testing::TempDir::new();
                let (daemon, relay, _ic) = stalled(&dir).await;
                assert_eq!(daemon.settings.sync_interval_ms, 0);
                let work = tokio::spawn({
                    let daemon = daemon.clone();
                    async move { daemon.sync_pass().await }
                });
                let (mut socket, _) = relay.accept().await.unwrap();
                let mut request = [0; 8192];
                assert!(socket.read(&mut request).await.unwrap() > 0);
                let body = json!({"ok": false, "error": {"code": code}}).to_string();
                socket.write_all(format!("HTTP/1.1 503 Unavailable\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                assert!(work.await.unwrap().is_err());
                {
                    let mut client = daemon.client.lock().await;
                    let last = client.last_sync().unwrap().unwrap();
                    assert_eq!(last.error.as_deref(), Some(code));
                    assert_eq!(last.stage, Some(dmsg_client::sync::SyncStage::ExportStart));
                    assert!(!sync_retry_due(&client, last.at + SYNC_RETRY_MS - 1));
                    assert_eq!(
                        sync_retry_due(&client, last.at + SYNC_RETRY_MS),
                        code != "INTEGRITY_FAILED"
                    );
                    if code != "UNAVAILABLE" {
                        client.lock();
                        assert!(!sync_retry_due(&client, last.at + 2 * SYNC_RETRY_MS));
                        continue;
                    }
                }
                // A successful pass removes the trigger, including when checked later.
                let work = tokio::spawn({
                    let daemon = daemon.clone();
                    async move { daemon.sync_pass().await }
                });
                for data in [
                    json!({"account": dmsg_client::testing::account().id.to_string(), "scope": "cloud_snapshot", "through": 0, "counts": []}),
                    json!({"entries": [], "next_cursor": 0, "inventory_end": true}),
                ] {
                    let (mut socket, _) = relay.accept().await.unwrap();
                    let mut request = [0; 8192];
                    assert!(socket.read(&mut request).await.unwrap() > 0);
                    let body = json!({"ok": true, "data": data}).to_string();
                    socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                }
                work.await.unwrap().unwrap();
                let client = daemon.client.lock().await;
                let last = client.last_sync().unwrap().unwrap();
                assert_eq!(last.error, None);
                assert!(!sync_retry_due(&client, last.at + SYNC_RETRY_MS));
            }
        })
        .await
        .expect("sync failure and recovery must finish");
    }

    #[tokio::test]
    async fn an_already_open_account_does_not_unlock_again() {
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, _relay, _ic) = stalled(&dir).await;
        // No keystore or login is configured: redoing the unlock would fail.
        for params in [
            json!({"fast": true}),
            json!({"fast": true, "account": dmsg_client::testing::account().id.to_string()}),
        ] {
            assert_eq!(
                daemon.call_daemon("session/unlock", &params).await.unwrap()["state"],
                "unlocked"
            );
        }
        assert!(!daemon.sync_requested.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn failed_unlock_preparation_notifies_after_dropping_the_previous_workspace() {
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, _relay, _ic) = stalled(&dir).await;
        let account: dmsg_types::AccountId = "140g2081040g2081040g".parse().unwrap();
        let target = dir.0.join("workspace/accounts").join(account.to_string());
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("workspace.sqlite"), b"corrupt workspace").unwrap();
        let mut events = daemon.events.subscribe();
        assert!(
            daemon
                .call_daemon(
                    "session/unlock",
                    &json!({"fast": true, "account": account.to_string()})
                )
                .await
                .is_err()
        );
        let event = events
            .try_recv()
            .expect("failed preparation must clear owner views");
        assert_eq!(event["type"], "session/changed");
        assert_ne!(event["data"]["state"], "unlocked");
        assert!(daemon.call_vault("vault/list", &Value::Null).await.is_err());
    }

    /// Its own daemon, so the user home's first connection is the profile's.
    #[tokio::test]
    async fn the_library_answers_while_another_profile_resolves() {
        let account = dmsg_client::testing::account().id.to_string();
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, _relay, ic) = stalled(&dir).await;
        // Another account's profile: its user home certifies its devices.
        let params = json!({"account": account});
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move { daemon.call_daemon("profile/resolve", &params).await }
        });
        answers_beside(&daemon, &ic, work).await;
    }

    #[tokio::test]
    async fn channel_readers_keep_running_while_scheduler_waits_for_client() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, relay, _ic) = stalled(&dir).await;
        let mut ledgers = std::collections::BTreeMap::new();
        {
            let mut client = daemon.client().lock().await;
            for n in 1..=5 {
                let id = format!("{n:064x}");
                ledgers.insert(
                    id.clone(),
                    dmsg_client::testing::joined_channel(&mut client, &id),
                );
            }
        }
        let background = tokio::spawn(crate::channels::run(daemon.clone()));
        let mut sockets = vec![];
        for _ in 0..4 {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), relay.accept())
                .await
                .expect("four channels should start without waiting for each other's response")
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut buffer = [0; 4096];
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if bytes.windows(4).any(|b| b == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            let path = request.split_whitespace().nth(1).unwrap();
            let id = path.strip_prefix("/v1/channels/").unwrap();
            sockets.push((socket, ledgers[id].clone()));
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(50), relay.accept())
                .await
                .is_err(),
            "the fifth channel waits for a slot"
        );
        let (mut socket, ledger) = sockets.pop().unwrap();
        // Deliberately hold the client until a reader has queued to settle
        // its reply, then make the scheduler queue for the same client.
        daemon.under_lock(async |_| {
            let body = json!({"ok": true, "data": ledger}).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            daemon.hint_channel("*");
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }).await.unwrap();
        tokio::time::timeout(
            Duration::from_secs(2),
            daemon.call_files("files/tree", &Value::Null),
        )
        .await
        .expect("reader and scheduler must not deadlock on the queued mutex permit")
        .unwrap();
        background.abort();
        let _ = background.await;
    }

    #[tokio::test]
    async fn channel_invites_and_rotations_do_not_block_local_reads() {
        for method in ["channel/invite", "channel/rotate", "channel/rekey"] {
            let dir = dmsg_client::testing::TempDir::new();
            let (daemon, relay, _ic) = stalled(&dir).await;
            let params = json!({"id": "c".repeat(64), "account": dmsg_client::testing::account().id.to_string()});
            let work = tokio::spawn({
                let daemon = daemon.clone();
                async move { daemon.call_more(method, &params).await }
            });
            tokio::time::timeout(
                Duration::from_secs(10),
                answers_beside(&daemon, &relay, work),
            )
            .await
            .expect("channel network waits must leave the client available");
        }
    }

    #[tokio::test]
    async fn the_library_answers_while_channel_images_names_and_receipts_load() {
        let account = dmsg_client::testing::account().id.to_string();
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, relay, ic) = stalled(&dir).await;
        let channel = "c".repeat(64);
        dmsg_client::testing::shared_image(&*daemon.client().lock().await, &channel, 7);
        // An image shown in place: the relay holds its upload.
        let params = json!({"id": channel, "seq": 7});
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move { daemon.call_more("channel/preview", &params).await }
        });
        answers_beside(&daemon, &relay, work).await;
        // Names return local data immediately; only the background worker waits.
        let params = json!({"accounts": [account]});
        let work = tokio::spawn(crate::display::run(daemon.clone()));
        assert_eq!(
            daemon.call_more("channel/names", &params).await.unwrap(),
            json!({})
        );
        answers_beside(&daemon, &ic, work).await;
        // How far the members took and read the messages: a signed relay read.
        let params = json!({"id": channel});
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move { daemon.call_more("channel/receipts", &params).await }
        });
        answers_beside(&daemon, &relay, work).await;
    }

    /// Each call has its own daemon, so the server's first connection is
    /// the call's.
    #[tokio::test]
    async fn the_library_answers_while_wallets_plans_events_and_letters_load() {
        let cases = [
            // The relay: the cloud's room, the event endpoints, the letters.
            ("files/cloud_usage", Value::Null, false),
            ("mcp-events/list", Value::Null, false),
            ("inbox/list", Value::Null, false),
            ("inbox/policy", Value::Null, false),
            ("agent/credentials", Value::Null, false),
            ("inbox/configure", json!({"mode":"free"}), false),
            (
                "inbox/mark",
                json!({"order":"ab".repeat(32),"read":true}),
                false,
            ),
            (
                "inbox/order",
                json!({"recipient":dmsg_client::testing::account().id.to_string(),"order":"ab".repeat(32)}),
                false,
            ),
            (
                "inbox/pay",
                json!({"recipient":dmsg_client::testing::account().id.to_string(),"order":"ab".repeat(32),"source":"gui"}),
                false,
            ),
            (
                "inbox/send",
                json!({"recipient":dmsg_client::testing::account().id.to_string(),"text":"hello"}),
                true,
            ),
            // The IC: a team wallet, the plan.
            ("wallet/show", json!({"wallet": "aaaaa-aa"}), true),
            ("wallet/freeze", json!({"wallet":"aaaaa-aa"}), true),
            (
                "wallet/execute",
                json!({"wallet":"aaaaa-aa","proposal":1,"source":"gui"}),
                true,
            ),
            (
                "wallet/settle",
                json!({"wallet":"aaaaa-aa","proposal":1}),
                true,
            ),
            ("plan/status", json!({"refresh": false}), true),
        ];
        for (method, params, on_ic) in cases {
            let dir = dmsg_client::testing::TempDir::new();
            let (daemon, relay, ic) = stalled(&dir).await;
            let work = tokio::spawn({
                let daemon = daemon.clone();
                async move {
                    let module = route(method).expect("a method");
                    daemon.call(module, method, &params).await
                }
            });
            answers_beside(&daemon, if on_ic { &ic } else { &relay }, work).await;
        }
    }

    #[tokio::test]
    async fn browser_pairing_waits_leave_local_reads_and_lock_available() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for method in [
            "connect/invite",
            "connect/candidates",
            "connect/grant",
            "connect/revoke",
        ] {
            let dir = dmsg_client::testing::TempDir::new();
            let (daemon, relay, _ic) = stalled(&dir).await;
            let invite = if matches!(method, "connect/candidates" | "connect/grant") {
                let creating = tokio::spawn({
                    let daemon = daemon.clone();
                    async move { daemon.call_daemon("connect/invite", &Value::Null).await }
                });
                let (mut socket, _) = tokio::time::timeout(Duration::from_secs(3), relay.accept())
                    .await
                    .unwrap()
                    .unwrap();
                let mut bytes = [0; 8192];
                assert!(socket.read(&mut bytes).await.unwrap() > 0);
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 21\r\nConnection: close\r\n\r\n{\"ok\":true,\"data\":{}}",
                    )
                    .await
                    .unwrap();
                drop(socket);
                creating.await.unwrap().unwrap()["invite_id"]
                    .as_str()
                    .unwrap()
                    .to_string()
            } else {
                "ab".repeat(32)
            };
            let work = tokio::spawn({
                let daemon = daemon.clone();
                async move {
                    daemon
                        .call_daemon(
                            method,
                            &json!({"invite":invite,"pairing":"cd".repeat(32),"human":true}),
                        )
                        .await
                }
            });
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(3), relay.accept())
                .await
                .expect(method)
                .unwrap();
            let mut bytes = [0; 8192];
            assert!(socket.read(&mut bytes).await.unwrap() > 0);
            tokio::time::timeout(Duration::from_secs(1), async {
                daemon.call_vault("vault/list", &Value::Null).await.unwrap();
                daemon.client().lock().await.lock();
            })
            .await
            .expect(method);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 21\r\nConnection: close\r\n\r\n{\"ok\":true,\"data\":{}}")
                .await
                .unwrap();
            drop(socket);
            let result = work.await.unwrap();
            if method == "connect/revoke" {
                assert!(result.is_ok());
            } else {
                assert_eq!(result, Err(Error::Locked), "{method}");
            }
        }
    }

    #[tokio::test]
    async fn agent_queries_and_approval_preparation_leave_reads_and_lock_available() {
        let account = dmsg_client::testing::account().id.to_string();
        let subject = dmsg_client::agent::agent_id(&[7; 32]);
        for (method, params, on_ic) in [
            ("agent/principal", Value::Null, true),
            (
                "agent/register",
                json!({"source":"gui", "unrestricted":true}),
                true,
            ),
            ("agent/enable", json!({"source":"gui"}), true),
            (
                "agent/rename",
                json!({"source":"gui", "generation":1, "name":"Agent"}),
                true,
            ),
            ("agent/refresh", Value::Null, false),
            (
                "agent/grant",
                json!({"source":"gui", "subject":subject, "scopes":["app.read"], "audiences":["https://example.com"]}),
                true,
            ),
            (
                "agent/revoke",
                json!({"source":"gui", "id":format!("{account}.test")}),
                false,
            ),
        ] {
            let dir = dmsg_client::testing::TempDir::new();
            let (daemon, relay, ic) = stalled(&dir).await;
            let work = tokio::spawn({
                let daemon = daemon.clone();
                async move { daemon.call_agents(method, &params).await }
            });
            let server = if on_ic { &ic } else { &relay };
            let (_socket, _) = tokio::time::timeout(Duration::from_secs(3), server.accept())
                .await
                .expect(method)
                .unwrap();
            tokio::time::timeout(Duration::from_secs(1), async {
                daemon.call_agents("agent/ids", &Value::Null).await.unwrap();
                daemon.call_vault("vault/list", &Value::Null).await.unwrap();
                daemon.client().lock().await.lock();
            })
            .await
            .expect(method);
            work.abort();
        }
    }

    #[tokio::test]
    async fn plan_quotes_and_name_preparation_leave_local_reads_and_lock_available() {
        use base64::Engine;
        let account = dmsg_client::testing::account().id.to_string();
        for (method, params) in [
            (
                "plan/quote",
                json!({"sku":"plus","method":"cash","selection":"aaaaa-aa"}),
            ),
            (
                "handle/register",
                json!({"name":"test_name","source":"gui"}),
            ),
            (
                "handle/transfer",
                json!({"name":"test_name","target":account}),
            ),
            ("handle/price", json!({"name":"test_name"})),
        ] {
            let dir = dmsg_client::testing::TempDir::new();
            let (daemon, _relay, ic) = stalled(&dir).await;
            // A deterministic test-only Ed25519 payer; connecting it uses the
            // fixture's root key and does not ask the blocked IC endpoint.
            let mut der = vec![
                0x30, 0x2e, 0x02, 0x01, 0, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
                0x04, 0x20,
            ];
            der.extend([7; 32]);
            let pem = dir.0.join("payer.pem");
            std::fs::write(
                &pem,
                format!(
                    "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
                    base64::engine::general_purpose::STANDARD.encode(der)
                ),
            )
            .unwrap();
            daemon
                .under_lock(async |c| c.payer_use(Some(&pem)).await)
                .await
                .unwrap();
            let work = tokio::spawn({
                let daemon = daemon.clone();
                async move { daemon.call(route(method).unwrap(), method, &params).await }
            });
            let (_socket, _) = tokio::time::timeout(Duration::from_secs(5), ic.accept())
                .await
                .expect(method)
                .unwrap();
            let parallel = match method {
                "handle/price" => 3,
                "handle/register" => 4,
                _ => 1,
            };
            let mut connections = Vec::new();
            for _ in 1..parallel {
                connections.push(
                    tokio::time::timeout(Duration::from_secs(2), ic.accept())
                        .await
                        .expect("independent name queries must start together")
                        .unwrap()
                        .0,
                );
            }
            tokio::time::timeout(Duration::from_secs(2), async {
                daemon.call_vault("vault/list", &Value::Null).await.unwrap();
                daemon.client().lock().await.lock();
            })
            .await
            .expect("local reads and locking must not wait for a quote or a name");
            work.abort();
        }
    }

    #[tokio::test]
    async fn a_large_statement_hash_leaves_the_executor_and_lock_available() {
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, _relay, _ic) = stalled(&dir).await;
        let path = dir.0.join("large.bin");
        std::fs::write(&path, vec![7; 32 * 1024 * 1024]).unwrap();
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move {
                daemon
                    .call_daemon(
                        "attest/request",
                        &json!({"source":"gui", "file":path, "statement":{"profile":"digest"}}),
                    )
                    .await
            }
        });
        tokio::task::yield_now().await;
        assert!(
            !work.is_finished(),
            "hashing must not monopolize the async executor"
        );
        tokio::time::timeout(Duration::from_secs(1), async {
            daemon.call_vault("vault/list", &Value::Null).await.unwrap();
            daemon.client().lock().await.lock();
        })
        .await
        .expect("the user must be able to read and lock while hashing");
        assert_eq!(work.await.unwrap(), Err(Error::Locked));
        assert_eq!(daemon.client().lock().await.approvals_waiting().unwrap(), 0);
    }

    /// A queued wallet action belongs to the account that submitted it,
    /// even when the selected workspace changes before its resource turn.
    #[tokio::test]
    async fn queued_wallet_work_does_not_follow_an_account_switch() {
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, _relay, _ic) = stalled(&dir).await;
        let turn = daemon.turn("wallet:aaaaa-aa").await;
        // Deliberately hold the client until the test operation queues its
        // first step; this synchronization wait uses the explicit exception.
        let work = daemon
            .under_lock(async |_client| {
                let (started, ready) = tokio::sync::oneshot::channel();
                let work = tokio::spawn({
                    let daemon = daemon.clone();
                    async move {
                        started.send(()).unwrap();
                        daemon
                            .call_wallet("wallet/freeze", &json!({"wallet":"aaaaa-aa"}))
                            .await
                    }
                });
                ready.await.unwrap();
                Ok(work)
            })
            .await
            .unwrap();
        // The first client acquisition is already queued. Tokio's FIFO mutex
        // lets it pin the account, then gives this task the following turn.
        {
            let mut client = daemon.client().lock().await;
            *client = Client::open(
                dmsg_client::testing::config(false),
                &dir.path("other-account"),
                None,
            )
            .unwrap();
        }
        drop(turn);
        let result = tokio::time::timeout(Duration::from_secs(5), work)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            result.err(),
            Some(Error::Unavailable("the account changed".into()))
        );
    }

    #[tokio::test]
    async fn queued_channel_controls_do_not_follow_an_account_switch() {
        for method in ["channel/invite", "channel/rotate", "channel/rekey"] {
            let dir = dmsg_client::testing::TempDir::new();
            let (daemon, _relay, _ic) = stalled(&dir).await;
            let channel = "c".repeat(64);
            let turn = daemon.turn(&format!("channel-control:{channel}")).await;
            let params = json!({
                "id": channel,
                "account": dmsg_client::testing::account().id.to_string(),
            });
            let mut work = Box::pin(daemon.call_more(method, &params));
            assert!(futures_util::poll!(&mut work).is_pending());
            {
                let mut client = daemon.client().lock().await;
                *client = Client::open(
                    dmsg_client::testing::config(false),
                    &dir.path("other-account"),
                    None,
                )
                .unwrap();
            }
            drop(turn);
            assert_eq!(
                work.await,
                Err(Error::Unavailable("the account changed".into())),
                "{method} must stay bound to the submitting account",
            );
        }
    }

    /// Each call has its own daemon, as above.
    #[tokio::test]
    async fn the_library_answers_while_approvals_are_asked() {
        use dmsg_types::Hash;
        use dmsg_types::app_action::{ActionArgs, ActionCommand, AppAction};
        let action = AppAction {
            version: 1,
            environment: dmsg_types::Environment::Local,
            app_id: "token-list".into(),
            app_config_version: 1,
            origin: "https://app.example".into(),
            receiver: candid::Principal::from_slice(&[9; 10]),
            actor: vec![8; 12].into(),
            signing_account: dmsg_client::testing::account().id,
            operation_id: Hash::new([1; 32]),
            intent_hash: Hash::new([2; 32]),
            schema_hash: Hash::new([3; 32]),
            issued_at_ms: 1,
            expires_at_ms: u64::MAX,
            command: ActionCommand {
                name: "Publish".into(),
                args: ActionArgs(vec![]),
            },
            files: vec![],
        };
        let action = dmsg_client::codec::b64(&dmsg_client::codec::canonical(&action));
        let cases = [
            // An application action: its registration, before it waits.
            ("attest/action", json!({"action": action})),
            // The account's certified devices, before asking to revoke one.
            ("device/revoke", json!({"device": "11".repeat(32)})),
        ];
        for (method, params) in cases {
            let dir = dmsg_client::testing::TempDir::new();
            let (daemon, _relay, ic) = stalled(&dir).await;
            let work = tokio::spawn({
                let daemon = daemon.clone();
                async move {
                    let module = route(method).expect("a method");
                    daemon.call(module, method, &params).await
                }
            });
            answers_beside(&daemon, &ic, work).await;
        }
    }

    #[tokio::test]
    async fn file_import_leaves_local_reads_and_lock_available() {
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, _relay, _ic) = stalled(&dir).await;
        let path = dir.0.join("import.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(2 * dmsg_client::files::CHUNK as u64)
            .unwrap();
        let blobs = Blobs::new(daemon.client().lock().await.files_blobs_dir().unwrap());
        let params = json!({"path": path});
        let mut work = Box::pin(daemon.call_files("files/import", &params));
        let mut read = Box::pin(daemon.call_files("files/tree", &Value::Null));
        // Queue the import, then the reader, behind the held client. Tokio's
        // FIFO mutex makes each reader stop the import at its next client step.
        // No file size or progress deadline stands in for synchronization.
        daemon
            .under_lock(async |_client| {
                assert!(futures_util::poll!(&mut work).is_pending());
                assert!(futures_util::poll!(&mut read).is_pending());
                Ok(())
            })
            .await
            .unwrap();
        loop {
            assert!(
                futures_util::poll!(&mut work).is_pending(),
                "local reads must finish before the import does"
            );
            let std::task::Poll::Ready(result) = futures_util::poll!(&mut read) else {
                panic!("local reads must run between import steps");
            };
            result.unwrap();
            if !blobs.entries().unwrap().is_empty() {
                break;
            }
            read = Box::pin(daemon.call_files("files/tree", &Value::Null));
            assert!(futures_util::poll!(&mut read).is_pending());
        }
        assert_eq!(blobs.entries().unwrap().len(), 1);
        // The first chunk is stored; put locking ahead of the next chunk.
        let mut lock = Box::pin(daemon.call_daemon("session/lock", &Value::Null));
        assert!(futures_util::poll!(&mut lock).is_pending());
        assert_eq!(
            futures_util::poll!(&mut work),
            std::task::Poll::Pending,
            "locking must run before the import completes"
        );
        let (locked, imported) = tokio::join!(lock, work);
        assert_eq!(locked.unwrap()["state"], "locked");
        assert_eq!(imported, Err(Error::Locked));
        assert_eq!(blobs.entries().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn the_library_answers_while_a_backup_starts() {
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, relay, _ic) = stalled(&dir).await;
        let path = dir.0.join("note.txt");
        std::fs::write(&path, b"a note").unwrap();
        let imported = daemon
            .call_files("files/import", &json!({"path": path}))
            .await
            .unwrap();
        // The backup reads the cloud's room, then uploads.
        let params = json!({"ids": [imported[0]["file"]["id"]]});
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move { daemon.call_files("files/backup", &params).await }
        });
        answers_beside(&daemon, &relay, work).await;
    }

    #[tokio::test]
    async fn an_unlock_moves_the_idle_lock() {
        let dir = dmsg_client::testing::TempDir::new();
        let home = Home {
            dir: dir.0.join("home"),
        };
        let storage = Arc::new(dmsg_client::testing::MemoryStorage::default());
        let mut client =
            dmsg_client::testing::unlocked(&dir.0.join("workspace"), false, Some(storage));
        client.enable_fast_unlock().unwrap();
        client.lock();
        let daemon = Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            client,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        );
        // The desktop reports only input in its windows, and a window that
        // asks for Touch ID as it comes to the front has none.
        daemon.activity.store(0, Ordering::Relaxed);
        let status = daemon
            .call_daemon("session/unlock", &json!({"fast": true}))
            .await
            .unwrap();
        assert_eq!(status["state"], "unlocked");
        assert!(daemon.sync_requested.load(Ordering::Relaxed));
        assert!(status["last_sync"].is_null());
        assert!(daemon.activity.load(Ordering::Relaxed) + daemon.settings.idle_lock_ms > now_ms());
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
        // A client that reports its person's activity: only the reports count.
        assert!(!is_activity(&request("vault/list", Value::Null), true));
        assert!(is_activity(&request("app/activity", Value::Null), true));
        assert!(is_activity(&request("app/activity", Value::Null), false));
    }
}
