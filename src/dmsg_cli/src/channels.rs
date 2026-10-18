//! Channel hints: while unlocked, one WebSocket per channel this account is
//! a member of says new messages or controls are available, and the daemon
//! reads them with the signed HTTP sync. A socket's authorization ends with
//! its ticket's evidence, about a minute, so each is renewed before then:
//! the new socket opens first and closes the one it replaces, so no hint
//! falls between them. A socket that opens with none before it reads once,
//! as hints sent while none was open are lost, and so does one that closes,
//! which may have missed one; the relay closes stale ones on the next
//! message. Each channel is also read every few minutes, which brings a
//! member's device revocation to the relay's notice without traffic.
//!
//! A channel that waits for a new epoch, after a member left or was
//! removed, a device was revoked or a manager rekeyed, is rotated here when
//! this account may coordinate it; the relay's lease lets one device win.

use crate::daemon::Daemon;
use dmsg_client::channel_log::{Status, may_rotate};
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
/// Each channel is read at least this often.
const READ: Duration = Duration::from_secs(600);
/// A rotation or a ticket that failed is tried again after this long.
const RETRY: Duration = Duration::from_secs(30);

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

/// A channel this account is a member of.
struct Joined {
    id: String,
    /// It waits for a rotation this account may coordinate.
    rotate: bool,
}

/// Keep a hint socket per joined channel while the daemon is unlocked, and
/// rotate the channels that wait for it.
pub async fn run(daemon: Arc<Daemon>) {
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let mut sockets: BTreeMap<String, Socket> = BTreeMap::new();
    let mut read: BTreeMap<String, Instant> = BTreeMap::new();
    let mut rotated: BTreeMap<String, Instant> = BTreeMap::new();
    // Tickets the relay refused, as to a member removed meanwhile.
    let mut refused: BTreeMap<String, Instant> = BTreeMap::new();
    // A steady tick, which reads in between do not postpone.
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            Some(channel) = rx.recv() => {
                sync(&daemon, &channel).await;
                read.insert(channel, Instant::now());
                continue;
            }
        }
        let channels = joined(&daemon).await;
        for channel in &channels {
            if channel.rotate
                && rotated
                    .get(&channel.id)
                    .is_none_or(|at| at.elapsed() >= RETRY)
            {
                rotated.insert(channel.id.clone(), Instant::now());
                rotate(&daemon, &channel.id).await;
            }
            if read.get(&channel.id).is_some_and(|at| at.elapsed() >= READ) {
                sync(&daemon, &channel.id).await;
                read.insert(channel.id.clone(), Instant::now());
            }
        }
        read.retain(|id, _| channels.iter().any(|c| c.id == *id));
        rotated.retain(|id, _| channels.iter().any(|c| c.id == *id && c.rotate));
        refused.retain(|_, at| at.elapsed() < RETRY);
        sockets.retain(|id, socket| {
            let keep = channels.iter().any(|c| c.id == *id) && !socket.task.is_finished();
            if !keep {
                socket.close();
            }
            keep
        });
        for Joined { id: channel, .. } in channels {
            let previous = match sockets.get(&channel) {
                Some(socket) if socket.opened.elapsed() < RENEW => continue,
                _ if refused.contains_key(&channel) => continue,
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
                Err(_) => {
                    refused.insert(channel, Instant::now());
                    continue;
                }
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

/// The channels this account is a member of and that are not archived,
/// while unlocked; none otherwise.
async fn joined(daemon: &Daemon) -> Vec<Joined> {
    let client = daemon.client().lock().await;
    let Some(me) = client
        .session_state()
        .unlocked_account()
        .map(str::to_string)
    else {
        return vec![];
    };
    client
        .channel_list()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            let ledger = c.ledger?;
            (ledger.members.contains_key(&me) && ledger.status != Status::Archived).then(|| {
                Joined {
                    rotate: ledger.status == Status::RotationRequired && may_rotate(&ledger, &me),
                    id: c.id,
                }
            })
        })
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
        if (hint["type"] == "available" || hint["type"] == "control")
            && tx.send(channel.to_string()).await.is_err()
        {
            return;
        }
    }
}

/// Read a channel's controls and messages; a change is announced.
async fn sync(daemon: &Daemon, channel: &str) {
    let mut client = daemon.client().lock().await;
    let state = |client: &dmsg_client::Client| {
        client
            .channel_view(channel)
            .ok()
            .and_then(|v| v.ledger)
            .map(|l| (l.head, l.status))
    };
    let before = state(&client);
    let received = client
        .channel_sync(channel, false)
        .await
        .map_or(0, |report| report.received);
    let changed = state(&client) != before;
    drop(client);
    if received > 0 || changed {
        daemon.emit(
            "channel/changed",
            json!({"id": channel, "received": received}),
        );
    }
}

/// Seal a new epoch to the channel's members.
async fn rotate(daemon: &Daemon, channel: &str) {
    let result = daemon.client().lock().await.channel_rotate(channel).await;
    if let Ok(view) = result {
        daemon.emit(
            "channel/changed",
            json!({"id": channel, "epoch": view.ledger.map(|l| l.epoch)}),
        );
    }
}
