//! Unlocked in-memory search caches and LDK-sealed message search buckets.
//!
//! All plaintext search state lives strictly inside [`SearchCache`] on [`Client`]
//! and is dropped immediately by [`SearchCache::clear`] on lock or workspace switch (SEC01).
//! Vault hidden fields (`concealed`, `otp`) are excluded before caching (V01).

use super::matcher::{HighlightSpan, PreparedText, Query, highlight_spans, vault_kind_aliases};
use crate::{
    Client,
    channel::{INBOX, Message},
    content::VAULT,
    hold::Hold,
    vault::{Item, VaultEntry, reading},
};
use dmsg_types::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

const MSG_INDEX_PREFIX: &str = "search-msg:";
const MSG_BUCKET_SIZE: u64 = 64;
const MSG_BUCKET_VERSION: u8 = 1;

/// Thread-safe unlocked search caches owned by [`Client`].
#[derive(Default)]
pub(crate) struct SearchCache {
    inner: Mutex<SearchCacheInner>,
}

#[derive(Default)]
struct SearchCacheInner {
    vault: HashMap<String, CachedVaultDoc>,
    messages: HashMap<String, ChannelMessageCache>,
}

impl SearchCache {
    /// Drop all in-memory search projections immediately (called on lock / workspace switch).
    pub(crate) fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.vault.clear();
            guard.messages.clear();
        }
    }

    /// Update or insert one live message in the in-memory channel cache if already loaded.
    pub(crate) fn upsert_message(&self, channel: &str, message: &Message) {
        if let Ok(mut guard) = self.inner.lock()
            && let Some(cache) = guard.messages.get_mut(channel)
        {
            cache.entries.insert(
                message.seq,
                CachedMessage::from_entry(MessageIndexEntry::from_message(message)),
            );
        }
    }
}

/// Safe, V01-compliant cached projection of one Vault item head.
#[derive(Clone)]
pub(crate) struct CachedVaultDoc {
    pub(crate) revision: String,
    pub(crate) state: String,
    pub(crate) conflict: bool,
    pub(crate) deleted: bool,
    pub(crate) summary: reading::Summary,
    title: PreparedText,
    subtitle: PreparedText,
    kind_aliases: PreparedText,
    tags: Vec<(String, PreparedText)>,
    /// Visible (non-hidden, non-OTP) fields: `(field_id, display_label, prepared_label, prepared_value)`.
    fields: Vec<(String, String, PreparedText, PreparedText)>,
    /// Notes are indexed solely for matching and are never surfaced as list snippets.
    notes: PreparedText,
}

/// Result of matching a [`CachedVaultDoc`] against a [`Query`].
pub(crate) struct VaultDocMatch {
    pub(crate) score: u32,
    pub(crate) title_spans: Vec<HighlightSpan>,
    pub(crate) subtitle_spans: Vec<HighlightSpan>,
    pub(crate) match_field: Option<String>,
    pub(crate) match_value: Option<String>,
}

impl CachedVaultDoc {
    pub(crate) fn from_entry(entry: VaultEntry, state: String, conflict: bool) -> Self {
        let deleted = entry.deleted;
        let revision = entry.revision.clone();
        let summary = reading::Summary::from(entry.clone());
        let item = &entry.item;
        Self::from_parts(revision, state, conflict, deleted, summary, item)
    }

    fn from_parts(
        revision: String,
        state: String,
        conflict: bool,
        deleted: bool,
        summary: reading::Summary,
        item: &Item,
    ) -> Self {
        let title = PreparedText::new(&item.title);
        let subtitle = PreparedText::new(&summary.subtitle);
        let kind_aliases = PreparedText::new(vault_kind_aliases(&item.kind));
        let tags = item
            .tags
            .iter()
            .map(|t| (t.clone(), PreparedText::new(t)))
            .collect();
        // V01: strictly exclude concealed and OTP fields from search indexing.
        let fields = item
            .fields
            .iter()
            .filter(|f| !f.hidden())
            .map(|f| {
                let label = if f.label.trim().is_empty() {
                    f.id.clone()
                } else {
                    f.label.clone()
                };
                (
                    f.id.clone(),
                    label.clone(),
                    PreparedText::new(&label),
                    PreparedText::new(&f.value),
                )
            })
            .collect();
        let notes = PreparedText::new(&item.notes);
        Self {
            revision,
            state,
            conflict,
            deleted,
            summary,
            title,
            subtitle,
            kind_aliases,
            tags,
            fields,
            notes,
        }
    }

    pub(crate) fn match_query(&self, query: &Query) -> Option<VaultDocMatch> {
        if query.is_empty() {
            return Some(VaultDocMatch {
                score: 1,
                title_spans: Vec::new(),
                subtitle_spans: Vec::new(),
                match_field: None,
                match_value: None,
            });
        }
        let mut weighted: Vec<(&'static str, u32, &PreparedText)> =
            Vec::with_capacity(4 + self.tags.len() + self.fields.len() * 2);
        // Index 0 = title (primary), Index 1 = subtitle (primary display)
        weighted.push(("title", 100, &self.title));
        weighted.push(("subtitle", 88, &self.subtitle));
        let tag_start = weighted.len();
        for (_, pt) in &self.tags {
            weighted.push(("tag", 84, pt));
        }
        let field_val_start = weighted.len();
        for (_, _, _, pv) in &self.fields {
            weighted.push(("field", 78, pv));
        }
        let field_lbl_start = weighted.len();
        for (_, _, pl, _) in &self.fields {
            weighted.push(("field_label", 60, pl));
        }
        let kind_idx = weighted.len();
        weighted.push(("type", 55, &self.kind_aliases));
        let notes_idx = weighted.len();
        weighted.push(("notes", 48, &self.notes));

        let doc_match = query.match_prepared_fields(&weighted)?;
        let title_spans = if doc_match.field_ranges[0].is_empty() {
            Vec::new()
        } else {
            highlight_spans(&self.summary.title, &doc_match.field_ranges[0])
        };
        let subtitle_spans = if doc_match.field_ranges[1].is_empty() {
            Vec::new()
        } else {
            highlight_spans(&self.summary.subtitle, &doc_match.field_ranges[1])
        };

        // Determine safe match hint when title/subtitle did not visibly show the match.
        let mut match_field: Option<String> = None;
        let mut match_value: Option<String> = None;
        if let Some(reason) = doc_match.matched_field {
            match reason {
                "subtitle" => {}
                "tag" => {
                    match_field = Some("tag".into());
                    for (i, (tag_name, _)) in self.tags.iter().enumerate() {
                        if !doc_match.field_ranges[tag_start + i].is_empty() {
                            match_value = Some(tag_name.clone());
                            break;
                        }
                    }
                }
                "field" => {
                    for (i, (fid, label, _, _)) in self.fields.iter().enumerate() {
                        if !doc_match.field_ranges[field_val_start + i].is_empty() {
                            match_field = Some(fid.clone());
                            match_value = Some(label.clone());
                            break;
                        }
                    }
                }
                "field_label" => {
                    for (i, (fid, label, _, _)) in self.fields.iter().enumerate() {
                        if !doc_match.field_ranges[field_lbl_start + i].is_empty() {
                            match_field = Some(fid.clone());
                            match_value = Some(label.clone());
                            break;
                        }
                    }
                }
                "type" if !doc_match.field_ranges[kind_idx].is_empty() => {
                    match_field = Some("type".into());
                }
                "notes" if !doc_match.field_ranges[notes_idx].is_empty() => {
                    // Never expose note plaintext in vault list rows.
                    match_field = Some("notes".into());
                }
                _ => {}
            }
        }

        let mut score = doc_match.score;
        if self.summary.favorite {
            score = score.saturating_add(15);
        }

        Some(VaultDocMatch {
            score,
            title_spans,
            subtitle_spans,
            match_field,
            match_value,
        })
    }
}

/// Sync the unlocked in-memory vault cache incrementally against SQLite heads,
/// releasing the client between any newly decrypted items and checking `session` every step.
pub(crate) async fn sync_vault_cache<H: Hold>(
    h: &H,
    session: reading::Session,
) -> Result<(Vec<CachedVaultDoc>, BTreeSet<String>)> {
    let (metas, conflicts, missing_or_stale) = h
        .with(|c| {
            session.check(c)?;
            let metas = c.workspace.store.head_meta(VAULT)?;
            let conflicts = reading::conflict_ids(c)?;
            let mut to_load = Vec::new();
            let guard = c
                .search_cache
                .inner
                .lock()
                .map_err(|_| Error::Unavailable("search cache".into()))?;
            for meta in &metas {
                let up_to_date = guard.vault.get(&meta.item).is_some_and(|cached| {
                    cached.revision == meta.revision
                        && cached.state == meta.state
                        && cached.conflict == meta.conflict
                });
                if !up_to_date {
                    to_load.push(meta.item.clone());
                }
            }
            Ok((metas, conflicts, to_load))
        })
        .await?;

    // Decrypt only new or changed items, releasing the client lock between batches.
    for item_id in missing_or_stale {
        h.with(|c| {
            session.check(c)?;
            let Some(row) = c.workspace.store.head_object(&item_id, VAULT)? else {
                if let Ok(mut guard) = c.search_cache.inner.lock() {
                    guard.vault.remove(&item_id);
                }
                return Ok(());
            };
            let entry = c.vault_decode_row(&row)?;
            let doc = CachedVaultDoc::from_entry(entry, row.state, row.conflict);
            if let Ok(mut guard) = c.search_cache.inner.lock() {
                guard.vault.insert(item_id, doc);
            }
            Ok(())
        })
        .await?;
    }

    // Prune deleted/removed heads and snapshot current docs.
    h.with(|c| {
        session.check(c)?;
        let live_ids: HashSet<&str> = metas.iter().map(|m| m.item.as_str()).collect();
        let mut guard = c
            .search_cache
            .inner
            .lock()
            .map_err(|_| Error::Unavailable("search cache".into()))?;
        guard.vault.retain(|id, _| live_ids.contains(id.as_str()));
        let docs: Vec<CachedVaultDoc> = metas
            .iter()
            .filter_map(|m| guard.vault.get(&m.item).cloned())
            .collect();
        Ok((docs, conflicts))
    })
    .await
}

/// Compact per-message projection stored inside LDK-encrypted search buckets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MessageIndexEntry {
    pub(crate) seq: u64,
    pub(crate) account: String,
    pub(crate) created_at: u64,
    pub(crate) stored_at: u64,
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) file_name: Option<String>,
    #[serde(default)]
    pub(crate) searchable: bool,
}

impl MessageIndexEntry {
    pub(crate) fn from_message(message: &Message) -> Self {
        let searchable =
            !message.amends() && !message.retracted && !message.hidden && !message.unreadable;
        Self {
            seq: message.seq,
            account: message.account.clone(),
            created_at: message.created_at,
            stored_at: message.stored_at,
            text: if searchable {
                message.text.clone()
            } else {
                String::new()
            },
            file_name: if searchable {
                message.file.as_ref().map(|f| f.name.clone())
            } else {
                None
            },
            searchable,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct MessageBucket {
    version: u8,
    entries: BTreeMap<u64, MessageIndexEntry>,
}

/// In-memory decoded message projection with lazily-prepared searchable fields.
#[derive(Clone)]
pub(crate) struct CachedMessage {
    pub(crate) entry: MessageIndexEntry,
    pub(crate) prepared_text: PreparedText,
    pub(crate) prepared_file: Option<PreparedText>,
}

impl CachedMessage {
    fn from_entry(entry: MessageIndexEntry) -> Self {
        let prepared_text = if entry.searchable && !entry.text.is_empty() {
            PreparedText::new(&entry.text)
        } else {
            PreparedText::new("")
        };
        let prepared_file = if entry.searchable {
            entry.file_name.as_deref().map(PreparedText::new)
        } else {
            None
        };
        Self {
            entry,
            prepared_text,
            prepared_file,
        }
    }
}

#[derive(Default)]
struct ChannelMessageCache {
    buckets_loaded: bool,
    entries: BTreeMap<u64, CachedMessage>,
}

fn bucket_id(seq: u64) -> u64 {
    seq / MSG_BUCKET_SIZE
}

fn bucket_key(channel: &str, bucket: u64) -> String {
    format!("{MSG_INDEX_PREFIX}{channel}:{bucket:012}")
}

impl Client {
    /// Keep the message search cache and LDK-sealed bucket consistent after writing a message.
    pub(crate) fn index_message_updated(&self, channel: &str, message: &Message) {
        self.search_cache.upsert_message(channel, message);
        let key = bucket_key(channel, bucket_id(message.seq));
        let _ = self.workspace.store.delete_sealed(&key);
    }

    /// Ensure all locally stored messages of `channel` are loaded into the unlocked search cache,
    /// using LDK-sealed buckets for fast reload and incrementally indexing any uncovered sequences.
    pub(crate) fn ensure_channel_search_index(&self, channel: &str) -> Result<Vec<CachedMessage>> {
        let keys = self.keys()?;
        let msg_prefix = format!("{INBOX}{channel}:");

        // 1. Load existing LDK-sealed buckets once per unlocked session.
        let need_buckets = {
            let guard = self
                .search_cache
                .inner
                .lock()
                .map_err(|_| Error::Unavailable("search cache".into()))?;
            !guard
                .messages
                .get(channel)
                .is_some_and(|c| c.buckets_loaded)
        };
        if need_buckets {
            let b_prefix = format!("{MSG_INDEX_PREFIX}{channel}:");
            let mut loaded = BTreeMap::new();
            let mut cursor = String::new();
            loop {
                let page: Vec<(String, String)> =
                    self.workspace.store.sealed_after(&b_prefix, &cursor, 64)?;
                if page.is_empty() {
                    break;
                }
                for (k, _) in &page {
                    cursor = k.clone();
                    if let Ok(Some(bucket)) = self.workspace.private::<MessageBucket>(keys, k)
                        && bucket.version == MSG_BUCKET_VERSION
                    {
                        for (seq, entry) in bucket.entries {
                            loaded.insert(seq, CachedMessage::from_entry(entry));
                        }
                    }
                }
            }
            let mut guard = self
                .search_cache
                .inner
                .lock()
                .map_err(|_| Error::Unavailable("search cache".into()))?;
            let cache = guard.messages.entry(channel.to_string()).or_default();
            for (seq, msg) in loaded {
                cache.entries.entry(seq).or_insert(msg);
            }
            cache.buckets_loaded = true;
        }

        // 2. Compare against stored message keys and decode only missing sequences.
        let stored_keys = self.workspace.store.sealed_keys(&msg_prefix)?;
        let mut stored_seqs = Vec::with_capacity(stored_keys.len());
        for key in &stored_keys {
            if let Some(seq_str) = key.strip_prefix(&msg_prefix)
                && let Ok(seq) = seq_str.parse::<u64>()
            {
                stored_seqs.push(seq);
            }
        }

        let missing: Vec<u64> = {
            let guard = self
                .search_cache
                .inner
                .lock()
                .map_err(|_| Error::Unavailable("search cache".into()))?;
            let existing = guard.messages.get(channel);
            stored_seqs
                .iter()
                .copied()
                .filter(|seq| existing.is_none_or(|c| !c.entries.contains_key(seq)))
                .collect()
        };

        if !missing.is_empty() {
            let mut dirty_buckets: BTreeSet<u64> = BTreeSet::new();
            let mut newly_indexed = Vec::with_capacity(missing.len());
            for seq in missing {
                let name = format!("{msg_prefix}{seq:020}");
                if let Some(message) = self.workspace.private::<Message>(keys, &name)? {
                    dirty_buckets.insert(bucket_id(seq));
                    newly_indexed.push((
                        seq,
                        CachedMessage::from_entry(MessageIndexEntry::from_message(&message)),
                    ));
                }
            }
            // Merge into memory and collect full bucket snapshots to persist sealed under LDK.
            let buckets_to_write: Vec<(String, MessageBucket)> = {
                let mut guard = self
                    .search_cache
                    .inner
                    .lock()
                    .map_err(|_| Error::Unavailable("search cache".into()))?;
                let cache = guard.messages.entry(channel.to_string()).or_default();
                for (seq, msg) in newly_indexed {
                    cache.entries.insert(seq, msg);
                }
                dirty_buckets
                    .into_iter()
                    .map(|bid| {
                        let start = bid * MSG_BUCKET_SIZE;
                        let end = start.saturating_add(MSG_BUCKET_SIZE);
                        let mut entries = BTreeMap::new();
                        for (&seq, cached) in cache.entries.range(start..end) {
                            entries.insert(seq, cached.entry.clone());
                        }
                        (
                            bucket_key(channel, bid),
                            MessageBucket {
                                version: MSG_BUCKET_VERSION,
                                entries,
                            },
                        )
                    })
                    .collect()
            };
            // Persist sealed buckets; ignore oversized edge cases gracefully without failing search.
            for (bkey, bucket) in buckets_to_write {
                let _ = self.workspace.put_private(keys, &bkey, &bucket);
            }
        }

        // Snapshot searchable entries newest-first for matching.
        let guard = self
            .search_cache
            .inner
            .lock()
            .map_err(|_| Error::Unavailable("search cache".into()))?;
        let Some(cache) = guard.messages.get(channel) else {
            return Ok(Vec::new());
        };
        Ok(cache
            .entries
            .values()
            .rev()
            .filter(|m| m.entry.searchable)
            .cloned()
            .collect())
    }
}
