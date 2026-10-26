//! Local vault reads release the shared client between items. Only encrypted
//! rows are snapshotted; keys stay in the client and a lock stops the read.

use super::{Filter, VaultEntry, health};
use crate::{Client, content::VAULT, hold::Hold};
use dmsg_types::{Error, Result, ensure};

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
