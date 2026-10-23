//! Durable, metadata-only events for mail accepted by a local connector.
//! The event and the connector snapshot commit in the same SQLite transaction.

use dmsg_types::{Error, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

/// The MCP event emitted only after verified mail is saved locally.
pub const EVENT_NAME: &str = "dmsg.mail.received";
const RETENTION_MS: i64 = 7 * 86_400_000;
const MAX_EVENTS: i64 = 10_000;

fn storage(error: rusqlite::Error) -> Error {
    Error::Unavailable(format!("mail events: {error}"))
}

pub(crate) fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mail_events (
            seq INTEGER PRIMARY KEY AUTOINCREMENT,
            agent TEXT NOT NULL, event_id TEXT NOT NULL, mailbox TEXT NOT NULL,
            routes TEXT NOT NULL, data TEXT NOT NULL, received_at INTEGER NOT NULL,
            UNIQUE(agent, event_id));
         CREATE INDEX IF NOT EXISTS mail_events_agent ON mail_events(agent,seq);
         CREATE TABLE IF NOT EXISTS mail_event_heads (
            agent TEXT PRIMARY KEY, head INTEGER NOT NULL, floor INTEGER NOT NULL DEFAULT 0);",
    )
    .map_err(storage)
}

fn key(item: &Value) -> Option<(String, String)> {
    (item["direction"] == "received").then_some(())?;
    Some((
        item["message"]["from"].as_str()?.into(),
        item["message"]["message_id"].as_str()?.into(),
    ))
}

/// Called inside the snapshot's transaction. Known messages never become
/// new events merely because their read state changed or history was pruned.
pub(crate) fn record(
    conn: &Connection,
    agent: &str,
    before: &Value,
    after: &Value,
    now: i64,
) -> Result<()> {
    let known: BTreeSet<_> = before["mail"]["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(key)
        .collect();
    for item in after["mail"]["messages"].as_array().into_iter().flatten() {
        let Some((from, id)) = key(item) else {
            continue;
        };
        if known.contains(&(from.clone(), id.clone())) {
            continue;
        }
        let mailbox = item["mailbox_id"].as_str().ok_or(Error::IntegrityFailed)?;
        let routes: BTreeSet<String> = after["mail"]["mailboxes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["mailbox_id"] == mailbox)
            .flat_map(|b| b["cards"].as_array().into_iter().flatten())
            .flat_map(|c| {
                c["event"]["payload"]["routes"]
                    .as_array()
                    .into_iter()
                    .flatten()
            })
            .filter_map(|r| r.as_str().map(str::to_owned))
            .collect();
        let data = json!({"mailbox_id":mailbox,"from":from,"message_id":id,"thread_id":item["message"]["thread_id"]});
        let event_id = format!("mail:{agent}:{from}:{id}");
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO mail_events(agent,event_id,mailbox,routes,data,received_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![agent,event_id,mailbox,serde_json::to_string(&routes).expect("routes"),data.to_string(),now],
        ).map_err(storage)?;
        if inserted > 0 {
            let seq = conn.last_insert_rowid();
            conn.execute("INSERT INTO mail_event_heads(agent,head) VALUES (?1,?2) ON CONFLICT(agent) DO UPDATE SET head=excluded.head",params![agent,seq]).map_err(storage)?;
        }
    }
    prune(conn, agent, now)
}

fn prune(conn: &Connection, agent: &str, now: i64) -> Result<()> {
    let keep: Option<i64> = conn
        .query_row(
            "SELECT seq FROM mail_events WHERE agent=?1 ORDER BY seq DESC LIMIT 1 OFFSET ?2",
            params![agent, MAX_EVENTS - 1],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let floor: Option<i64> = conn
        .query_row(
            "SELECT MAX(seq) FROM mail_events WHERE agent=?1 AND (received_at<?2 OR seq<?3)",
            params![agent, now.saturating_sub(RETENTION_MS), keep.unwrap_or(0)],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if let Some(floor) = floor {
        conn.execute(
            "UPDATE mail_event_heads SET floor=MAX(floor,?2) WHERE agent=?1",
            params![agent, floor],
        )
        .map_err(storage)?;
        conn.execute(
            "DELETE FROM mail_events WHERE agent=?1 AND seq<=?2",
            params![agent, floor],
        )
        .map_err(storage)?;
    }
    Ok(())
}

/// A query is scoped to the current Agent ID and its currently granted origins.
pub struct Query<'a> {
    /// Authenticated Agent ID, never selected by an event payload.
    pub agent: &'a str,
    /// Relay origins the Agent may currently read.
    pub reach: &'a [String],
    /// Optional mailbox filter.
    pub mailbox: Option<&'a str>,
    /// This Agent's opaque event cursor. None starts at the current head.
    pub cursor: Option<&'a str>,
    /// At most 100 events per page.
    pub limit: usize,
    /// Optional replay floor, in Unix milliseconds.
    pub since: Option<i64>,
    /// Current time, read once by the caller.
    pub now: i64,
}

/// Read a stable page without marking mail read or consuming inbox work.
///
/// # Errors
/// Refuses a cursor of another Agent or event family, malformed bounds,
/// and empty authorization. Storage errors remain errors, never empty success.
pub fn page(path: &Path, query: Query<'_>) -> Result<Value> {
    let conn = crate::runtime::open(path)?;
    page_in(&conn, query)
}

fn page_in(conn: &Connection, q: Query<'_>) -> Result<Value> {
    ensure(!q.reach.is_empty(), Error::Forbidden)?;
    ensure(
        q.limit > 0 && q.limit <= 100,
        Error::InvalidInput("max".into()),
    )?;
    let tx = conn.unchecked_transaction().map_err(storage)?;
    prune(&tx, q.agent, q.now)?;
    let (head, floor) = tx
        .query_row(
            "SELECT head,floor FROM mail_event_heads WHERE agent=?1",
            [q.agent],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(storage)?
        .unwrap_or((0, 0));
    let prefix = format!("mail:{}:{}:", q.agent, q.mailbox.unwrap_or("*"));
    let after = match q.cursor {
        None => head,
        Some(cursor) => cursor
            .strip_prefix(&prefix)
            .and_then(|s| {
                (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| s.parse::<i64>().ok())
                    .flatten()
            })
            .filter(|n| *n <= head)
            .ok_or_else(|| Error::InvalidInput("mail event cursor".into()))?,
    };
    let age_floor: Option<i64> = tx
        .query_row(
            "SELECT MAX(seq) FROM mail_events WHERE agent=?1 AND received_at<?2",
            params![q.agent, q.since.unwrap_or(0)],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let floor = floor.max(age_floor.unwrap_or(0));
    let truncated = q.cursor.is_some() && after < floor;
    let after = after.max(floor);
    let mut statement = tx.prepare("SELECT seq,event_id,data,received_at FROM mail_events e WHERE agent=?1 AND seq>?2 AND seq<=?3 AND (?4 IS NULL OR mailbox=?4) AND EXISTS (SELECT 1 FROM json_each(e.routes) r JOIN json_each(?5) a ON r.value=a.value) ORDER BY seq LIMIT ?6").map_err(storage)?;
    let rows = statement
        .query_map(
            params![
                q.agent,
                after,
                head,
                q.mailbox,
                serde_json::to_string(q.reach).expect("reach"),
                q.limit as i64 + 1
            ],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        )
        .map_err(storage)?;
    let mut rows = rows
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    let more = rows.len() > q.limit;
    rows.truncate(q.limit);
    let cursor = if more {
        rows.last().expect("page").0
    } else {
        head
    };
    let events = rows.into_iter().map(|(seq,id,data,at)| Ok(json!({"eventId":id,"name":EVENT_NAME,"received_at":at,"data":serde_json::from_str::<Value>(&data).map_err(|_|Error::IntegrityFailed)?,"cursor":format!("{prefix}{seq}")}))).collect::<Result<Vec<_>>>()?;
    drop(statement);
    tx.commit().map_err(storage)?;
    Ok(
        json!({"events":events,"cursor":format!("{prefix}{cursor}"),"hasMore":more,"truncated":truncated,"nextPollMs":30000}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(id: &str) -> Value {
        json!({"mail":{"mailboxes":[{"mailbox_id":"box","cards":[{"event":{"payload":{"routes":["https://mail.example"]}}}]}],"messages":[{"direction":"received","mailbox_id":"box","message":{"from":"sender","message_id":id,"thread_id":"thread","parts":[{"text":"private"}]}}]}})
    }

    #[test]
    fn events_commit_with_snapshot_and_replay_without_contents_or_duplicates() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let reach = vec!["https://mail.example".to_owned()];
        let query = |cursor| Query {
            agent: "agent",
            reach: &reach,
            mailbox: None,
            cursor,
            limit: 100,
            since: None,
            now: 100,
        };
        let start = page_in(&conn, query(None)).unwrap()["cursor"]
            .as_str()
            .unwrap()
            .to_owned();
        let tx = conn.unchecked_transaction().unwrap();
        record(&tx, "agent", &Value::Null, &snapshot("one"), 100).unwrap();
        tx.rollback().unwrap();
        assert!(
            page_in(&conn, query(Some(&start))).unwrap()["events"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let tx = conn.unchecked_transaction().unwrap();
        record(&tx, "agent", &Value::Null, &snapshot("one"), 100).unwrap();
        tx.commit().unwrap();
        record(&conn, "agent", &snapshot("one"), &snapshot("one"), 100).unwrap();
        let page = page_in(&conn, query(Some(&start))).unwrap();
        assert_eq!(page["events"].as_array().unwrap().len(), 1);
        assert!(!page.to_string().contains("private"));
        assert!(page_in(&conn, query(Some("mail:other:0"))).is_err());
        let mut filtered = query(Some(&start));
        filtered.mailbox = Some("box");
        assert!(page_in(&conn, filtered).is_err());
        let other_relay = vec!["https://other.example".into()];
        let mut outside_scope = query(Some(&start));
        outside_scope.reach = &other_relay;
        assert!(
            page_in(&conn, outside_scope).unwrap()["events"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let mut denied = query(Some(&start));
        denied.reach = &[];
        assert_eq!(page_in(&conn, denied), Err(Error::Forbidden));
        let mut expired = query(Some(&start));
        expired.now = RETENTION_MS + 101;
        let lost = page_in(&conn, expired).unwrap();
        assert_eq!(lost["truncated"], true);
        assert!(lost["events"].as_array().unwrap().is_empty());
    }
}
