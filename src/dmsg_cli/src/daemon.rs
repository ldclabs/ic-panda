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
//! Approval requests are answered only through `approval/respond` with
//! `human: true`, which only the desktop approval window and an interactive
//! terminal send. The daemon keeps a WebSocket to the relay's hint channel
//! with a bearer token that receives content-free hints, locked or not: a
//! `connect` hint fetches the mailbox, or, locked, tells the desktop to
//! notify the user.

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
    atomic::{AtomicU64, Ordering},
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
const METHODS: [&str; 50] = [
    "session/status",
    "session/login",
    "session/login_result",
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
    "workspace/reset",
    "daemon/stop",
    "approval/list",
    "approval/show",
    "approval/respond",
    "approval/cancel",
    "attest/request",
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
    outcome: Option<std::result::Result<(), String>>,
}

pub(crate) struct Daemon {
    token: String,
    instance: String,
    port: u16,
    settings: Settings,
    client: Mutex<Client>,
    login: std::sync::Mutex<LoginState>,
    activity: AtomicU64,
    stop: Notify,
    events: broadcast::Sender<Value>,
    seq: AtomicU64,
    hints: std::sync::Mutex<HintState>,
    wake: Notify,
    /// The latest mailbox pass, refusal codes included; nothing secret.
    last_connect: std::sync::Mutex<Option<Value>>,
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
    let token = home.owner_token(&settings, true)?;
    let client = Client::open(
        settings.config.clone(),
        &home.workspace(&settings.config),
        Home::secure(&settings),
    )?;
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
        stop: Notify::new(),
        events: broadcast::channel(256).0,
        seq: AtomicU64::new(0),
        hints: std::sync::Mutex::new(HintState::default()),
        wake: Notify::new(),
        last_connect: std::sync::Mutex::new(None),
    });
    home.publish(&Discovery {
        port,
        pid: std::process::id(),
        instance: daemon.instance.clone(),
    })?;
    tokio::spawn(background(daemon.clone()));
    tokio::spawn(crate::hints::run(daemon.clone()));
    let app = Router::new()
        .route("/daemon/status", get(status))
        .route("/app/v1", post(rpc))
        .route("/ws/app/v1", get(socket))
        .route("/auth/callback", get(callback_page).post(callback))
        .with_state(daemon.clone());
    eprintln!("dmsg daemon listening on 127.0.0.1:{port}");
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
        let now = now_ms();
        let mut client = daemon.client.lock().await;
        let before = client.status().map(|s| s.state).unwrap_or_default();
        if now > daemon.activity.load(Ordering::Relaxed) + daemon.settings.idle_lock_ms {
            client.lock();
        }
        let status = client.status().ok();
        if status.as_ref().map(|s| s.state) != Some(before) {
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
            if client.sync().await.is_ok() {
                daemon.emit("sync/changed", Value::Null);
            }
        } else {
            let _ = client.flush().await;
        }
        if ready && now >= polled + CONNECT_POLL_MS {
            polled = now;
            if let Ok(report) = client.connect_sync().await {
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
        Ok(value)
    }

    async fn start_login(&self, params: &Value) -> Result<Value> {
        let origin = params["origin"].as_str().unwrap_or("https://dmsg.net");
        let config = self.client.lock().await.config().clone();
        let targets = login_targets(&config).await?;
        let session = LoginSession::new(&targets, origin, self.port, DEFAULT_TTL_MS, now_ms())?;
        let reply = json!({"url": session.url(&config), "expires_at": session.deadline});
        *self.login.lock().expect("login state") = LoginState {
            session: Some(session),
            outcome: None,
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
                let (pending, outcome) = {
                    let login = self.login.lock().expect("login state");
                    let pending = login
                        .session
                        .as_ref()
                        .is_some_and(|s| s.deadline > now_ms());
                    (pending, login.outcome.clone())
                };
                Ok(json!({
                    "pending": pending,
                    "error": outcome.and_then(|o| o.err()),
                    "status": self.status().await?,
                }))
            }
            "session/unlock" => {
                let mut client = self.client.lock().await;
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
                let account = <AccountId as std::str::FromStr>::from_str(&text("account")?)
                    .map_err(|_| Error::InvalidInput("account".into()))?;
                let profile = self.client.lock().await.profile_resolve(&account).await?;
                Ok(to_json(&profile))
            }
            "workspace/reset" => {
                let mut client = self.client.lock().await;
                // The caller names this device, as shown in the status.
                if text("device")? != client.status()?.device_id {
                    return Err(Error::InvalidInput("device does not match".into()));
                }
                Ok(to_json(&client.reset()?))
            }
            "daemon/stop" => {
                self.stop.notify_one();
                Ok(Value::Null)
            }
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
                self.client
                    .lock()
                    .await
                    .approval_respond(&text("id")?, &text("digest")?, approve)
                    .await
            }
            "approval/cancel" => self.client.lock().await.approval_cancel(&text("id")?),
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
            _ => Err(Error::NotFound),
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
        "session/login" | "session/login_result" | "session/fast_unlock" => &[],
        m if m.starts_with("session/") || m == "workspace/reset" || m == "account/create" => {
            &["session/changed", "approval/changed"]
        }
        m if m.starts_with("vault/") || m.starts_with("file/") => &["vault/changed"],
        "sync/run" => &["sync/changed", "vault/changed"],
        "approval/respond"
        | "approval/cancel"
        | "attest/request"
        | "device/capabilities"
        | "device/formal_approve"
        | "device/revoke"
        | "connect/sync" => &["approval/changed"],
        "connect/grant" | "connect/revoke" => &["connect/changed"],
        "device/approve" => &["session/changed"],
        _ => &[],
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
    if !daemon.owner(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(request) = serde_json::from_slice::<RpcRequest>(&body) else {
        return Json(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}))
            .into_response();
    };
    Json(daemon.dispatch(request).await).into_response()
}

impl Daemon {
    /// Run one request and announce what it changed.
    async fn dispatch(&self, request: RpcRequest) -> Value {
        if request.jsonrpc != "2.0" {
            return json!({"jsonrpc": "2.0", "id": request.id, "error": {"code": -32600, "message": "invalid request"}});
        }
        if !METHODS.contains(&request.method.as_str()) {
            return json!({"jsonrpc": "2.0", "id": request.id, "error": {"code": -32601, "message": "method not found"}});
        }
        if !matches!(
            request.method.as_str(),
            "events/poll" | "daemon/status" | "app/initialize"
        ) {
            self.activity.store(now_ms(), Ordering::Relaxed);
        }
        match self.call(&request.method, &request.params).await {
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
                    let _ = replies.send(daemon.dispatch(request).await).await;
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
            Ok(delegated) => daemon
                .client
                .lock()
                .await
                .login(delegated.identity, delegated.expires_at)
                .await
                .map(|_| ()),
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
