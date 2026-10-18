//! SQLite storage of one workspace. The store holds plaintext metadata,
//! records the caller has already sealed, encrypted content objects with
//! their ciphertext chunks, and sync jobs and the file library's backup
//! jobs, made only of ciphertext and signatures; it never sees a key.
//!
//! A store has a single owner, the client, which serializes its use; each
//! statement takes the connection lock, so [`Store::atomic`] groups the
//! owner's writes but does not isolate them from the owner's own reads.

use dmsg_types::{Error, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS sealed (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS objects (
  key TEXT PRIMARY KEY,
  item TEXT NOT NULL,
  revision TEXT NOT NULL,
  kind TEXT NOT NULL,
  frame BLOB NOT NULL,
  state TEXT NOT NULL,
  conflict INTEGER NOT NULL,
  resolved INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS objects_state ON objects (state, created_at);
CREATE INDEX IF NOT EXISTS objects_item ON objects (item);
CREATE TABLE IF NOT EXISTS heads (
  item TEXT PRIMARY KEY,
  revision TEXT NOT NULL,
  kind TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS chunks (digest TEXT PRIMARY KEY, bytes BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS jobs (key TEXT PRIMARY KEY, value BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS library_jobs (key TEXT PRIMARY KEY, value BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS approvals (
  id TEXT PRIMARY KEY,
  state TEXT NOT NULL,
  class TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS approvals_state ON approvals (state, created_at);
CREATE TABLE IF NOT EXISTS connect_inbox (
  request_id TEXT PRIMARY KEY,
  seq INTEGER NOT NULL,
  value BLOB NOT NULL,
  done INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS mcp_events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  endpoint TEXT NOT NULL,
  webhook_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  received_at INTEGER NOT NULL,
  value TEXT NOT NULL,
  UNIQUE (endpoint, webhook_id)
);
CREATE INDEX IF NOT EXISTS mcp_events_received ON mcp_events (received_at);
CREATE TABLE IF NOT EXISTS audit (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  at INTEGER NOT NULL,
  actor TEXT NOT NULL,
  via TEXT NOT NULL,
  action TEXT NOT NULL,
  target TEXT,
  detail TEXT,
  outcome TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS audit_actor ON audit (actor, seq);
CREATE INDEX IF NOT EXISTS audit_at ON audit (at);
";

/// One audit entry: who did what, when and through which way in.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct AuditRow {
    /// Local sequence; never reused.
    pub seq: u64,
    /// Unix milliseconds.
    pub at: u64,
    /// `owner`, or the `did:agent:` of a hosted agent.
    pub actor: String,
    /// `cli`, `gui` or `mcp`.
    pub via: String,
    /// The method or tool.
    pub action: String,
    /// What it acted on: an ID, never content.
    pub target: Option<String>,
    /// A short note, such as the capabilities of a grant.
    pub detail: Option<String>,
    /// `ok`, or the error code.
    pub outcome: String,
}

/// A relayed MCP event as stored: routing columns and the sealed payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpEventRow {
    /// Local sequence; never reused.
    pub seq: u64,
    /// The endpoint ID.
    pub endpoint: String,
    /// The delivery's `webhook-id`, or the name of a local record.
    pub webhook_id: String,
    /// `event`, `verification`, `gap`, `terminated`, `relay_gap` or `refresh_due`.
    pub kind: String,
    /// When the relay received it, in Unix milliseconds.
    pub received_at: u64,
    /// The payload sealed under the LDK.
    pub value: String,
}

/// A local version the relay has not accepted yet.
pub const LOCAL: &str = "local";
/// The relay accepted the version, as head or as a conflict copy.
pub const SYNCED: &str = "synced";
/// A local version the relay refused against its head; it stays a conflict
/// copy on this device and is never sent.
pub const BLOCKED: &str = "blocked";
/// A local version resealed under a newer root; its replacement is sent.
pub const REPLACED: &str = "replaced";

/// The plaintext index row of an approval request; its content is sealed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRow {
    /// Request ID, hex.
    pub id: String,
    /// The request's state.
    pub state: String,
    /// What kind of source asked, such as `cli` or `connect`; never an origin.
    pub class: String,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
    /// Deadline in Unix milliseconds.
    pub expires_at: u64,
}

/// One workspace database.
pub struct Store {
    conn: Mutex<Connection>,
}

/// An encrypted object version with its routing columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectRow {
    /// `item:revision`.
    pub key: String,
    /// Item ID.
    pub item: String,
    /// Revision ID.
    pub revision: String,
    /// Object kind, such as `vault`.
    pub kind: String,
    /// Canonical CBOR of the encrypted record.
    pub frame: Vec<u8>,
    /// [`LOCAL`], [`SYNCED`], [`BLOCKED`] or [`REPLACED`].
    pub state: String,
    /// A conflict copy beside the item's head.
    pub conflict: bool,
    /// A newer version resolved this conflict copy.
    pub resolved: bool,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
}

fn storage(error: rusqlite::Error) -> Error {
    Error::Unavailable(format!("storage: {error}"))
}

fn cbor<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    cbor2::from_slice(bytes).map_err(|_| Error::Unavailable("corrupt record".into()))
}

/// The least key above every key starting with `prefix`, the end of its
/// range; the prefixes are ASCII.
fn prefix_end(prefix: &str) -> String {
    let mut end = prefix.to_string();
    let last = end.pop().expect("a prefix");
    end.push(char::from_u32(u32::from(last) + 1).expect("an ASCII prefix"));
    end
}

/// `?, ?, …` for `n` values.
fn marks(n: usize) -> String {
    vec!["?"; n].join(", ")
}

impl Store {
    /// Open or create the database at `path`.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path).map_err(storage)?)
    }

    /// A private in-memory database.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn memory() -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(storage)?)
    }

    fn init(conn: Connection) -> Result<Self> {
        // Collected chunks give their pages back to the file system; this
        // takes effect only in a database created with it, so before WAL,
        // whose switch writes the new file's header.
        conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")
            .map_err(storage)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(storage)?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(storage)?;
        // Deleted pages are zeroed, so a removed record leaves no ciphertext
        // or metadata behind in the file.
        conn.pragma_update(None, "secure_delete", "ON")
            .map_err(storage)?;
        conn.execute_batch(SCHEMA).map_err(storage)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run `f` in one transaction; an error rolls everything back.
    ///
    /// # Errors
    /// Returns the error of `f`, or `Error::Unavailable` when SQLite fails.
    pub fn atomic<R>(&self, f: impl FnOnce(&Self) -> Result<R>) -> Result<R> {
        self.conn()
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(storage)?;
        match f(self) {
            Ok(result) => {
                self.conn().execute_batch("COMMIT").map_err(storage)?;
                Ok(result)
            }
            Err(error) => {
                let _ = self.conn().execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    fn get<T: DeserializeOwned>(&self, table: &str, key: &str) -> Result<Option<T>> {
        let bytes: Option<Vec<u8>> = self
            .conn()
            .query_row(
                &format!("SELECT value FROM {table} WHERE key = ?1"),
                [key],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        bytes.map(|b| cbor(&b)).transpose()
    }

    fn set<T: Serialize>(&self, table: &str, key: &str, value: &T) -> Result<()> {
        self.conn()
            .execute(
                &format!("INSERT OR REPLACE INTO {table} (key, value) VALUES (?1, ?2)"),
                params![key, crate::codec::canonical(value)],
            )
            .map(|_| ())
            .map_err(storage)
    }

    fn delete(&self, table: &str, key: &str) -> Result<()> {
        self.conn()
            .execute(&format!("DELETE FROM {table} WHERE key = ?1"), [key])
            .map(|_| ())
            .map_err(storage)
    }

    /// A plaintext metadata value.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails or the value is corrupt.
    pub fn meta<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.get("meta", key)
    }

    /// Store a plaintext metadata value. Never pass secrets.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn set_meta<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.set("meta", key, value)
    }

    /// Remove a metadata value.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn delete_meta(&self, key: &str) -> Result<()> {
        self.delete("meta", key)
    }

    /// A sync job: ciphertext, signatures and progress, never a key.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails or the job is corrupt.
    pub fn job<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.get("jobs", key)
    }

    /// Store a sync job.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn set_job<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.set("jobs", key, value)
    }

    /// Remove a sync job.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn delete_job(&self, key: &str) -> Result<()> {
        self.delete("jobs", key)
    }

    /// Every sync job, by key.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails or a job is corrupt.
    pub fn jobs<T: DeserializeOwned>(&self) -> Result<Vec<(String, T)>> {
        self.rows("jobs")
    }

    /// A job of the file library, a backup in flight; kept apart from the
    /// sync jobs, whose listing reads every row as a sync job.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails or the job is corrupt.
    pub fn library_job<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.get("library_jobs", key)
    }

    /// Store a job of the file library.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn set_library_job<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.set("library_jobs", key, value)
    }

    /// Remove a job of the file library.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn delete_library_job(&self, key: &str) -> Result<()> {
        self.delete("library_jobs", key)
    }

    /// Every job of the file library, by key.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails or a job is corrupt.
    pub fn library_jobs<T: DeserializeOwned>(&self) -> Result<Vec<(String, T)>> {
        self.rows("library_jobs")
    }

    fn rows<T: DeserializeOwned>(&self, table: &str) -> Result<Vec<(String, T)>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(&format!("SELECT key, value FROM {table} ORDER BY key"))
            .map_err(storage)?;
        let rows = statement
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(storage)?;
        rows.map(|row| {
            let (key, bytes) = row.map_err(storage)?;
            Ok((key, cbor(&bytes)?))
        })
        .collect()
    }

    /// A sealed record, as the caller stored it.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn sealed(&self, key: &str) -> Result<Option<String>> {
        self.conn()
            .query_row("SELECT value FROM sealed WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(storage)
    }

    /// Keys of the sealed records starting with `prefix`, in order; a range
    /// of the primary key.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn sealed_keys(&self, prefix: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT key FROM sealed WHERE key >= ?1 AND key < ?2 ORDER BY key")
            .map_err(storage)?;
        let rows = statement
            .query_map(params![prefix, prefix_end(prefix)], |r| r.get(0))
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// At most `limit` sealed records starting with `prefix` whose keys sort
    /// after `after`, in order, with their values.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn sealed_after(
        &self,
        prefix: &str,
        after: &str,
        limit: usize,
    ) -> Result<Vec<(String, String)>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT key, value FROM sealed WHERE key > ?1 AND key >= ?2 AND key < ?3 \
                 ORDER BY key LIMIT ?4",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(
                params![after, prefix, prefix_end(prefix), limit as i64],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// At most `limit` sealed records starting with `prefix` whose keys sort
    /// before `before`, the last first, with their values.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn sealed_before(
        &self,
        prefix: &str,
        before: &str,
        limit: usize,
    ) -> Result<Vec<(String, String)>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT key, value FROM sealed WHERE key < ?1 AND key >= ?2 AND key < ?3 \
                 ORDER BY key DESC LIMIT ?4",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(
                params![before, prefix, prefix_end(prefix), limit as i64],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// Store a sealed record.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn set_sealed(&self, key: &str, value: &str) -> Result<()> {
        self.conn()
            .execute(
                "INSERT OR REPLACE INTO sealed (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .map(|_| ())
            .map_err(storage)
    }

    /// Remove a sealed record.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn delete_sealed(&self, key: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM sealed WHERE key = ?1", [key])
            .map(|_| ())
            .map_err(storage)
    }

    /// Insert or update an approval's index row.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn put_approval(&self, row: &ApprovalRow) -> Result<()> {
        self.conn()
            .execute(
                "INSERT OR REPLACE INTO approvals (id, state, class, created_at, expires_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    row.id,
                    row.state,
                    row.class,
                    row.created_at as i64,
                    row.expires_at as i64
                ],
            )
            .map(|_| ())
            .map_err(storage)
    }

    /// Approval index rows in `states`, oldest first; every row when empty.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn approvals(&self, states: &[&str]) -> Result<Vec<ApprovalRow>> {
        let filter = match states.len() {
            0 => String::new(),
            n => format!("WHERE state IN ({})", marks(n)),
        };
        let conn = self.conn();
        let mut statement = conn
            .prepare(&format!(
                "SELECT id, state, class, created_at, expires_at FROM approvals {filter} \
                 ORDER BY created_at, id"
            ))
            .map_err(storage)?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(states), |r| {
                Ok(ApprovalRow {
                    id: r.get(0)?,
                    state: r.get(1)?,
                    class: r.get(2)?,
                    created_at: r.get::<_, i64>(3)? as u64,
                    expires_at: r.get::<_, i64>(4)? as u64,
                })
            })
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// How many approvals in `states` have a deadline after `after`.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn count_approvals(&self, states: &[&str], after: u64) -> Result<u64> {
        let mut values: Vec<rusqlite::types::Value> =
            states.iter().map(|s| s.to_string().into()).collect();
        values.push((after.min(i64::MAX as u64) as i64).into());
        self.conn()
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM approvals WHERE state IN ({}) AND expires_at > ?",
                    marks(states.len())
                ),
                rusqlite::params_from_iter(values),
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(storage)
    }

    /// Delete the approvals in `states` created before `before`; returns
    /// their IDs.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn prune_approvals(&self, states: &[&str], before: u64) -> Result<Vec<String>> {
        let mut values: Vec<rusqlite::types::Value> =
            states.iter().map(|s| s.to_string().into()).collect();
        values.push((before.min(i64::MAX as u64) as i64).into());
        let conn = self.conn();
        let mut statement = conn
            .prepare(&format!(
                "DELETE FROM approvals WHERE state IN ({}) AND created_at < ? RETURNING id",
                marks(states.len())
            ))
            .map_err(storage)?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(values), |r| r.get(0))
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// Store or replace a relay mailbox entry; `done` once it is handled.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn put_inbox<T: Serialize>(
        &self,
        request: &str,
        seq: u64,
        value: &T,
        done: bool,
    ) -> Result<()> {
        self.conn()
            .execute(
                "INSERT OR REPLACE INTO connect_inbox (request_id, seq, value, done) VALUES (?1, ?2, ?3, ?4)",
                params![request, seq as i64, crate::codec::canonical(value), done],
            )
            .map(|_| ())
            .map_err(storage)
    }

    /// A relay mailbox entry and whether it is handled.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails or the entry is corrupt.
    pub fn inbox<T: DeserializeOwned>(&self, request: &str) -> Result<Option<(T, bool)>> {
        let row: Option<(Vec<u8>, bool)> = self
            .conn()
            .query_row(
                "SELECT value, done FROM connect_inbox WHERE request_id = ?1",
                [request],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage)?;
        row.map(|(bytes, done)| Ok((cbor(&bytes)?, done)))
            .transpose()
    }

    /// Remove a relay mailbox entry.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn delete_inbox(&self, request: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM connect_inbox WHERE request_id = ?1", [request])
            .map(|_| ())
            .map_err(storage)
    }

    /// How many relay mailbox entries are not handled yet.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn count_inbox_pending(&self) -> Result<u64> {
        self.conn()
            .query_row(
                "SELECT COUNT(*) FROM connect_inbox WHERE done = 0",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(storage)
    }

    /// Relay mailbox entries in sequence order; only unhandled ones with `pending`.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails or an entry is corrupt.
    pub fn inbox_entries<T: DeserializeOwned>(&self, pending: bool) -> Result<Vec<T>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT value FROM connect_inbox WHERE done = 0 OR ?1 = 0 ORDER BY seq")
            .map_err(storage)?;
        let rows = statement
            .query_map([pending], |r| r.get::<_, Vec<u8>>(0))
            .map_err(storage)?;
        rows.map(|row| cbor(&row.map_err(storage)?)).collect()
    }

    /// Store a relayed MCP event once per endpoint and `webhook_id`; false
    /// when it was already there.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn put_mcp_event(
        &self,
        endpoint: &str,
        webhook_id: &str,
        kind: &str,
        received_at: u64,
        value: &str,
    ) -> Result<bool> {
        self.conn()
            .execute(
                "INSERT OR IGNORE INTO mcp_events (endpoint, webhook_id, kind, received_at, value) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![endpoint, webhook_id, kind, received_at as i64, value],
            )
            .map(|n| n == 1)
            .map_err(storage)
    }

    /// Whether an endpoint's `webhook_id` is stored.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn has_mcp_event(&self, endpoint: &str, webhook_id: &str) -> Result<bool> {
        self.conn()
            .query_row(
                "SELECT 1 FROM mcp_events WHERE endpoint = ?1 AND webhook_id = ?2",
                [endpoint, webhook_id],
                |_| Ok(()),
            )
            .optional()
            .map(|row| row.is_some())
            .map_err(storage)
    }

    /// How many MCP events of `endpoint` are stored.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn count_mcp_events(&self, endpoint: &str) -> Result<u64> {
        self.conn()
            .query_row(
                "SELECT COUNT(*) FROM mcp_events WHERE endpoint = ?1",
                [endpoint],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(storage)
    }

    /// The MCP events of `endpoints` after `after`, oldest first.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn mcp_events(
        &self,
        endpoints: &[String],
        after: u64,
        limit: usize,
    ) -> Result<Vec<McpEventRow>> {
        if endpoints.is_empty() {
            return Ok(Vec::new());
        }
        let marks = marks(endpoints.len());
        let conn = self.conn();
        let mut statement = conn
            .prepare(&format!(
                "SELECT seq, endpoint, webhook_id, kind, received_at, value FROM mcp_events WHERE seq > ? AND endpoint IN ({marks}) ORDER BY seq LIMIT ?"
            ))
            .map_err(storage)?;
        let mut values: Vec<rusqlite::types::Value> = vec![(after as i64).into()];
        values.extend(endpoints.iter().map(|e| e.clone().into()));
        values.push((limit as i64).into());
        let rows = statement
            .query_map(rusqlite::params_from_iter(values), |r| {
                Ok(McpEventRow {
                    seq: r.get::<_, i64>(0)? as u64,
                    endpoint: r.get(1)?,
                    webhook_id: r.get(2)?,
                    kind: r.get(3)?,
                    received_at: r.get::<_, i64>(4)? as u64,
                    value: r.get(5)?,
                })
            })
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// Delete the MCP events of `endpoints` up to `through`; returns how many.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn delete_mcp_events(&self, endpoints: &[String], through: u64) -> Result<u64> {
        if endpoints.is_empty() {
            return Ok(0);
        }
        let marks = marks(endpoints.len());
        let mut values: Vec<rusqlite::types::Value> = vec![(through as i64).into()];
        values.extend(endpoints.iter().map(|e| e.clone().into()));
        self.conn()
            .execute(
                &format!("DELETE FROM mcp_events WHERE seq <= ? AND endpoint IN ({marks})"),
                rusqlite::params_from_iter(values),
            )
            .map(|n| n as u64)
            .map_err(storage)
    }

    /// Keep the MCP events received at or after `since`, at most `keep` of
    /// the newest.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn prune_mcp_events(&self, since: u64, keep: u64) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM mcp_events WHERE received_at < ?1",
            [since as i64],
        )
        .map_err(storage)?;
        conn.execute(
            "DELETE FROM mcp_events WHERE seq <= \
             (SELECT seq FROM mcp_events ORDER BY seq DESC LIMIT 1 OFFSET ?1)",
            [keep as i64],
        )
        .map(|_| ())
        .map_err(storage)
    }

    /// Append an audit entry, keeping those at or after `since` and at most
    /// `keep` of the newest; returns its sequence.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn put_audit(&self, row: &AuditRow, since: u64, keep: u64) -> Result<u64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO audit (at, actor, via, action, target, detail, outcome) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                row.at as i64,
                row.actor,
                row.via,
                row.action,
                row.target,
                row.detail,
                row.outcome
            ],
        )
        .map_err(storage)?;
        let seq = conn.last_insert_rowid() as u64;
        conn.execute("DELETE FROM audit WHERE at < ?1", [since as i64])
            .map_err(storage)?;
        // Sequences grow by one and are never reused, so the newest `keep`
        // are the ones above this bound.
        conn.execute(
            "DELETE FROM audit WHERE seq <= ?1",
            [seq.saturating_sub(keep) as i64],
        )
        .map_err(storage)?;
        Ok(seq)
    }

    /// Audit entries before `before`, newest first, of `actor` when given.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn audit(&self, before: u64, actor: Option<&str>, limit: usize) -> Result<Vec<AuditRow>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT seq, at, actor, via, action, target, detail, outcome FROM audit
                 WHERE seq < ?1 AND (?2 IS NULL OR actor = ?2) ORDER BY seq DESC LIMIT ?3",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(
                params![before.min(i64::MAX as u64) as i64, actor, limit as i64],
                |r| {
                    Ok(AuditRow {
                        seq: r.get::<_, i64>(0)? as u64,
                        at: r.get::<_, i64>(1)? as u64,
                        actor: r.get(2)?,
                        via: r.get(3)?,
                        action: r.get(4)?,
                        target: r.get(5)?,
                        detail: r.get(6)?,
                        outcome: r.get(7)?,
                    })
                },
            )
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// Insert an object version; an existing key must hold the same frame.
    ///
    /// # Errors
    /// A different frame under the same key returns `Error::IdempotencyConflict`.
    pub fn put_object(&self, row: &ObjectRow) -> Result<()> {
        if let Some(existing) = self.object(&row.key)? {
            return ensure(existing.frame == row.frame, Error::IdempotencyConflict);
        }
        self.conn()
            .execute(
                "INSERT INTO objects
                 (key, item, revision, kind, frame, state, conflict, resolved, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    row.key,
                    row.item,
                    row.revision,
                    row.kind,
                    row.frame,
                    row.state,
                    row.conflict,
                    row.resolved,
                    row.created_at as i64
                ],
            )
            .map(|_| ())
            .map_err(storage)
    }

    /// One object version by `item:revision`.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn object(&self, key: &str) -> Result<Option<ObjectRow>> {
        self.conn()
            .query_row(
                "SELECT key, item, revision, kind, frame, state, conflict, resolved, created_at
                 FROM objects WHERE key = ?1",
                [key],
                row,
            )
            .optional()
            .map_err(storage)
    }

    fn select(&self, filter: &str, value: &str) -> Result<Vec<ObjectRow>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(&format!(
                "SELECT key, item, revision, kind, frame, state, conflict, resolved, created_at
                 FROM objects WHERE {filter} ORDER BY created_at, key"
            ))
            .map_err(storage)?;
        let rows = statement.query_map([value], row).map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// Object versions in one state, oldest first.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn objects_in(&self, state: &str) -> Result<Vec<ObjectRow>> {
        self.select("state = ?1", state)
    }

    /// Conflict copies of one kind that are neither resolved nor replaced.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn conflicts(&self, kind: &str) -> Result<Vec<ObjectRow>> {
        self.select(
            "conflict = 1 AND resolved = 0 AND state != 'replaced' AND kind = ?1",
            kind,
        )
    }

    /// Every stored version of one item, oldest first.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn versions(&self, item: &str) -> Result<Vec<ObjectRow>> {
        self.select("item = ?1", item)
    }

    /// How many [`Self::conflicts`] there are.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn count_conflicts(&self, kind: &str) -> Result<u64> {
        self.conn()
            .query_row(
                "SELECT COUNT(*) FROM objects
                 WHERE conflict = 1 AND resolved = 0 AND state != 'replaced' AND kind = ?1",
                [kind],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(storage)
    }

    /// Record that a newer version resolved this conflict copy.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn set_resolved(&self, key: &str) -> Result<()> {
        self.conn()
            .execute("UPDATE objects SET resolved = 1 WHERE key = ?1", [key])
            .map(|_| ())
            .map_err(storage)
    }

    /// Number of object versions in one state.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn count_in(&self, state: &str) -> Result<u64> {
        self.conn()
            .query_row(
                "SELECT COUNT(*) FROM objects WHERE state = ?1",
                [state],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(storage)
    }

    /// Set an object version's sync state and conflict flag.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn set_object(&self, key: &str, state: &str, conflict: bool) -> Result<()> {
        self.conn()
            .execute(
                "UPDATE objects SET state = ?2, conflict = ?3 WHERE key = ?1",
                params![key, state, conflict],
            )
            .map(|_| ())
            .map_err(storage)
    }

    /// Make `revision` the current version of an item.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn set_head(&self, item: &str, revision: &str, kind: &str) -> Result<()> {
        self.conn()
            .execute(
                "INSERT OR REPLACE INTO heads (item, revision, kind) VALUES (?1, ?2, ?3)",
                params![item, revision, kind],
            )
            .map(|_| ())
            .map_err(storage)
    }

    /// Forget an item's head.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn delete_head(&self, item: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM heads WHERE item = ?1", [item])
            .map(|_| ())
            .map_err(storage)
    }

    /// The current revision of an item.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn head(&self, item: &str) -> Result<Option<String>> {
        self.conn()
            .query_row("SELECT revision FROM heads WHERE item = ?1", [item], |r| {
                r.get(0)
            })
            .optional()
            .map_err(storage)
    }

    /// Current `(item, revision)` pairs of one kind, by item.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn heads(&self, kind: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT item, revision FROM heads WHERE kind = ?1 ORDER BY item")
            .map_err(storage)?;
        let rows = statement
            .query_map([kind], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// Store a ciphertext chunk under the SHA-256 of its bytes.
    ///
    /// # Errors
    /// A digest that does not match returns `Error::IntegrityFailed`.
    pub fn put_chunk(&self, digest: &str, bytes: &[u8]) -> Result<()> {
        ensure(
            hex::encode(*crate::codec::sha256(bytes)) == digest,
            Error::IntegrityFailed,
        )?;
        self.conn()
            .execute(
                "INSERT OR IGNORE INTO chunks (digest, bytes) VALUES (?1, ?2)",
                params![digest, bytes],
            )
            .map(|_| ())
            .map_err(storage)
    }

    /// A ciphertext chunk, checked against its digest.
    ///
    /// # Errors
    /// Stored bytes that no longer match return `Error::IntegrityFailed`.
    pub fn chunk(&self, digest: &str) -> Result<Option<Vec<u8>>> {
        let bytes: Option<Vec<u8>> = self
            .conn()
            .query_row(
                "SELECT bytes FROM chunks WHERE digest = ?1",
                [digest],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        if let Some(bytes) = &bytes {
            ensure(
                hex::encode(*crate::codec::sha256(bytes)) == digest,
                Error::IntegrityFailed,
            )?;
        }
        Ok(bytes)
    }

    /// Whether a chunk is stored.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn has_chunk(&self, digest: &str) -> Result<bool> {
        self.conn()
            .query_row("SELECT 1 FROM chunks WHERE digest = ?1", [digest], |_| {
                Ok(())
            })
            .optional()
            .map(|r| r.is_some())
            .map_err(storage)
    }

    /// The digests of every stored chunk.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn chunk_digests(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT digest FROM chunks").map_err(storage)?;
        let rows = statement.query_map([], |r| r.get(0)).map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// Delete chunks in one transaction, then give the freed pages back.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn delete_chunks(&self, digests: &[String]) -> Result<()> {
        self.atomic(|store| {
            for digest in digests {
                store
                    .conn()
                    .execute("DELETE FROM chunks WHERE digest = ?1", [digest])
                    .map_err(storage)?;
            }
            Ok(())
        })?;
        // Each step frees one page and returns a row: step to the end.
        let conn = self.conn();
        let mut statement = conn.prepare("PRAGMA incremental_vacuum").map_err(storage)?;
        let mut rows = statement.query([]).map_err(storage)?;
        while rows.next().map_err(storage)?.is_some() {}
        Ok(())
    }
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ObjectRow> {
    Ok(ObjectRow {
        key: r.get(0)?,
        item: r.get(1)?,
        revision: r.get(2)?,
        kind: r.get(3)?,
        frame: r.get(4)?,
        state: r.get(5)?,
        conflict: r.get(6)?,
        resolved: r.get(7)?,
        created_at: r.get::<_, i64>(8)? as u64,
    })
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
