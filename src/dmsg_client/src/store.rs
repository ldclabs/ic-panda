//! SQLite storage of one workspace. The store holds plaintext metadata,
//! records the caller has already sealed, and encrypted content objects; it
//! never sees a key.
//!
//! A store has a single owner, the client, which serializes its use; each
//! statement takes the connection lock, so [`Store::atomic`] groups the
//! owner's writes but does not isolate them from the owner's own reads.

use dmsg_types::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
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
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS objects_state ON objects (state, created_at);
CREATE TABLE IF NOT EXISTS heads (
  item TEXT PRIMARY KEY,
  revision TEXT NOT NULL,
  kind TEXT NOT NULL
);
";

/// Where a local object version stands with the relay.
pub const LOCAL: &str = "local";
/// The relay accepted the version.
pub const SYNCED: &str = "synced";

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
    /// [`LOCAL`] or [`SYNCED`].
    pub state: String,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
}

fn storage(error: rusqlite::Error) -> Error {
    Error::Unavailable(format!("storage: {error}"))
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
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(storage)?;
        conn.pragma_update(None, "synchronous", "FULL")
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

    /// A plaintext metadata value.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails or the value is corrupt.
    pub fn meta<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let bytes: Option<Vec<u8>> = self
            .conn()
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(storage)?;
        bytes
            .map(|b| cbor2::from_slice(&b).map_err(|_| Error::Unavailable("corrupt meta".into())))
            .transpose()
    }

    /// Store a plaintext metadata value. Never pass secrets.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn set_meta<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn()
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
                params![key, crate::codec::canonical(value)],
            )
            .map(|_| ())
            .map_err(storage)
    }

    /// Remove a metadata value.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn delete_meta(&self, key: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM meta WHERE key = ?1", [key])
            .map(|_| ())
            .map_err(storage)
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

    /// Insert an object version; an existing key must hold the same frame.
    ///
    /// # Errors
    /// A different frame under the same key returns `Error::IdempotencyConflict`.
    pub fn put_object(&self, row: &ObjectRow) -> Result<()> {
        if let Some(existing) = self.object(&row.key)? {
            return dmsg_types::ensure(existing.frame == row.frame, Error::IdempotencyConflict);
        }
        self.conn()
            .execute(
                "INSERT INTO objects (key, item, revision, kind, frame, state, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    row.key,
                    row.item,
                    row.revision,
                    row.kind,
                    row.frame,
                    row.state,
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
                "SELECT key, item, revision, kind, frame, state, created_at FROM objects WHERE key = ?1",
                [key],
                row,
            )
            .optional()
            .map_err(storage)
    }

    /// Object versions in one state, oldest first.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn objects_in(&self, state: &str) -> Result<Vec<ObjectRow>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT key, item, revision, kind, frame, state, created_at FROM objects
                 WHERE state = ?1 ORDER BY created_at, key",
            )
            .map_err(storage)?;
        let rows = statement.query_map([state], row).map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    /// Mark an object version's sync state.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when SQLite fails.
    pub fn set_object_state(&self, key: &str, state: &str) -> Result<()> {
        self.conn()
            .execute(
                "UPDATE objects SET state = ?2 WHERE key = ?1",
                params![key, state],
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
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ObjectRow> {
    Ok(ObjectRow {
        key: r.get(0)?,
        item: r.get(1)?,
        revision: r.get(2)?,
        kind: r.get(3)?,
        frame: r.get(4)?,
        state: r.get(5)?,
        created_at: r.get::<_, i64>(6)? as u64,
    })
}
