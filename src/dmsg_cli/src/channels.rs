//! Channel hints: while unlocked, one WebSocket per active channel says new
//! messages are available, and the daemon reads them with the signed HTTP
//! sync. A socket's authorization ends with its ticket's evidence, about a
//! minute, so each is renewed before then: the new socket opens first and
//! closes the one it replaces, so no hint falls between them. A socket that
//! opens with none before it reads once, as hints sent while none was open
//! are lost, and so does one that closes, which may have missed one; the
//! relay closes stale ones on the next message.

use crate::daemon::Daemon;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_tungstenite::tungstenite::Message;

/// A socket is renewed after this long.
const RENEW: Duration = Duration::from_secs(40);
/// How often the set of channels is checked.
const TICK: Duration = Duration::from_secs(5);

/// A channel's hint socket.
struct Socket {
    task: JoinHandle<()>,
    opened: Instant,
    /// The socket it renews, which it closes once open.
    previous: Option<AbortHandle>,
}

impl Socket {
    fn close(&self) {
        self.task.abort();
        if let Some(previous) = &self.previous {
            previous.abort();
        }
    }
}

/// Keep a hint socket per active channel while the daemon is unlocked.
pub async fn run(daemon: Arc<Daemon>) {
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let mut sockets: BTreeMap<String, Socket> = BTreeMap::new();
    // A steady tick, which reads in between do not postpone.
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            Some(channel) = rx.recv() => {
                sync(&daemon, &channel).await;
                continue;
            }
        }
        let channels = active(&daemon).await;
        sockets.retain(|id, socket| {
            let keep = channels.contains(id) && !socket.task.is_finished();
            if !keep {
                socket.close();
            }
            keep
        });
        for channel in channels {
            let previous = match sockets.get(&channel) {
                Some(socket) if socket.opened.elapsed() < RENEW => continue,
                Some(socket) => {
                    // Whatever this one replaced is closed by now.
                    if let Some(previous) = &socket.previous {
                        previous.abort();
                    }
                    Some(socket.task.abort_handle())
                }
                None => None,
            };
            let url = match ticket(&daemon, &channel).await {
                Ok(url) => url,
                Err(_) => continue,
            };
            let tx = tx.clone();
            let id = channel.clone();
            let replaces = previous.clone();
            let task = tokio::spawn(async move {
                listen(&url, &id, &tx, replaces).await;
                // A closed socket may have missed a hint.
                let _ = tx.send(id).await;
            });
            sockets.insert(
                channel,
                Socket {
                    task,
                    opened: Instant::now(),
                    previous,
                },
            );
        }
    }
}

/// A live address of `channel`: the relay's ticket is asked for without
/// holding the client.
async fn ticket(daemon: &Daemon, channel: &str) -> dmsg_types::Result<String> {
    let (relay, (request, ticket)) = daemon
        .signed(|client| client.channel_ticket_request(channel))
        .await?;
    let value = relay.send(request).await?;
    daemon
        .client()
        .lock()
        .await
        .channel_ticket_url(channel, &ticket, &value)
}

/// Active channels, while unlocked; none otherwise.
async fn active(daemon: &Daemon) -> Vec<String> {
    let client = daemon.client().lock().await;
    if client.session_state().unlocked_account().is_none() {
        return vec![];
    }
    client
        .channel_list()
        .unwrap_or_default()
        .into_iter()
        .filter(|c| {
            c.ledger
                .as_ref()
                .is_some_and(|l| l.status == dmsg_client::channel_log::Status::Active)
        })
        .map(|c| c.id)
        .collect()
}

/// Read hints until the socket closes. Once open, it closes the socket it
/// renews, or else reads once.
async fn listen(
    url: &str,
    channel: &str,
    tx: &mpsc::Sender<String>,
    replaces: Option<AbortHandle>,
) {
    let Ok((socket, _)) = tokio_tungstenite::connect_async(url).await else {
        return;
    };
    match replaces {
        Some(previous) => previous.abort(),
        // Hints sent while no socket was open are lost.
        None => {
            if tx.send(channel.to_string()).await.is_err() {
                return;
            }
        }
    }
    let (_, mut stream) = socket.split();
    while let Some(Ok(message)) = stream.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(hint) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if hint["type"] == "available" && tx.send(channel.to_string()).await.is_err() {
            return;
        }
    }
}

async fn sync(daemon: &Daemon, channel: &str) {
    let result = daemon
        .client()
        .lock()
        .await
        .channel_sync(channel, false)
        .await;
    if let Ok(report) = result
        && report.received > 0
    {
        daemon.emit(
            "channel/changed",
            json!({"id": channel, "received": report.received}),
        );
    }
}
