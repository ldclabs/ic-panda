//! The relay's hint channel (connect-v1 §7.5): a WebSocket authenticated by
//! a bearer token that receives only content-free hints, so a locked daemon
//! still learns that a web request, an MCP event, a channel message or a
//! letter waits. The token is registered while the
//! device key is available and renewed a week before it expires.

use crate::daemon::Daemon;
use dmsg_client::{diagnostic::Diagnostic, now_ms};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

/// The client heartbeat; the relay answers each `ping` with `pong`.
const PING: Duration = Duration::from_secs(25);
/// Silence after which the connection counts as lost.
const SILENCE: Duration = Duration::from_secs(60);

/// Keep the hint channel connected for as long as the daemon runs.
pub async fn run(daemon: Arc<Daemon>) {
    let mut tasks: std::collections::BTreeMap<String, (String, tokio::task::JoinHandle<()>)> =
        Default::default();
    loop {
        let mut endpoints = std::collections::BTreeMap::new();
        if let Ok(Some((url, token, account))) = endpoint(&daemon).await {
            endpoints.insert(account, (url, token));
        }
        {
            let client = daemon.client().lock().await;
            for context in client.notification_receivers().unwrap_or_default() {
                if endpoints.contains_key(&context.account) {
                    continue;
                }
                if let Ok(Some(token)) = client.notification_hint_token(&context) {
                    let origin = daemon
                        .settings()
                        .config
                        .relay_origin()
                        .replacen("https://", "wss://", 1)
                        .replacen("http://", "ws://", 1);
                    endpoints.insert(
                        context.account.clone(),
                        (
                            format!("{origin}/v1/hints/ws?account={}", context.account),
                            token.token,
                        ),
                    );
                }
            }
        }
        tasks.retain(|account, (token, task)| {
            if endpoints
                .get(account)
                .is_some_and(|(_, current)| current == token)
                && !task.is_finished()
            {
                true
            } else {
                task.abort();
                false
            }
        });
        for (account, (url, token)) in endpoints {
            if tasks.contains_key(&account) {
                continue;
            }
            let d = daemon.clone();
            let original = account.clone();
            let saved = token.clone();
            let task = tokio::spawn(async move {
                let mut backoff = 1;
                loop {
                    let result = connect_account(&d, &url, &token, &original).await;
                    if let Err(error) = &result {
                        error.emit();
                    }
                    if d.client().lock().await.session_state().account.as_deref()
                        == Some(original.as_str())
                    {
                        d.set_hints(|h| {
                            h.connected = false;
                            h.last_error = result
                                .as_ref()
                                .err()
                                .map(|e| format!("{}: {}", e.source, e.reason));
                        });
                    }
                    backoff = if result.is_ok() {
                        1
                    } else {
                        (backoff * 2).min(60)
                    };
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                }
            });
            tasks.insert(account, (saved, task));
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(30)) => {},
            _ = daemon.hints_changed().notified() => {},
        }
    }
}

/// The account of the hint channel, its relay URL and the token,
/// registering one when the device key is available; `None` before the
/// device has an account.
async fn endpoint(daemon: &Daemon) -> Result<Option<(String, String, String)>, Diagnostic> {
    let (session, stored) = {
        let client = daemon.client().lock().await;
        (
            client.session_state(),
            client
                .stored_hint_token()
                .map_err(|e| Diagnostic::error(&e))?,
        )
    };
    let Some(account) = session.account else {
        return Ok(None);
    };
    let token = match stored {
        Some(token) if token.expires_at > now_ms() + 7 * dmsg_types::DAY => token,
        // A new token, about weekly, registers outside the client lock.
        stored if session.state == "unlocked" || session.resident => {
            match dmsg_client::connect::hint_token(&daemon.hold()).await {
                Ok(token) => token,
                Err(_) if stored.is_some() => stored.expect("stored token"),
                Err(error) => return Err(Diagnostic::error(&error)),
            }
        }
        Some(token) => token,
        None => return Ok(None),
    };
    let relay = daemon.settings().config.relay_origin();
    let ws = relay
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    Ok(Some((
        format!("{ws}/v1/hints/ws?account={account}"),
        token.token,
        account,
    )))
}

/// One connection, until it drops.
async fn connect_account(
    daemon: &Arc<Daemon>,
    url: &str,
    token: &str,
    account: &str,
) -> Result<(), Diagnostic> {
    let mut request = url.into_client_request().map_err(socket_failure)?;
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {token}")
            .parse()
            .map_err(|_| Diagnostic::new("hints", "invalid_endpoint"))?,
    );
    let (socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(socket_failure)?;
    if daemon
        .client()
        .lock()
        .await
        .session_state()
        .account
        .as_deref()
        == Some(account)
    {
        daemon.set_hints(|h| {
            h.connected = true;
            h.last_error = None;
        });
    }
    daemon.note("hints", "connected");
    // Hints sent while it was down are lost: catch up authoritative state.
    daemon.notifications.receive.notify_one();
    if daemon
        .client()
        .lock()
        .await
        .session_state()
        .account
        .as_deref()
        == Some(account)
    {
        daemon.hint_channel("*");
        daemon.notifications.inbox.notify_one();
        daemon.mail_push().receive.notify_one();
        daemon.mcp().wake();
    }
    let (mut sink, mut stream) = socket.split();
    let mut ping = tokio::time::interval(PING);
    let mut heard = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if heard.elapsed() > SILENCE {
                    return Err(Diagnostic::new("hints", "heartbeat_timeout"));
                }
                sink.send(Message::Text("ping".into())).await.map_err(socket_failure)?;
            }
            message = stream.next() => {
                let Some(message) = message else { return Ok(()) };
                let message = message.map_err(socket_failure)?;
                heard = tokio::time::Instant::now();
                let Message::Text(text) = message else { continue };
                let Ok(hint) = serde_json::from_str::<Value>(&text) else { continue };
                if hint["type"] == "hint" {
                    daemon.set_hints(|h| h.received += 1);
                    handle(daemon, account, &hint).await;
                }
            }
        }
    }
}

/// WebSocket errors may contain a handshake body, URL or close-frame text.
fn socket_failure(error: tokio_tungstenite::tungstenite::Error) -> Diagnostic {
    use tokio_tungstenite::tungstenite::Error;
    match error {
        Error::Io(error) => Diagnostic::io("hints", &error),
        Error::Tls(_) => Diagnostic::new("hints", "tls"),
        Error::Http(response) => Diagnostic::http("hints", response.status().as_u16()),
        Error::ConnectionClosed | Error::AlreadyClosed => {
            Diagnostic::new("hints", "connection_closed")
        }
        Error::Url(_) => Diagnostic::new("hints", "invalid_endpoint"),
        _ => Diagnostic::new("hints", "protocol"),
    }
}

/// Act on one hint. `connect`: fetch the mailbox when the keys are here,
/// else tell the desktop, which notifies without saying from where.
/// `mcp-event`: wake the fetch, which runs while unlocked. `channel`:
/// unlocked, the channel loop reads the channel the hint names; locked,
/// the desktop notifies that messages arrived, never for a control.
/// `inbox`: locked, the desktop notifies; unlocked, the inbox view reloads.
async fn handle(daemon: &Arc<Daemon>, expected_account: &str, hint: &Value) {
    let kind = hint["kind"].as_str().unwrap_or_default();
    let session = daemon.client().lock().await.session_state();
    if hint["account"].as_str() != Some(expected_account) {
        return;
    }
    if session.account.as_deref() != Some(expected_account) {
        if matches!(kind, "channel" | "inbox" | "notification") {
            daemon.notifications.receive.notify_one();
        }
        if kind == "connect" {
            daemon.notification_hint(expected_account, kind, None).await;
        }
        return;
    }
    let unlocked = session.state == "unlocked";
    let ready = unlocked || session.resident;
    match kind {
        "notification" => daemon.notifications.receive.notify_one(),
        "connect" if ready => daemon.wake(),
        "connect" => {
            daemon.notification_hint(expected_account, kind, None).await;
            daemon.emit("connect/hint", json!({"locked": true}));
        }
        "mcp-event" => {
            daemon.mcp().wake();
            // The hint is only a wakeup. A resident Agent can check its own
            // Mail without decrypting the device's general event envelopes.
            daemon.mail_push().receive.notify_one();
            daemon.emit("hint", json!({"kind": kind}));
        }
        "inbox" if unlocked => {
            daemon.notifications.inbox.notify_one();
            daemon.notifications.receive.notify_one();
        }
        "channel" if unlocked => {
            daemon.hint_channel(hint["resource"].as_str().unwrap_or("*"));
            daemon.notifications.receive.notify_one();
        }
        "channel" if hint["event"] == "control" => {}
        "channel" | "inbox" => {
            daemon.notifications.receive.notify_one();
            let supported = daemon
                .client()
                .lock()
                .await
                .notification_receivers()
                .unwrap_or_default()
                .iter()
                .any(|c| c.account == expected_account && c.expires_at > now_ms());
            if !supported && let Some(account) = session.account.as_deref() {
                daemon
                    .notification_hint(account, kind, hint["resource"].as_str())
                    .await;
            }
            daemon.emit("hint", json!({"kind": kind}));
        }
        _ => {}
    }
}
