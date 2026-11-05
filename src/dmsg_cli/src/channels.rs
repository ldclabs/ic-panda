//! Background channel control maintenance. Message bodies are fetched only for
//! a visible owner view: the open conversation, or [`catch_up`] for a list on
//! screen, never by hints alone; a view hears of a channel from the relay's
//! hint and once more after [`AGAIN`].

use crate::daemon::Daemon;
use dmsg_client::channel::Reading;
use dmsg_client::channel_log::{Status, may_rotate};
use dmsg_client::hold::Hold;
use dmsg_types::Result;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior};

/// How often the set of channels is checked.
const TICK: Duration = Duration::from_secs(1);
/// Independent channels may wait on the relay side by side.
const READERS: usize = 4;
/// Each channel is read at least this often.
const READ: Duration = Duration::from_secs(600);
/// A hinted channel is read again after this long, and its view told again,
/// for the messages the relay did not hint.
const AGAIN: Duration = Duration::from_secs(6);
/// A rotation that failed is tried again after this long, and the delivery
/// and read cursors go at most this often.
const RETRY: Duration = Duration::from_secs(30);

/// A channel this account is a member of.
struct Joined {
    id: String,
    /// It waits for a rotation this account may coordinate.
    rotate: bool,
    /// This device took or read messages past what it reported.
    report: bool,
}

/// What the loop remembers of a channel.
#[derive(Default)]
struct Seen {
    read: Option<Instant>,
    again: Option<Instant>,
    rotated: Option<Instant>,
    reported: Option<Instant>,
    /// Messages came or were read since the cursors last went.
    unreported: bool,
    running: bool,
    pending: bool,
}

/// Read the channels the relay hints at and rotate the ones that wait for
/// it, while the daemon is unlocked.
pub async fn run(daemon: Arc<Daemon>) {
    let Some(mut hints) = daemon.take_channel_hints() else {
        return;
    };
    let mut seen: BTreeMap<String, Seen> = BTreeMap::new();
    // Readers must keep polling independently while this scheduler reads
    // the channel list. A queued client-lock acquisition can already own
    // the next mutex permit before its future is polled again.
    let mut work = JoinSet::new();
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        let hinted = tokio::select! {
            _ = tick.tick() => None,
            Some(channel) = hints.recv() => Some(channel),
            Some(Ok((id, unreported, reported))) = work.join_next(), if !work.is_empty() => {
                if let Some(state) = seen.get_mut(&id) {
                    state.running = false;
                    state.unreported = unreported;
                    if let Some(at) = reported {
                        state.reported = Some(at);
                    }
                }
                None
            }
        };
        let channels = joined(&daemon).await;
        seen.retain(|id, state| state.running || channels.iter().any(|c| c.id == *id));
        for channel in &channels {
            let state = seen.entry(channel.id.clone()).or_default();
            state.unreported |= channel.report;
            if hinted
                .as_deref()
                .is_some_and(|h| h == channel.id || h == "*")
            {
                state.pending = true;
                state.again = Some(Instant::now());
            }
        }
        // Hints get a free slot before periodic maintenance. Hints arriving
        // during an in-flight read stay pending for the next read.
        let mut channels = channels;
        channels.sort_by_key(|c| !seen[&c.id].pending);
        for channel in channels {
            if work.len() >= READERS {
                break;
            }
            let state = seen.get_mut(&channel.id).expect("joined channel");
            if state.running {
                continue;
            }
            let due =
                |at: Option<Instant>, every: Duration| at.is_none_or(|at| at.elapsed() >= every);
            let again = state.again.is_some_and(|at| at.elapsed() >= AGAIN);
            let read = state.pending || due(state.read, READ) || again;
            let report_due = due(state.reported, RETRY);
            let rotate_due = channel.rotate && due(state.rotated, RETRY);
            if !read && !(state.unreported && report_due) && !rotate_due {
                continue;
            }
            if read {
                state.pending = false;
                state.read = Some(Instant::now());
                if again {
                    state.again = None;
                    daemon.emit("channel/hint", json!({"id": channel.id}));
                }
            }
            if rotate_due {
                state.rotated = Some(Instant::now());
            }
            state.running = true;
            let mut unreported = state.unreported;
            let daemon = daemon.clone();
            work.spawn(async move {
                if read {
                    unreported |= sync(&daemon, &channel.id).await;
                    unreported |= daemon
                        .client()
                        .lock()
                        .await
                        .channel_cursor_due(&channel.id)
                        .ok()
                        .flatten()
                        .is_some();
                }
                let mut reported = None;
                if unreported && report_due {
                    reported = Some(Instant::now());
                    unreported =
                        dmsg_client::channel::report_cursor(&daemon.hold(), &channel.id, None)
                            .await
                            .is_err();
                }
                if rotate_due {
                    rotate(&daemon, &channel.id).await;
                }
                if read {
                    let shared = daemon.hold();
                    if let Ok(bound) = dmsg_client::hold::Unlocked::new(&shared).await {
                        // Attachment metadata must not hold the message-read turn
                        // or the foreground RPC waiting for a receipt report.
                        if dmsg_client::channel::sync_file_deletions(&bound, &channel.id)
                            .await
                            .is_ok()
                        {
                            daemon.emit("channel/changed", json!({"id":channel.id}));
                        }
                    }
                }
                (channel.id, unreported, reported)
            });
        }
    }
}

/// The channels this account is a member of and that are not archived, or
/// that it holds and has yet to read, while unlocked; none otherwise.
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
            // Another device joined it and nothing read its log here yet.
            let Some(ledger) = c.ledger else {
                return (!c.hidden && !c.deletable_legacy_draft).then_some(Joined {
                    id: c.id,
                    rotate: false,
                    report: false,
                });
            };
            (ledger.members.contains_key(&me) && ledger.status != Status::Archived).then(|| {
                Joined {
                    rotate: ledger.status == Status::RotationRequired && may_rotate(&ledger, &me),
                    report: c.cursor_due,
                    id: c.id,
                }
            })
        })
        .collect()
}

/// Read a channel's controls and messages; a change, including what another
/// device of this account read, is announced with the count of others'
/// messages that came. Whether messages came, for the delivery cursor.
async fn sync(daemon: &Daemon, channel: &str) -> bool {
    // The log's head and status, the message cursor, the unread count and
    // whether the relay holds more.
    let state = |client: &dmsg_client::Client| {
        client
            .channel_view(channel)
            .map(|v| {
                (
                    v.ledger.map(|l| (l.head, l.status)),
                    v.message_cursor,
                    v.unread,
                    v.news,
                )
            })
            .unwrap_or_default()
    };
    let shared = daemon.hold();
    let Ok(bound) = dmsg_client::hold::Unlocked::new(&shared).await else {
        return false;
    };
    let Ok((log, cursor, unread, news)) = bound.with(|c| Ok(state(c))).await else {
        return false;
    };
    let _turn = daemon.turn(&format!("channel-read:{channel}")).await;
    let mut reading = Reading::controls(channel);
    let _ = dmsg_client::channel::read(&bound, &mut reading).await;
    let Ok((log_after, cursor_after, unread_after, news_after)) =
        bound.with(|c| Ok(state(c))).await
    else {
        return false;
    };
    if reading.received > 0
        || log_after != log
        || cursor_after > cursor
        || unread_after != unread
        || news_after != news
    {
        let view = daemon.client().lock().await.channel_view(channel).ok();
        daemon.emit(
            "channel/changed",
            json!({
                "id": channel,
                // This account's own messages from its other devices are
                // no news.
                "received": unread_after.saturating_sub(unread),
                "name": view.as_ref().map(|v| v.name.clone()),
                "muted": view.as_ref().is_some_and(|v| v.muted || v.hidden),
            }),
        );
    }
    cursor_after > cursor
}

/// A list on screen reads ahead a channel with news that is not muted: its
/// latest page, at most every [`RETRY`].
pub(crate) async fn catch_up(daemon: &Daemon, channel: &str) -> Result<serde_json::Value> {
    let shared = daemon.hold();
    let bound = dmsg_client::hold::Unlocked::new(&shared).await?;
    let view = bound.with(|c| c.channel_view(channel)).await?;
    if !view.news || view.muted || view.hidden || !daemon.read_ahead_due(channel, RETRY) {
        return Ok(json!({"received": 0}));
    }
    let _turn = daemon.turn(&format!("channel-read:{channel}")).await;
    let result = dmsg_client::channel::sync_tail(&bound, channel).await;
    // A failed later page can still have committed earlier messages.
    daemon.emit("channel/changed", json!({"id": channel}));
    Ok(json!({"received": result?.received}))
}

/// Seal a new epoch to the channel's members without blocking other channels.
async fn rotate(daemon: &Daemon, channel: &str) {
    let Some(_turn) = daemon.try_turn(&format!("channel-control:{channel}")) else {
        return;
    };
    let result = dmsg_client::channel::rotate(&daemon.hold(), channel).await;
    if let Ok(view) = result {
        daemon.emit(
            "channel/changed",
            json!({"id": channel, "epoch": view.ledger.map(|l| l.epoch)}),
        );
    }
}

/// Names are local, account-scoped display records; the daemon queues refreshes.
pub(crate) async fn names(daemon: &Daemon, accounts: &[String]) -> Result<serde_json::Value> {
    daemon.display_names(accounts).await
}
