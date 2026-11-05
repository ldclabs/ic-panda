//! The relay's hint channel (connect-v1 §7.5): a WebSocket authenticated by
//! a bearer token that receives only content-free hints, so a locked daemon
//! still learns that a web request, an MCP event, a channel message or a
//! letter waits, and an unlocked one that the account's content moved. The
//! token is registered while the device key is available and renewed a week
//! before it expires.

use crate::daemon::Daemon;
use dmsg_client::hint_channel::{self, Connection, Hint};
use dmsg_client::{diagnostic::Diagnostic, now_ms, sync::RetryCause};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// A `content` hint is followed by one more read after this long: the relay
/// hints at most every five seconds.
const CONTENT_AGAIN: Duration = Duration::from_secs(6);

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
                    let origin = daemon.settings().config.relay_origin();
                    endpoints.insert(
                        context.account.clone(),
                        (hint_channel::url(&origin, &context.account), token.token),
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
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(backoff)) => {},
                        _ = d.hints_redial().notified() => {},
                    }
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
    Ok(Some((
        hint_channel::url(&relay, &account),
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
    let mut connection = Connection::open(url, token).await?;
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
    let current = {
        let client = daemon.client().lock().await;
        let current = client.session_state().account.as_deref() == Some(account);
        if current {
            // Reaching the relay again ends a wait for the network.
            let _ = dmsg_client::sync::retry_early(&client, RetryCause::Network);
        }
        current
    };
    if current {
        daemon.hint_channel("*");
        daemon.emit("channel/hint", json!({"id":"*"}));
        daemon.request_content_sync();
        daemon.notifications.inbox.notify_one();
        daemon.mail_push().receive.notify_one();
        daemon.mcp().wake();
    }
    loop {
        tokio::select! {
            // A socket from before a wake or network change is likely dead.
            _ = daemon.hints_redial().notified() => return Ok(()),
            hint = connection.next() => {
                let Some(hint) = hint? else { return Ok(()) };
                daemon.set_hints(|h| h.received += 1);
                handle(daemon, account, &hint).await;
            }
        }
    }
}

/// Act on one hint. `connect`: fetch the mailbox when the keys are here,
/// else tell the desktop, which notifies without saying from where.
/// `mcp-event`: wake the fetch, which runs while unlocked. `channel`:
/// unlocked, the channel loop reads the channel the hint names; locked,
/// the desktop notifies that messages arrived, never for a control.
/// `inbox`: locked, the desktop notifies; unlocked, the inbox view reloads.
/// `content`: unlocked, a device behind its cursor syncs now, and every device
/// reads once more after [`CONTENT_AGAIN`] for what the relay did not hint.
pub(crate) async fn handle(daemon: &Arc<Daemon>, expected_account: &str, hint: &Hint) {
    let kind = hint.kind.as_str();
    let session = daemon.client().lock().await.session_state();
    if hint.account != expected_account {
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
            let channel = hint.resource.as_deref().unwrap_or("*");
            daemon.hint_channel(channel);
            daemon.emit("channel/hint", json!({"id": channel}));
            daemon.notifications.receive.notify_one();
        }
        // Another device of this account read further: the read of the head takes it.
        "read" if unlocked => daemon.hint_channel(hint.resource.as_deref().unwrap_or("*")),
        "read" => {}
        "channel" if hint.event.as_deref() == Some("control") => {}
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
                    .notification_hint(account, kind, hint.resource.as_deref())
                    .await;
            }
            daemon.emit("hint", json!({"kind": kind}));
        }
        // A file arrived through a request link; listing it stays explicit.
        "file-request" if unlocked => {
            daemon.emit("files/changed", json!({"method": "file-request"}));
        }
        "content" if unlocked => {
            let news = {
                let client = daemon.client().lock().await;
                let news = dmsg_client::sync::hint_is_news(&client, hint.cursor);
                if news {
                    let _ = dmsg_client::sync::retry_early(&client, RetryCause::Network);
                }
                news
            };
            if news {
                daemon.request_content_sync();
            }
            let later = daemon.clone();
            tokio::spawn(async move {
                tokio::time::sleep(CONTENT_AGAIN).await;
                later.request_content_sync();
            });
        }
        _ => {}
    }
}
