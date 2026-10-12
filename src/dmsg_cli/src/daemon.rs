//! `dmsg daemon`: the only process that holds keys and state.
//!
//! It listens on a loopback port. `/daemon/status` and `/app/v1` (JSON-RPC
//! 2.0 over HTTP POST) need the owner credential and refuse any request with
//! an `Origin`, so web pages cannot even learn whether it runs. The login
//! callback is open only while a login is pending, accepts one POST from its
//! own loopback origin and then closes (`docs/protocol/login-v1_zh.md` §4).

use crate::home::{Discovery, Home};
use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use dmsg_client::{
    account::login_targets,
    login::{LoginSession, DEFAULT_TTL_MS},
    now_ms,
    vault::{Item, VaultEntry},
    Client,
};
use dmsg_types::{user::ControllerRole, Error, Result, MINUTE};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::{Mutex, Notify};

/// Inactivity after which the daemon locks.
const IDLE_MS: u64 = 15 * MINUTE;

/// The JSON-RPC methods of `/app/v1`.
const METHODS: [&str; 14] = [
    "session/status",
    "session/login",
    "session/login_result",
    "session/unlock",
    "session/lock",
    "account/create",
    "account/show",
    "device/pair_request",
    "device/approve",
    "vault/put",
    "vault/list",
    "vault/show",
    "sync/run",
    "daemon/stop",
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

struct Daemon {
    token: String,
    instance: String,
    port: u16,
    client: Mutex<Client>,
    login: std::sync::Mutex<LoginState>,
    activity: AtomicU64,
    stop: Notify,
}

#[derive(Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

fn error_code(error: &Error) -> String {
    let name = format!("{error:?}");
    let name = name.split('(').next().unwrap_or_default();
    let mut code = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            code.push('_');
        }
        code.push(c.to_ascii_uppercase());
    }
    code
}

/// Run the daemon in the foreground until stopped.
///
/// # Errors
/// Returns `Error::Locked` when another daemon serves this home.
pub async fn run(home: Home, port: u16) -> Result<()> {
    let config = home.config()?;
    let _lock = home.lock()?;
    let token = home.owner_token(true)?;
    let client = Client::open(config.clone(), &home.workspace(&config))?;
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
        client: Mutex::new(client),
        login: std::sync::Mutex::new(LoginState::default()),
        activity: AtomicU64::new(now_ms()),
        stop: Notify::new(),
    });
    home.publish(&Discovery {
        port,
        pid: std::process::id(),
        instance: daemon.instance.clone(),
    })?;
    let idle = daemon.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            if now_ms() > idle.activity.load(Ordering::Relaxed) + IDLE_MS {
                idle.client.lock().await.lock();
            }
        }
    });
    let app = Router::new()
        .route("/daemon/status", get(status))
        .route("/app/v1", post(rpc))
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
    daemon.client.lock().await.lock();
    home.unpublish();
    served.map_err(|e| Error::Unavailable(format!("serve: {e}")))
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
                let status = self.client.lock().await.unlock().await?;
                Ok(to_json(&status))
            }
            "session/lock" => {
                self.client.lock().await.lock();
                self.status().await
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
                let id = params["id"].as_str();
                let entry = self.client.lock().await.vault_put(id, &item)?;
                Ok(to_json(&masked(entry, false)))
            }
            "vault/list" => {
                let entries = self.client.lock().await.vault_list()?;
                let entries: Vec<VaultEntry> =
                    entries.into_iter().map(|e| masked(e, false)).collect();
                Ok(to_json(&entries))
            }
            "vault/show" => {
                let entry = self.client.lock().await.vault_show(&text("id")?)?;
                Ok(to_json(&masked(entry, reveal)))
            }
            "sync/run" => {
                let report = self.client.lock().await.sync().await?;
                Ok(to_json(&report))
            }
            "daemon/stop" => {
                self.stop.notify_one();
                Ok(Value::Null)
            }
            _ => Err(Error::NotFound),
        }
    }
}

/// Secret fields leave the daemon only on explicit request.
fn masked(mut entry: VaultEntry, reveal: bool) -> VaultEntry {
    if !reveal && !entry.item.secret.is_empty() {
        entry.item.secret = "********".into();
    }
    entry
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
    if request.jsonrpc != "2.0" {
        return Json(json!({"jsonrpc": "2.0", "id": request.id, "error": {"code": -32600, "message": "invalid request"}}))
            .into_response();
    }
    if !METHODS.contains(&request.method.as_str()) {
        return Json(json!({"jsonrpc": "2.0", "id": request.id, "error": {"code": -32601, "message": "method not found"}}))
            .into_response();
    }
    daemon.activity.store(now_ms(), Ordering::Relaxed);
    let reply = match daemon.call(&request.method, &request.params).await {
        Ok(result) => json!({"jsonrpc": "2.0", "id": request.id, "result": result}),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "error": {"code": -32000, "message": error.to_string(), "data": {"code": error_code(&error)}},
        }),
    };
    Json(reply).into_response()
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
