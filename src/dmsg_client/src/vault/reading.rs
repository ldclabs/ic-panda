//! Local vault reads release the shared client between items. Only encrypted
//! rows are snapshotted; keys stay in the client and a lock stops the read.

use super::{Filter, VaultEntry, health};
use crate::{Client, content::VAULT, hold::Hold};
use dmsg_types::{Error, Result, ensure};
use serde::Serialize;
use std::collections::BTreeMap;

/// Item IDs with unresolved copies, read without decrypting other items.
pub fn conflict_ids(client: &Client) -> Result<std::collections::BTreeSet<String>> {
    client.roots()?;
    Ok(client
        .workspace
        .store
        .conflicts(VAULT)?
        .into_iter()
        .map(|row| row.item)
        .collect())
}

/// Safe list metadata. Hidden values and full field bodies never accumulate
/// while sorting the complete local inventory.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Stable item ID.
    pub id: String,
    /// Immutable version ID.
    pub revision: String,
    /// Vault type.
    pub kind: String,
    /// Display title.
    pub title: String,
    /// A non-hidden preview, at most 160 characters.
    pub subtitle: String,
    /// Explicitly starred by the user.
    pub favorite: bool,
    /// User labels.
    pub tags: Vec<String>,
    /// This version has a relay acknowledgement.
    pub synced: bool,
    /// This version conflicts with another head.
    pub conflict: bool,
    /// Managed by dMsg and read-only.
    pub reserved: bool,
    /// The current version is a tombstone.
    pub deleted: bool,
    /// Item creation time in milliseconds.
    pub created_at: u64,
    /// Item modification time in milliseconds.
    pub updated_at: u64,
}

/// Pick a useful, non-hidden preview for each type.
pub fn subtitle(item: &super::Item) -> String {
    let preferred = match item.kind.as_str() {
        "login" | "api" => "username",
        "ssh" => "fingerprint",
        "wallet" => "address",
        "card" => "cardholder",
        "identity" => "name",
        "server" => "url",
        "database" => "host",
        "license" => "licensed_to",
        _ => "",
    };
    let value = item
        .fields
        .iter()
        .find(|f| f.id == preferred && !f.hidden() && !f.value.is_empty())
        .or_else(|| {
            item.fields
                .iter()
                .find(|f| !f.hidden() && !f.value.is_empty())
        })
        .map(|f| f.value.as_str())
        .unwrap_or(if item.kind == "note" { &item.notes } else { "" });
    value
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(160)
        .collect()
}

impl From<VaultEntry> for Summary {
    fn from(entry: VaultEntry) -> Self {
        Self {
            subtitle: subtitle(&entry.item),
            reserved: entry.item.reserved(),
            id: entry.id,
            revision: entry.revision,
            kind: entry.item.kind,
            title: entry.item.title,
            favorite: entry.item.favorite,
            tags: entry.item.tags,
            synced: entry.synced,
            conflict: entry.conflict,
            deleted: entry.deleted,
            created_at: entry.item.created_at,
            updated_at: entry.item.updated_at,
        }
    }
}

/// Local browsing options, applied before pagination.
pub struct BrowseQuery {
    /// Search and trash scope.
    pub filter: Filter,
    /// Vault type.
    pub kind: Option<String>,
    /// Require a star, or count starred matches in a result.
    pub favorites: bool,
    /// An exact tag to match.
    pub tag: Option<String>,
    /// `title` (ascending) or `updated` (newest first).
    pub sort: String,
    /// Matching rows to skip after sorting.
    pub offset: usize,
    /// Maximum number of rows to return.
    pub limit: usize,
}

/// A page and full-query navigation counts.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsePage {
    /// The requested page of safe summaries.
    pub items: Vec<Summary>,
    /// Matching rows before pagination.
    pub total: usize,
    /// Query and tag matches across all types.
    pub all: usize,
    /// Require a star, or count starred matches in a result.
    pub favorites: usize,
    /// Query and tag matches by type.
    pub kinds: BTreeMap<String, usize>,
    /// User labels.
    pub tags: Vec<String>,
    /// More matching rows follow this page.
    pub has_more: bool,
}

/// Filter and sort before pagination, with counts for the complete query.
/// Only safe summaries are retained; each decryption releases the client.
pub async fn browse<H: Hold>(h: &H, query: &BrowseQuery) -> Result<BrowsePage> {
    dmsg_types::ensure_valid(query.limit > 0 && query.limit <= 10_000, "page")?;
    dmsg_types::ensure_valid(matches!(query.sort.as_str(), "title" | "updated"), "sort")?;
    dmsg_types::ensure_valid(
        query
            .kind
            .as_ref()
            .is_none_or(|k| super::TYPES.contains(&k.as_str())),
        "kind",
    )?;
    let session = h.with(|c| Session::start(c)).await?;
    let conflicts = h
        .with(|c| {
            session.check(c)?;
            conflict_ids(c)
        })
        .await?;
    let mut after = String::new();
    let mut items = Vec::new();
    let mut all = 0;
    let mut favorites = 0;
    let mut kinds = BTreeMap::new();
    let mut tags = std::collections::BTreeSet::new();
    loop {
        let rows = h
            .with(|c| {
                session.check(c)?;
                c.workspace.store.head_objects_after(VAULT, &after, 64)
            })
            .await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            after = row.item.clone();
            let entry = h
                .with(|c| {
                    session.check(c)?;
                    c.vault_list_entry(&row, &query.filter)
                })
                .await?;
            let Some(entry) = entry else {
                continue;
            };
            tags.extend(entry.item.tags.iter().cloned());
            if query
                .tag
                .as_ref()
                .is_some_and(|tag| !entry.item.tags.contains(tag))
            {
                continue;
            }
            all += 1;
            favorites += usize::from(entry.item.favorite);
            *kinds.entry(entry.item.kind.clone()).or_default() += 1;
            if query
                .kind
                .as_ref()
                .is_none_or(|kind| *kind == entry.item.kind)
                && (!query.favorites || entry.item.favorite)
            {
                let mut summary = Summary::from(entry);
                summary.conflict |= conflicts.contains(&summary.id);
                items.push(summary);
            }
        }
    }
    h.with(|c| session.check(c)).await?;
    if query.sort == "title" {
        items.sort_by_cached_key(|e| (e.title.to_lowercase(), e.id.clone()));
    } else {
        items.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
    }
    let total = items.len();
    let has_more = query.offset.saturating_add(query.limit) < total;
    let items = items
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .collect();
    Ok(BrowsePage {
        items,
        total,
        all,
        favorites,
        kinds,
        tags: tags.into_iter().collect(),
        has_more,
    })
}

/// The unlocked session and root in which a local vault operation began.
pub struct Session {
    generation: u64,
    root: u64,
}

impl Session {
    /// Bind an operation before reading or parsing outside the client.
    /// # Errors
    /// Returns `Error::Locked` or `Error::RecoveryIncomplete` without roots.
    pub fn start(client: &Client) -> Result<Self> {
        Ok(Self {
            root: client.roots()?.generation,
            generation: client.session_generation,
        })
    }

    /// Check before using or returning the operation's content.
    /// # Errors
    /// Locking ends the operation, even if subsequently unlocked; a changed
    /// root returns `Error::VersionConflict`.
    pub fn check(&self, client: &Client) -> Result<()> {
        let roots = client.roots()?;
        ensure(client.session_generation == self.generation, Error::Locked)?;
        ensure(roots.generation == self.root, Error::VersionConflict)
    }
}

/// Read current items, releasing the client between decryptions.
/// # Errors
/// As [`Client::vault_list`]; locking during the read returns `Error::Locked`.
pub async fn list<H: Hold>(h: &H, filter: &Filter) -> Result<Vec<VaultEntry>> {
    let (session, rows) = h
        .with(|c| Ok((Session::start(c)?, c.workspace.store.head_objects(VAULT)?)))
        .await?;
    let mut entries = Vec::new();
    for row in rows {
        let entry = h
            .with(|c| {
                session.check(c)?;
                c.vault_list_entry(&row, filter)
            })
            .await?;
        if let Some(entry) = entry {
            entries.push(entry);
        }
    }
    h.with(|c| session.check(c)).await?;
    Ok(entries)
}

/// Page after applying all display filters. Only one encrypted batch and the
/// requested matching entries are retained; each decryption is interruptible.
pub async fn page<H: Hold>(
    h: &H,
    filter: &Filter,
    kind: Option<&str>,
    favorites: bool,
    offset: usize,
    limit: usize,
) -> Result<(Vec<VaultEntry>, bool)> {
    dmsg_types::ensure_valid((1..=100).contains(&limit), "page")?;
    dmsg_types::ensure_valid(kind.is_none_or(|k| super::TYPES.contains(&k)), "kind")?;
    let session = h.with(|c| Session::start(c)).await?;
    let mut after = String::new();
    let mut matched = 0;
    let mut entries = Vec::with_capacity(limit);
    loop {
        let rows = h
            .with(|c| {
                session.check(c)?;
                c.workspace.store.head_objects_after(VAULT, &after, 64)
            })
            .await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            after = row.item.clone();
            let entry = h
                .with(|c| {
                    session.check(c)?;
                    c.vault_list_entry(&row, filter)
                })
                .await?;
            let Some(entry) = entry.filter(|e| {
                kind.is_none_or(|k| e.item.kind == k) && (!favorites || e.item.favorite)
            }) else {
                continue;
            };
            if matched >= offset {
                if entries.len() == limit {
                    return Ok((entries, true));
                }
                entries.push(entry);
            }
            matched += 1;
        }
    }
    h.with(|c| session.check(c)).await?;
    Ok((entries, false))
}

/// Check passwords and dates without holding the client during the analysis.
/// # Errors
/// As [`list`].
pub async fn health<H: Hold>(h: &H) -> Result<Vec<health::Finding>> {
    let session = h.with(|c| Session::start(c)).await?;
    let items: Vec<_> = list(h, &Filter::default())
        .await?
        .into_iter()
        .filter(|e| !e.item.reserved())
        .map(|e| (e.id, e.item))
        .collect();
    let findings = health::findings(&items, crate::now_ms());
    h.with(|c| session.check(c)).await?;
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempDir, unlocked};
    use crate::vault::{Changes, FieldChange};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Lock between the first and second item, as a queued request would.
    struct LockDuringRead {
        client: Mutex<Client>,
        steps: AtomicUsize,
    }

    impl Hold for LockDuringRead {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            let mut client = self.client.lock().unwrap();
            if self.steps.fetch_add(1, Ordering::Relaxed) == 2 {
                client.lock();
            }
            step(&mut client)
        }
    }

    #[tokio::test]
    async fn a_lock_interrupts_a_vault_scan_without_returning_partial_content() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        client
            .vault_add_all(
                (0..3)
                    .map(|i| super::super::Item::new("note", &i.to_string(), 1))
                    .collect(),
            )
            .unwrap();
        let h = LockDuringRead {
            client: Mutex::new(client),
            steps: AtomicUsize::new(0),
        };
        assert_eq!(
            list(&h, &Filter::default()).await.err(),
            Some(Error::Locked)
        );
        assert_eq!(h.steps.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn scans_keep_current_versions_trash_search_and_health_semantics() {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        let changes = Changes {
            title: Some("mail".into()),
            set: Some(vec![FieldChange {
                id: Some("password".into()),
                value: Some("secret-password".into()),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let first = c.vault_create("login", &changes).unwrap();
        let second = c.vault_create("login", &changes).unwrap();
        c.vault_edit(
            &first.id,
            None,
            &Changes {
                title: Some("edited".into()),
                ..Default::default()
            },
        )
        .unwrap();
        for filter in [
            Filter::default(),
            Filter {
                trash: false,
                query: Some("secret-password".into()),
            },
        ] {
            let expected = c.vault_list(&filter).unwrap();
            let actual = list(&c.hold(), &filter).await.unwrap();
            assert_eq!(
                crate::codec::canonical(&actual),
                crate::codec::canonical(&expected)
            );
        }
        assert_eq!(health(&c.hold()).await.unwrap(), c.vault_health().unwrap());
        c.vault_delete(&second.id).unwrap();
        assert_eq!(list(&c.hold(), &Filter::default()).await.unwrap().len(), 1);
        let trash = list(
            &c.hold(),
            &Filter {
                trash: true,
                query: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(trash[0].id, second.id);
        assert!(trash[0].deleted);
        c.workspace
            .store
            .set_head("missing", "revision", VAULT)
            .unwrap();
        assert_eq!(
            list(&c.hold(), &Filter::default()).await.err(),
            Some(Error::IntegrityFailed)
        );
    }

    #[test]
    fn a_prepared_import_cannot_cross_a_lock_or_root_change() {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        let session = Session::start(&c).unwrap();
        c.root.as_mut().unwrap().generation += 1;
        assert_eq!(session.check(&c), Err(Error::VersionConflict));
        c.root.as_mut().unwrap().generation -= 1;
        // Model a lock followed by unlocking the same account/root.
        c.session_generation += 1;
        assert_eq!(session.check(&c), Err(Error::Locked));
        c.lock();
        assert_eq!(session.check(&c), Err(Error::Locked));
    }

    #[test]
    #[ignore = "local performance comparison, run explicitly"]
    fn vault_listing_profile() {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        c.vault_add_all(
            (0..1000)
                .map(|i| super::super::Item::new("note", &format!("item {i}"), 1))
                .collect(),
        )
        .unwrap();
        let started = std::time::Instant::now();
        for _ in 0..10 {
            let mut entries = Vec::new();
            // The previous path: heads, then head + object per item,
            // and decode the same encrypted record twice.
            for (id, _) in c.workspace.store.heads(VAULT).unwrap() {
                let (row, record) = c.current(&id).unwrap();
                if !record.tombstone {
                    entries.push(c.entry(&row).unwrap());
                }
            }
            assert_eq!(entries.len(), 1000);
        }
        let before = started.elapsed();
        let started = std::time::Instant::now();
        for _ in 0..10 {
            assert_eq!(c.vault_list(&Filter::default()).unwrap().len(), 1000);
        }
        eprintln!(
            "1000 items, 10 local list reads: previous={before:?}, joined={:?}",
            started.elapsed()
        );
    }
    #[tokio::test]
    async fn browse_counts_and_order_cover_the_inventory_without_hidden_values() {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        let items = (0..135)
            .map(|i| {
                let mut item = super::super::Item::new(
                    super::super::TYPES[i % 11],
                    &format!("Entry {i:03}"),
                    1,
                );
                item.favorite = i < 9;
                item.tags = vec![if i % 2 == 0 { "work" } else { "personal" }.into()];
                item.fields = vec![super::super::Field::new(
                    "secret",
                    "concealed",
                    "never-in-summary",
                )];
                item
            })
            .collect();
        c.vault_add_all(items).unwrap();
        let mut query = BrowseQuery {
            filter: Filter::default(),
            kind: None,
            favorites: false,
            tag: None,
            sort: "title".into(),
            offset: 0,
            limit: 60,
        };
        let first = browse(&c.hold(), &query).await.unwrap();
        assert_eq!(
            (first.all, first.favorites, first.total, first.kinds.len()),
            (135, 9, 135, 11)
        );
        assert_eq!(first.items[0].title, "Entry 000");
        assert_eq!(first.items[59].title, "Entry 059");
        assert!(first.has_more);
        assert!(
            !serde_json::to_string(&first)
                .unwrap()
                .contains("never-in-summary")
        );
        query.offset = 60;
        assert_eq!(
            browse(&c.hold(), &query).await.unwrap().items[0].title,
            "Entry 060"
        );
        query.offset = 0;
        query.favorites = true;
        assert_eq!(browse(&c.hold(), &query).await.unwrap().items.len(), 9);
        query.tag = Some("work".into());
        query.kind = Some("login".into());
        let scoped = browse(&c.hold(), &query).await.unwrap();
        assert_eq!((scoped.all, scoped.total), (68, 1));
        query = BrowseQuery {
            filter: Filter::default(),
            kind: None,
            favorites: false,
            tag: None,
            sort: "updated".into(),
            offset: 0,
            limit: 60,
        };
        c.vault_edit(
            &first.items[59].id,
            None,
            &super::super::Changes {
                title: Some("Newest change".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            browse(&c.hold(), &query).await.unwrap().items[0].title,
            "Newest change"
        );
        let id = &first.items[0].id;
        let base = &first.items[0].revision;
        c.vault_edit(
            id,
            Some(base),
            &super::super::Changes {
                title: Some("Current".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let copy = c
            .vault_edit(
                id,
                Some(base),
                &super::super::Changes {
                    title: Some("Conflicting".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(
            browse(&c.hold(), &query)
                .await
                .unwrap()
                .items
                .iter()
                .find(|e| e.id == *id)
                .unwrap()
                .conflict
        );
        c.vault_resolve(
            &format!("{}:{}", copy.id, copy.revision),
            &super::super::Keep::Current,
        )
        .unwrap();
        assert!(!conflict_ids(&c).unwrap().contains(id));
        c.vault_delete(id).unwrap();
        assert_eq!(browse(&c.hold(), &query).await.unwrap().all, 134);
        query.filter.trash = true;
        let trash = browse(&c.hold(), &query).await.unwrap();
        assert!(trash.items[0].deleted);
        assert_eq!(trash.items[0].id, *id);
        c.lock();
        assert_eq!(browse(&c.hold(), &query).await.err(), Some(Error::Locked));
    }

    #[tokio::test]
    async fn paging_applies_type_favorites_and_query_before_the_offset() {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        c.vault_add_all(
            (0..130)
                .map(|i| {
                    let mut item = super::super::Item::new(
                        if i % 2 == 0 { "login" } else { "note" },
                        &format!("entry {i}"),
                        1,
                    );
                    item.favorite = i % 3 == 0;
                    item
                })
                .collect(),
        )
        .unwrap();
        let filter = Filter {
            trash: false,
            query: Some("entry".into()),
        };
        let expected: Vec<_> = c
            .vault_list(&filter)
            .unwrap()
            .into_iter()
            .filter(|e| e.item.kind == "login" && e.item.favorite)
            .map(|e| e.id)
            .collect();
        let (first, more) = page(&c.hold(), &filter, Some("login"), true, 0, 10)
            .await
            .unwrap();
        assert!(more);
        assert_eq!(
            first.into_iter().map(|e| e.id).collect::<Vec<_>>(),
            expected[..10]
        );
        let (last, more) = page(&c.hold(), &filter, Some("login"), true, 10, 100)
            .await
            .unwrap();
        assert!(!more);
        assert_eq!(
            last.into_iter().map(|e| e.id).collect::<Vec<_>>(),
            expected[10..]
        );
        c.lock();
        assert_eq!(
            page(&c.hold(), &filter, None, false, 0, 10).await.err(),
            Some(Error::Locked)
        );
    }
}
