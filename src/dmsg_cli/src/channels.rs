//! Channel hints: while unlocked, one WebSocket per active channel says new
//! messages are available, and the daemon reads them with the signed HTTP
//! sync. A socket's authorization ends with its ticket's evidence, about a
//! minute, so each is renewed before then. Every new socket reads once, as
//! hints sent while none was open are lost, and so does a socket the relay
//! closes, since it closes stale ones on the next message.

use crate::daemon::Daemon;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// A socket is renewed after this long.
const RENEW: Duration = Duration::from_secs(40);
/// How often the set of channels is checked.
const TICK: Duration = Duration::from_secs(5);

/// Keep a hint socket per active channel while the daemon is unlocked.
pub async fn run(daemon: Arc<Daemon>) {
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let mut sockets: BTreeMap<String, (tokio::task::JoinHandle<()>, tokio::time::Instant)> =
        BTreeMap::new();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(TICK) => {}
            Some(channel) = rx.recv() => {
                sync(&daemon, &channel).await;
                continue;
            }
        }
        let channels = active(&daemon).await;
        sockets.retain(|id, (task, opened)| {
            let keep = channels.contains(id) && !task.is_finished() && opened.elapsed() < RENEW;
            if !keep {
                task.abort();
            }
            keep
        });
        for channel in channels {
            if sockets.contains_key(&channel) {
                continue;
            }
            let url = match daemon.client().lock().await.channel_ticket(&channel).await {
                Ok(url) => url,
                Err(_) => continue,
            };
            let tx = tx.clone();
            let id = channel.clone();
            let task = tokio::spawn(async move {
                listen(&url, &id, &tx).await;
                // A closed socket may have missed a hint.
                let _ = tx.send(id).await;
            });
            sockets.insert(channel, (task, tokio::time::Instant::now()));
        }
    }
}

/// Active channels, while unlocked; none otherwise.
async fn active(daemon: &Daemon) -> Vec<String> {
    let client = daemon.client().lock().await;
    if !client.status().is_ok_and(|s| s.state == "unlocked") {
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

/// Read hints until the socket closes.
async fn listen(url: &str, channel: &str, tx: &mpsc::Sender<String>) {
    let Ok((socket, _)) = tokio_tungstenite::connect_async(url).await else {
        return;
    };
    // Hints sent while no socket was open are lost: read once now.
    if tx.send(channel.to_string()).await.is_err() {
        return;
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
    if let Ok(report) = result {
        if report.received > 0 {
            daemon.emit(
                "channel/changed",
                json!({"id": channel, "received": report.received}),
            );
        }
    }
}
