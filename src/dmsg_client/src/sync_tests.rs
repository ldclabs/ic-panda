use super::*;
use crate::testing::{unlocked, TempDir};
use crate::vault::Filter;
use serde_json::json;

fn item(title: &str) -> Item {
    Item {
        kind: "note".into(),
        title: title.into(),
        tags: vec![],
        body: String::new(),
        username: String::new(),
        secret: String::new(),
        url: String::new(),
        favorite: false,
        created_at: 1,
        updated_at: 1,
        file: None,
        resolved_conflicts: None,
    }
}

/// The relay's rules over vault revisions, in memory: a revision whose base
/// is not the head is stored as a conflict copy; a tombstone needs the head
/// as its base; a deleted item takes only an explicit restore.
#[derive(Default)]
struct Relay {
    through: u64,
    plans: Vec<UploadPlan>,
    revisions: Vec<(Revision, bool)>,
    heads: BTreeMap<String, String>,
    tombstones: BTreeMap<String, bool>,
    chunks: BTreeMap<String, Vec<u8>>,
}

impl Relay {
    /// Take the client's local versions, parents first, and settle each.
    fn accept(&mut self, client: &mut Client) -> (usize, usize) {
        let (mut sent, mut refused) = (0, 0);
        let rows = ordered(client.workspace.store.objects_in(LOCAL).unwrap()).unwrap();
        for row in rows {
            let state = client
                .workspace
                .store
                .object(&row.key)
                .unwrap()
                .unwrap()
                .state;
            if state != LOCAL {
                continue;
            }
            let job: ContentJob = client.workspace.store.job(&row.key).unwrap().unwrap();
            let found = record(&row).unwrap();
            for upload in job.file.iter().chain([&job.vault]) {
                for chunk in &upload.plan.chunks {
                    let bytes = client
                        .workspace
                        .store
                        .chunk(&chunk.digest)
                        .unwrap()
                        .unwrap();
                    self.chunks.insert(chunk.digest.clone(), bytes);
                }
                self.chunks.insert(
                    upload.plan.manifest_digest.clone(),
                    upload.manifest.to_vec(),
                );
                self.plans.push(upload.plan.clone());
                self.through += 1;
            }
            let head = self.heads.get(&found.id).cloned();
            let deleted = head.as_ref().is_some_and(|h| self.tombstones[h]);
            let refuse = found
                .parent
                .as_ref()
                .is_some_and(|p| !self.tombstones.contains_key(p))
                || match deleted {
                    true => !(job.restore && found.parent == head && !found.tombstone),
                    false => job.restore,
                }
                || (found.tombstone && found.parent != head);
            if refuse {
                client.block(&row.key).unwrap();
                refused += 1;
                continue;
            }
            let conflict = head != found.parent;
            self.tombstones
                .insert(found.revision.clone(), found.tombstone);
            if !conflict {
                self.heads.insert(found.id.clone(), found.revision.clone());
            }
            self.revisions.push((
                Revision {
                    item_id: found.id.clone(),
                    revision_id: found.revision.clone(),
                    base_revision: found.parent.clone(),
                    upload_id: (!found.tombstone).then(|| job.vault.plan.upload_id.clone()),
                    tombstone: found.tombstone,
                    restore: job.restore,
                },
                conflict,
            ));
            self.through += 1;
            let verdict = json!({
                "revision_id": found.revision,
                "head": if conflict { head.unwrap() } else { found.revision.clone() },
                "conflict": conflict,
                "tombstone": found.tombstone,
            });
            client.settle(&row.key, &verdict).unwrap();
            sent += 1;
        }
        (sent, refused)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            through: self.through,
            plans: self.plans.clone(),
            revisions: self.revisions.clone(),
        }
    }

    /// What a pull hands the client once the chunks are cached.
    fn deliver(&self, client: &mut Client) -> Result<usize> {
        for (digest, bytes) in &self.chunks {
            client.workspace.store.put_chunk(digest, bytes)?;
        }
        client.apply(&self.snapshot())
    }
}

fn titles(client: &Client, trash: bool) -> Vec<String> {
    client
        .vault_list(&Filter {
            trash,
            ..Filter::default()
        })
        .unwrap()
        .into_iter()
        .map(|e| e.item.title)
        .collect()
}

#[test]
fn concurrent_edits_become_conflict_copies() {
    let (da, db) = (TempDir::new(), TempDir::new());
    let (mut a, mut b) = (unlocked(&da.0, false, None), unlocked(&db.0, false, None));
    let mut relay = Relay::default();
    let first = a.vault_put(None, None, &item("v1")).unwrap();
    assert_eq!(relay.accept(&mut a), (1, 0));
    relay.deliver(&mut b).unwrap();
    assert_eq!(titles(&b, false), ["v1"]);

    a.vault_put(Some(&first.id), None, &item("from a")).unwrap();
    let theirs = b.vault_put(Some(&first.id), None, &item("from b")).unwrap();
    assert_eq!(relay.accept(&mut a), (1, 0));
    // B pushes on a stale base: the relay keeps it as a conflict copy.
    assert_eq!(relay.accept(&mut b), (1, 0));
    relay.deliver(&mut b).unwrap();
    relay.deliver(&mut a).unwrap();
    for device in [&a, &b] {
        assert_eq!(titles(device, false), ["from a"]);
        let conflicts = device.vault_conflicts().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].revision, theirs.revision);
        assert_eq!(conflicts[0].item.title, "from b");
        assert_eq!(device.status().unwrap().conflicts, 1);
    }

    // A adopts B's version; B learns the resolution from the new head.
    let key = format!("{}:{}", theirs.id, theirs.revision);
    a.vault_resolve(&key, false).unwrap();
    assert_eq!(relay.accept(&mut a), (1, 0));
    relay.deliver(&mut b).unwrap();
    relay.deliver(&mut a).unwrap();
    for device in [&a, &b] {
        assert_eq!(titles(device, false), ["from b"]);
        assert!(device.vault_conflicts().unwrap().is_empty());
    }
}

#[test]
fn a_tombstone_takes_only_an_explicit_restore() {
    let (da, db, dc) = (TempDir::new(), TempDir::new(), TempDir::new());
    let mut a = unlocked(&da.0, false, None);
    let mut b = unlocked(&db.0, false, None);
    let mut c = unlocked(&dc.0, false, None);
    let mut relay = Relay::default();
    let first = a.vault_put(None, None, &item("v1")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();
    relay.deliver(&mut c).unwrap();

    a.vault_delete(&first.id).unwrap();
    let late = b
        .vault_put(Some(&first.id), None, &item("late edit"))
        .unwrap();
    let later = c
        .vault_put(Some(&first.id), None, &item("later edit"))
        .unwrap();
    relay.accept(&mut a);
    // Pulled first, an older edit turns into a conflict copy, which the
    // relay then refuses against the tombstone.
    relay.deliver(&mut b).unwrap();
    assert!(titles(&b, false).is_empty());
    assert_eq!(titles(&b, true), ["v1"]);
    let key = format!("{}:{}", late.id, late.revision);
    let row = b.workspace.store.object(&key).unwrap().unwrap();
    assert_eq!((row.state.as_str(), row.conflict), (LOCAL, true));
    assert_eq!(b.vault_conflicts().unwrap().len(), 1);
    assert_eq!(relay.accept(&mut b), (0, 1));
    let row = b.workspace.store.object(&key).unwrap().unwrap();
    assert_eq!(row.state, BLOCKED);
    // Sent without a pull, it is refused the same way.
    assert_eq!(relay.accept(&mut c), (0, 1));
    let refused = format!("{}:{}", later.id, later.revision);
    let row = c.workspace.store.object(&refused).unwrap().unwrap();
    assert_eq!((row.state.as_str(), row.conflict), (BLOCKED, true));
    relay.deliver(&mut c).unwrap();
    assert_eq!(titles(&c, true), ["v1"]);
    assert_eq!(c.vault_conflicts().unwrap().len(), 1);

    // Adopting the late edit restores the item explicitly.
    let restored = b.vault_resolve(&key, false).unwrap();
    assert!(!restored.deleted);
    assert_eq!(relay.accept(&mut b), (1, 0));
    relay.deliver(&mut a).unwrap();
    assert_eq!(titles(&a, false), ["late edit"]);
}

#[test]
fn local_edits_on_the_relay_head_stay() {
    let (da, db) = (TempDir::new(), TempDir::new());
    let (mut a, mut b) = (unlocked(&da.0, false, None), unlocked(&db.0, false, None));
    let mut relay = Relay::default();
    let first = a.vault_put(None, None, &item("v1")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();
    let edit = b
        .vault_put(Some(&first.id), None, &item("pending"))
        .unwrap();
    relay.deliver(&mut b).unwrap();
    assert_eq!(b.vault_show(&first.id).unwrap().revision, edit.revision);
    assert_eq!(b.status().unwrap().pending_sync, 1);
    assert_eq!(relay.accept(&mut b), (1, 0));
    relay.deliver(&mut a).unwrap();
    assert_eq!(titles(&a, false), ["pending"]);
}

#[test]
fn rollback_and_omission_are_refused() {
    let (da, db) = (TempDir::new(), TempDir::new());
    let (mut a, mut b) = (unlocked(&da.0, false, None), unlocked(&db.0, false, None));
    let mut relay = Relay::default();
    a.vault_put(None, None, &item("v1")).unwrap();
    a.vault_put(None, None, &item("v2")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();

    let mut older = relay.snapshot();
    older.through -= 1;
    assert_eq!(b.apply(&older).err(), Some(Error::IntegrityFailed));
    let mut dropped = relay.snapshot();
    dropped.revisions.pop();
    assert_eq!(b.apply(&dropped).err(), Some(Error::IntegrityFailed));
    let mut missing = relay.snapshot();
    missing.plans.pop();
    assert_eq!(b.apply(&missing).err(), Some(Error::RecoveryIncomplete));
    assert_eq!(titles(&b, false).len(), 2);
}

#[test]
fn files_reach_the_other_device() {
    let (da, db) = (TempDir::new(), TempDir::new());
    let (mut a, mut b) = (unlocked(&da.0, false, None), unlocked(&db.0, false, None));
    let mut relay = Relay::default();
    let data: Vec<u8> = (0..(crate::files::CHUNK + 77))
        .map(|i| (i % 13) as u8)
        .collect();
    let path = da.path("photo.png");
    std::fs::write(&path, &data).unwrap();
    let entry = a.file_import(&path).unwrap();
    let empty = da.path("empty.txt");
    std::fs::write(&empty, b"").unwrap();
    let blank = a.file_import(&empty).unwrap();
    assert_eq!(relay.accept(&mut a), (2, 0));
    // Two file uploads and two vault uploads.
    assert_eq!(relay.plans.len(), 4);

    // Without the chunks a pull cannot verify the file.
    let chunks = std::mem::take(&mut relay.chunks);
    let keep: BTreeMap<String, Vec<u8>> = chunks
        .iter()
        .filter(|(d, _)| {
            !entry
                .item
                .file
                .as_ref()
                .unwrap()
                .chunks
                .iter()
                .any(|c| &c.digest == *d)
        })
        .map(|(d, b)| (d.clone(), b.clone()))
        .collect();
    relay.chunks = keep;
    assert_eq!(relay.deliver(&mut b).err(), Some(Error::RecoveryIncomplete));
    relay.chunks = chunks;
    relay.deliver(&mut b).unwrap();

    let out = db.path("photo.png");
    let manifest = b.file_export(&entry.id, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), data);
    assert_eq!(manifest.mime, "image/png");
    b.file_verify(&blank.id).unwrap();
}

#[test]
fn a_diverged_branch_is_one_conflict() {
    let (da, db) = (TempDir::new(), TempDir::new());
    let (mut a, mut b) = (unlocked(&da.0, false, None), unlocked(&db.0, false, None));
    let mut relay = Relay::default();
    let first = a.vault_put(None, None, &item("v1")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();
    a.vault_put(Some(&first.id), None, &item("from a")).unwrap();
    b.vault_put(Some(&first.id), None, &item("b one")).unwrap();
    let tip = b.vault_put(Some(&first.id), None, &item("b two")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();
    assert_eq!(titles(&b, false), ["from a"]);
    let conflicts = b.vault_conflicts().unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].revision, tip.revision);
    assert_eq!(relay.accept(&mut b), (2, 0));
    relay.deliver(&mut a).unwrap();
    assert_eq!(a.vault_conflicts().unwrap().len(), 1);
    // Keeping A's version settles the whole branch everywhere.
    let key = format!("{}:{}", tip.id, tip.revision);
    let kept = a.vault_resolve(&key, true).unwrap();
    assert_eq!(kept.item.title, "from a");
    assert_eq!(kept.item.resolved_conflicts.as_ref().unwrap().len(), 2);
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();
    for device in [&a, &b] {
        assert!(device.vault_conflicts().unwrap().is_empty());
        assert_eq!(device.status().unwrap().conflicts, 0);
        assert_eq!(titles(device, false), ["from a"]);
    }
}
