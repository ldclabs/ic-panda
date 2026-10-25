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
    let mut backoff = 1;
    loop {
        match connect(&daemon).await {
            Ok(()) => backoff = 1,
            Err(error) => {
                error.emit();
                daemon.note("hints", "disconnected");
                daemon.set_hints(|h| {
                    h.connected = false;
                    h.last_error = Some(format!("{}: {}", error.source, error.reason));
                });
                backoff = (backoff * 2).min(60);
            }
        }
        daemon.set_hints(|h| h.connected = false);
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(backoff)) => {},
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
async fn connect(daemon: &Arc<Daemon>) -> Result<(), Diagnostic> {
    let Some((url, token, account)) = endpoint(daemon).await? else {
        // Login/unlock must not wait out the idle polling interval.
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(20)) => {},
            _ = daemon.hints_changed().notified() => {},
        }
        return Ok(());
    };
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
    daemon.set_hints(|h| {
        h.connected = true;
        h.last_error = None;
    });
    daemon.note("hints", "connected");
    // Hints sent while it was down are lost: catch up authoritative state.
    daemon.hint_channel("*");
    daemon.mail_push().receive.notify_one();
    daemon.mcp().wake();
    let (mut sink, mut stream) = socket.split();
    let mut ping = tokio::time::interval(PING);
    let mut heard = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = daemon.hints_changed().notified() => {
                let selected = daemon.client().lock().await.session_state().account;
                if selected.as_deref() != Some(account.as_str()) {
                    return Ok(());
                }
            }
            _ = ping.tick() => {
                if heard.elapsed() > SILENCE {
                    return Err(Diagnostic::new("hints", "heartbeat_timeout"));
                }
                // Another account selected: its own channel replaces this one.
                let selected = daemon.client().lock().await.session_state().account;
                if selected.as_deref() != Some(account.as_str()) {
                    return Ok(());
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
                    handle(daemon, &hint).await;
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
async fn handle(daemon: &Arc<Daemon>, hint: &Value) {
    let kind = hint["kind"].as_str().unwrap_or_default();
    let session = daemon.client().lock().await.session_state();
    let unlocked = session.state == "unlocked";
    let ready = unlocked || session.resident;
    match kind {
        "connect" if ready => daemon.wake(),
        "connect" => daemon.emit("connect/hint", json!({"locked": true})),
        "mcp-event" => {
            daemon.mcp().wake();
            // The hint is only a wakeup. A resident Agent can check its own
            // Mail without decrypting the device's general event envelopes.
            daemon.mail_push().receive.notify_one();
            daemon.emit("hint", json!({"kind": kind}));
        }
        "inbox" if unlocked => daemon.emit("inbox/changed", json!({"hint": true})),
        "channel" if unlocked => daemon.hint_channel(hint["resource"].as_str().unwrap_or("*")),
        "channel" if hint["event"] == "control" => {}
        "channel" | "inbox" => daemon.emit("hint", json!({"kind": kind})),
        _ => {}
    }
}
