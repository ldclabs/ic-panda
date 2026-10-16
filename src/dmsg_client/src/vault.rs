//! The vault: items as encrypted versions (see [`crate::content`]). Files
//! are not vault items; they live in the file library ([`crate::library`]).
//!
//! Editing writes a version whose parent is the version edited; deleting
//! writes a tombstone and restoring writes a version on top of it, so
//! history is never overwritten. A version that lost a race is a conflict
//! copy: resolving it writes a new head that names it in
//! `resolvedConflicts`, keeping either side.

use crate::client::Client;
use crate::codec::{canonical, decode};
use crate::content::{record, EncryptedObject, MAX_OBJECT, VAULT};
use crate::store::{ObjectRow, SYNCED};
use crate::upload::random_hex;
use dmsg_types::{ensure, ensure_valid, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use zeroize::Zeroizing;

/// The tag prefix of dMsg's own keys in the vault: Agent ID and controller
/// keys. No agent capability reaches an item tagged with it.
pub const RESERVED_TAG: &str = "dmsg-";

/// A vault item's plaintext.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Item {
    /// `note`, `login`, `api` or `key`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Title, 1 to 200 characters after trimming.
    pub title: String,
    /// Up to 20 tags of at most 40 characters.
    pub tags: Vec<String>,
    /// Body text.
    pub body: String,
    /// Login name.
    pub username: String,
    /// Secret field; shown only on explicit request.
    pub secret: String,
    /// Related URL.
    pub url: String,
    /// Starred.
    pub favorite: bool,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
    /// Last edit in Unix milliseconds.
    pub updated_at: u64,
    /// Conflict copies, as `id:revision`, this version resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_conflicts: Option<Vec<String>>,
}

impl Item {
    /// Whether this is one of dMsg's own keys, which agents never read,
    /// write or reveal.
    pub fn reserved(&self) -> bool {
        self.tags.iter().any(|t| t.starts_with(RESERVED_TAG))
    }

    /// Check the item limits of the vault.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` naming the field, or
    /// `Error::QuotaExceeded` when body and secret exceed 32 KiB.
    pub fn validate(&self) -> Result<()> {
        let chars = |s: &str| s.chars().count();
        let title = self.title.trim();
        ensure_valid(
            matches!(self.kind.as_str(), "note" | "login" | "api" | "key"),
            "type",
        )?;
        ensure_valid(!title.is_empty() && chars(title) <= 200, "title")?;
        ensure_valid(
            self.tags.len() <= 20 && self.tags.iter().all(|t| chars(t.trim()) <= 40),
            "tags",
        )?;
        ensure_valid(chars(&self.body) <= 20_000, "body")?;
        ensure_valid(chars(&self.username) <= 512, "username")?;
        ensure_valid(chars(&self.secret) <= 10_000, "secret")?;
        ensure_valid(chars(&self.url) <= 2048, "url")?;
        ensure(
            self.body.len() + self.secret.len() <= 32 * 1024,
            Error::QuotaExceeded,
        )
    }

    fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        [&self.title, &self.body, &self.username, &self.url]
            .into_iter()
            .chain(&self.tags)
            .any(|field| field.to_lowercase().contains(&query))
    }

    fn resolved(&self) -> BTreeSet<String> {
        self.resolved_conflicts.iter().flatten().cloned().collect()
    }
}

/// One version of an item, opened.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct VaultEntry {
    /// Item ID.
    pub id: String,
    /// This version's revision.
    pub revision: String,
    /// Plaintext.
    pub item: Item,
    /// The relay holds this version.
    pub synced: bool,
    /// A conflict copy beside the item's head.
    pub conflict: bool,
    /// The item is in the trash.
    pub deleted: bool,
}

impl VaultEntry {
    pub(crate) fn new(record: &EncryptedObject, item: Item, synced: bool, conflict: bool) -> Self {
        Self {
            id: record.id.clone(),
            revision: record.revision.clone(),
            item,
            synced,
            conflict,
            deleted: record.tombstone,
        }
    }

    /// The entry as it may leave the daemon: the secret only when revealed.
    pub fn masked(mut self, reveal: bool) -> Self {
        if !reveal && !self.item.secret.is_empty() {
            self.item.secret = "********".into();
        }
        self
    }
}

/// Which items [`Client::vault_list`] returns.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// Deleted items instead of current ones.
    pub trash: bool,
    /// Case-insensitive text in the title, tags, body, user name or URL.
    pub query: Option<String>,
}

impl Client {
    fn open_row(&self, row: &ObjectRow) -> Result<(EncryptedObject, Item)> {
        let record = record(row)?;
        ensure(record.kind == VAULT, Error::NotFound)?;
        let item: Item = decode(&record.open(self.environment(), self.roots()?)?, MAX_OBJECT)?;
        Ok((record, item))
    }

    fn entry(&self, row: &ObjectRow) -> Result<VaultEntry> {
        let (record, item) = self.open_row(row)?;
        Ok(VaultEntry::new(
            &record,
            item,
            row.state == SYNCED,
            row.conflict,
        ))
    }

    fn put_item(
        &mut self,
        id: String,
        parent: Option<String>,
        tombstone: bool,
        item: &Item,
    ) -> Result<VaultEntry> {
        let payload = Zeroizing::new(canonical(item));
        let (record, conflict) = self.write(VAULT, id, parent, tombstone, &payload, None)?;
        Ok(VaultEntry::new(&record, item.clone(), false, conflict))
    }

    /// Add an item, or edit item `id`. An edit replaces `base`, by default
    /// the current version; when the head has moved past `base` the edit is
    /// kept as a conflict copy.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` for an
    /// unknown or deleted item.
    pub fn vault_put(
        &mut self,
        id: Option<&str>,
        base: Option<&str>,
        item: &Item,
    ) -> Result<VaultEntry> {
        let mut item = item.clone();
        item.resolved_conflicts = None;
        let Some(id) = id else {
            item.validate()?;
            return self.put_item(random_hex(), None, false, &item);
        };
        let (head, _) = self.current(id)?;
        let base = base.unwrap_or(&head.revision);
        let row = self
            .workspace
            .store
            .object(&format!("{id}:{base}"))?
            .ok_or(Error::NotFound)?;
        let (record, old) = self.open_row(&row)?;
        ensure(!record.tombstone, Error::NotFound)?;
        ensure_valid(item.kind == old.kind, "type")?;
        item.resolved_conflicts = old.resolved_conflicts;
        item.validate()?;
        self.put_item(id.into(), Some(base.into()), false, &item)
    }

    /// Add an item under a fixed ID, such as a key whose ID its purpose
    /// determines.
    ///
    /// # Errors
    /// Returns `Error::IdempotencyConflict` when the ID is taken.
    pub(crate) fn vault_item_with_id(&mut self, id: &str, item: &Item) -> Result<VaultEntry> {
        ensure(
            self.workspace.store.head(id)?.is_none(),
            Error::IdempotencyConflict,
        )?;
        item.validate()?;
        self.put_item(id.into(), None, false, item)
    }

    /// Move an item to the trash: a tombstone on its current version.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown or deleted item.
    pub fn vault_delete(&mut self, id: &str) -> Result<VaultEntry> {
        let (row, record) = self.current(id)?;
        ensure(!record.tombstone, Error::NotFound)?;
        let (_, item) = self.open_row(&row)?;
        self.put_item(id.into(), Some(record.revision), true, &item)
    }

    /// Restore a deleted item as a new version on top of its tombstone.
    ///
    /// # Errors
    /// Returns `Error::NotFound` unless the item is in the trash.
    pub fn vault_restore(&mut self, id: &str) -> Result<VaultEntry> {
        let (row, record) = self.current(id)?;
        ensure(record.tombstone, Error::NotFound)?;
        let (_, item) = self.open_row(&row)?;
        self.put_item(id.into(), Some(record.revision), false, &item)
    }

    /// The current version of every item, or of every deleted item.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn vault_list(&self, filter: &Filter) -> Result<Vec<VaultEntry>> {
        self.roots()?;
        let mut entries = Vec::new();
        for (item, _) in self.workspace.store.heads(VAULT)? {
            let (row, record) = self.current(&item)?;
            if record.tombstone != filter.trash {
                continue;
            }
            let entry = self.entry(&row)?;
            if filter
                .query
                .as_deref()
                .is_none_or(|q| entry.item.matches(q))
            {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    /// The current version of one item, deleted or not.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown item.
    pub fn vault_show(&self, id: &str) -> Result<VaultEntry> {
        let (row, _) = self.current(id)?;
        self.entry(&row)
    }

    /// Conflict copies waiting for a decision: of a chain of copies, the
    /// newest stands for the rest.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn vault_conflicts(&self) -> Result<Vec<VaultEntry>> {
        self.roots()?;
        let rows = self.workspace.store.conflicts(VAULT)?;
        let parents: BTreeSet<String> = rows
            .iter()
            .filter_map(|row| {
                record(row)
                    .ok()?
                    .parent
                    .map(|p| format!("{}:{p}", row.item))
            })
            .collect();
        rows.iter()
            .filter(|row| !parents.contains(&row.key))
            .map(|row| self.entry(row))
            .collect()
    }

    /// Settle a conflict copy, `id:revision`: adopt it, or keep the current
    /// version. A new head records the copy, and the earlier copies it
    /// replaced, as resolved; a deleted head that is kept stays as it is.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a key that is not a conflict copy.
    pub fn vault_resolve(&mut self, key: &str, keep_current: bool) -> Result<VaultEntry> {
        let row = self
            .workspace
            .store
            .object(key)?
            .filter(|r| r.conflict && !r.resolved)
            .ok_or(Error::NotFound)?;
        let (copy, theirs) = self.open_row(&row)?;
        let (head_row, head) = self.current(&copy.id)?;
        let (_, current) = self.open_row(&head_row)?;
        let mut resolved = current.resolved();
        resolved.extend(theirs.resolved());
        let mut chain = vec![row.key.clone()];
        let mut parent = copy.parent.clone();
        while let Some(revision) = parent {
            let key = format!("{}:{revision}", copy.id);
            match self.workspace.store.object(&key)? {
                Some(earlier) if earlier.conflict => {
                    parent = record(&earlier)?.parent;
                    chain.push(key);
                }
                _ => break,
            }
        }
        resolved.extend(chain.iter().cloned());
        let tombstone = if keep_current {
            head.tombstone
        } else {
            copy.tombstone
        };
        let entry = if tombstone && head.tombstone {
            // A tombstone cannot follow a tombstone; the copy is only
            // dismissed on this device.
            VaultEntry::new(&head, current, head_row.state == SYNCED, false)
        } else {
            let mut item = if keep_current { current } else { theirs };
            item.resolved_conflicts = Some(resolved.into_iter().collect());
            self.put_item(copy.id.clone(), Some(head.revision), tombstone, &item)?
        };
        for key in &chain {
            self.workspace.store.set_resolved(key)?;
        }
        Ok(entry)
    }
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
