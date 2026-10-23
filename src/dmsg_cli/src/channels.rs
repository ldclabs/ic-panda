//! The channel loop: while unlocked, the daemon reads a channel when the
//! account's hint channel names it (a message or a control landed), every
//! few minutes regardless, which brings a member's device revocation to the
//! relay's notice without traffic, and once more a few seconds after a
//! hint, as the relay hints a channel at most every five seconds. Each read
//! waits on the relay without holding the client.
//!
//! A channel that waits for a new epoch, after a member left or was
//! removed, a device was revoked or a manager rekeyed, is rotated here when
//! this account may coordinate it; the relay's lease lets one device win.
//! The delivery cursor of what this device read goes to the relay at most
//! every half minute per channel (C04). Reads of one channel may overlap,
//! such as this loop's and one after a send: each settles against what the
//! others kept.

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
/// A hinted channel is read again after this long, for the messages the
/// relay did not hint.
const AGAIN: Duration = Duration::from_secs(6);
/// A rotation that failed is tried again after this long, and a delivery
/// cursor goes at most this often.
const RETRY: Duration = Duration::from_secs(30);

/// A channel this account is a member of.
struct Joined {
    id: String,
    /// It waits for a rotation this account may coordinate.
    rotate: bool,
}

/// What the loop remembers of a channel.
#[derive(Default)]
struct Seen {
    read: Option<Instant>,
    again: Option<Instant>,
    rotated: Option<Instant>,
    reported: Option<Instant>,
    /// Messages came since the delivery cursor last went.
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
                }
                let mut reported = None;
                if unreported && report_due {
                    reported = Some(Instant::now());
                    unreported = report(&daemon, &channel.id, None).await.is_err();
                }
                if rotate_due {
                    rotate(&daemon, &channel.id).await;
                }
                (channel.id, unreported, reported)
            });
        }
    }
}

/// Report this device's delivery cursor, and the read one when given,
/// without holding the client; whether one went.
///
/// # Errors
/// Relay errors as returned.
pub(crate) async fn report(daemon: &Daemon, channel: &str, read: Option<u64>) -> Result<bool> {
    let due = daemon.client().lock().await.channel_delivery_due(channel)?;
    let delivered = match (due, read) {
        (Some(delivered), _) => delivered,
        (None, Some(_)) => 0,
        (None, None) => return Ok(false),
    };
    daemon
        .hold()
        .send(|client| client.channel_cursor_request(channel, delivered, read))
        .await?;
    daemon
        .client()
        .lock()
        .await
        .channel_cursor_settle(channel, delivered)?;
    Ok(true)
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

/// Read a channel's controls and messages; a change is announced with the
/// count of others' messages that came. Whether messages came, for the
/// delivery cursor.
async fn sync(daemon: &Daemon, channel: &str) -> bool {
    // The log's head and status, the message cursor and the unread count.
    let state = |client: &dmsg_client::Client| {
        client
            .channel_view(channel)
            .map(|v| {
                (
                    v.ledger.map(|l| (l.head, l.status)),
                    v.message_cursor,
                    v.unread,
                )
            })
            .unwrap_or_default()
    };
    let (log, cursor, unread) = state(&*daemon.client().lock().await);
    let mut reading = Reading::new(channel);
    let _ = dmsg_client::channel::read(&daemon.hold(), &mut reading).await;
    let (log_after, cursor_after, unread_after) = state(&*daemon.client().lock().await);
    if reading.received > 0 || log_after != log || cursor_after > cursor {
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

/// A found display name stays for ten minutes; a missing one is retried after
/// one minute, so a later publication is visible without a reload.
fn name_ttl(name: &Option<String>) -> Duration {
    if name.is_some() {
        Duration::from_secs(600)
    } else {
        Duration::from_secs(60)
    }
}

/// Display names of accounts, read from their certified PrincipalDocuments.
/// A failed read is not cached, so the next call asks again.
///
/// # Errors
/// Returns `Error::Locked` when the client cannot be read; failed per-account
/// reads are omitted and asked for again on a later call.
pub(crate) async fn names(daemon: &Daemon, accounts: &[String]) -> Result<serde_json::Value> {
    use std::sync::{Mutex, OnceLock};
    type Cache = Mutex<BTreeMap<String, (Option<String>, Instant)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let missing: Vec<dmsg_types::AccountId> = {
        let known = cache.lock().expect("names");
        accounts
            .iter()
            .filter(|a| {
                known
                    .get(*a)
                    .is_none_or(|(name, at)| at.elapsed() >= name_ttl(name))
            })
            .filter_map(|a| a.parse().ok())
            .take(100)
            .collect()
    };
    if !missing.is_empty() {
        let found = dmsg_client::principal::names(&daemon.hold(), &missing).await?;
        let mut known = cache.lock().expect("names");
        for (id, name) in found {
            known.insert(id, (name, Instant::now()));
        }
    }
    let known = cache.lock().expect("names");
    Ok(json!(
        accounts
            .iter()
            .filter_map(|a| known.get(a).map(|(name, _)| (a.clone(), json!(name))))
            .collect::<serde_json::Map<_, _>>()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_names_refresh_before_found_names() {
        assert_eq!(name_ttl(&Some("Panda".into())), Duration::from_secs(600));
        assert_eq!(name_ttl(&None), Duration::from_secs(60));
    }
}
