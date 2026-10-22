use super::*;
use crate::testing::{TempDir, unlocked};
use crate::vault::{Changes, Filter, Keep};
use serde_json::json;

fn item(title: &str) -> Changes {
    Changes {
        title: Some(title.into()),
        ..Changes::default()
    }
}

#[tokio::test]
async fn a_sync_response_cannot_follow_a_lock_and_reopen_of_the_same_account() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let h = client.hold();
    let generation = h.with(|c| Ok(c.session_generation)).await.unwrap();
    let old = SyncSession { h: &h, generation };
    h.with(|c| {
        c.lock();
        c.keys = Some(c.workspace.unlock(&crate::testing::SECRET)?);
        c.root = Some(crate::root::RootKeys::single(1, [5; 32]));
        Ok(())
    })
    .await
    .unwrap();
    let empty = Snapshot {
        through: 0,
        plans: vec![],
        revisions: vec![],
    };
    assert_eq!(old.with(|c| c.apply(&empty)).await, Err(Error::Locked));
    h.with(|c| {
        assert_eq!(c.status()?.state, "unlocked");
        assert_eq!(c.workspace.store.meta::<u64>(SNAPSHOT)?, None);
        // A new pass under this unlock can apply the same valid response.
        c.apply(&empty)
    })
    .await
    .unwrap();
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
fn partial_legacy_history_syncs_with_its_encrypted_parts_to_another_device() {
    use crate::legacy::{Archive, Offer};
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

    let (da, db) = (TempDir::new(), TempDir::new());
    let (mut a, mut b) = (unlocked(&da.0, false, None), unlocked(&db.0, false, None));
    let vectors = crate::vectors::load("legacy-v1.json");
    let transfer = &vectors["transfer"];
    let offer: Offer = serde_json::from_value(transfer["offer"].clone()).unwrap();
    let seed = hex::decode(transfer["seed_hex"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let opened = crate::legacy::open_transfer(
        &URL_SAFE_NO_PAD
            .decode(transfer["file_b64"].as_str().unwrap())
            .unwrap(),
        &offer,
        &seed,
        transfer["now"].as_u64().unwrap(),
    )
    .unwrap();
    let archive = Archive::decode(&opened.plain).unwrap();
    let checked = crate::legacy::verify(&archive);
    assert!(checked.partial);
    let id = a
        .keep_archive(
            &opened.plain,
            &archive,
            &crate::codec::hex_sha256(&opened.plain),
            checked.gaps.len(),
        )
        .unwrap();
    let mut relay = Relay::default();
    assert_eq!(relay.accept(&mut a), (2, 0));
    relay.deliver(&mut b).unwrap();
    let report = b.legacy_report(&id, 0, 100).unwrap();
    assert_eq!(report["stage"], "partial");
    assert_eq!(report["synced"], true);
    assert_eq!(report["total_messages"], 4);
    assert_eq!(
        b.legacy_channels().unwrap()["channels"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let removed = "nvdn4-5qaaa-aaaaj-qa4pq-cai/channel/1";
    a.legacy_delete_channel(&id, removed).unwrap();
    assert_eq!(relay.accept(&mut a), (3, 0));
    relay.deliver(&mut b).unwrap();
    let history = b.legacy_channels().unwrap();
    assert_eq!(history["channels"].as_array().unwrap().len(), 1);
    assert!(history["errors"].as_array().unwrap().is_empty());
    let report = b.legacy_report(&id, 0, 100).unwrap();
    assert_eq!(report["total_messages"], 0);
    assert_eq!(report["profile"]["name"], "Old Panda");
    assert_eq!(
        b.legacy_channel_report(&id, Some(removed), 0, 100).err(),
        Some(Error::NotFound)
    );
    a.legacy_delete_channel(&id, "nvdn4-5qaaa-aaaaj-qa4pq-cai/channel/2")
        .unwrap();
    assert_eq!(relay.accept(&mut a), (3, 0));
    relay.deliver(&mut b).unwrap();
    assert!(
        b.legacy_channels().unwrap()["channels"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn concurrent_edits_become_conflict_copies() {
    let (da, db) = (TempDir::new(), TempDir::new());
    let (mut a, mut b) = (unlocked(&da.0, false, None), unlocked(&db.0, false, None));
    let mut relay = Relay::default();
    let first = a.vault_create("note", &item("v1")).unwrap();
    assert_eq!(relay.accept(&mut a), (1, 0));
    relay.deliver(&mut b).unwrap();
    assert_eq!(titles(&b, false), ["v1"]);

    a.vault_edit(&first.id, None, &item("from a")).unwrap();
    let theirs = b.vault_edit(&first.id, None, &item("from b")).unwrap();
    assert_eq!(relay.accept(&mut a), (1, 0));
    // B pushes on a stale base: the relay keeps it as a conflict copy.
    assert_eq!(relay.accept(&mut b), (1, 0));
    relay.deliver(&mut b).unwrap();
    relay.deliver(&mut a).unwrap();
    for device in [&a, &b] {
        assert_eq!(titles(device, false), ["from a"]);
        let conflicts = device.vault_conflicts().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].copy.revision, theirs.revision);
        assert_eq!(conflicts[0].copy.item.title, "from b");
        assert_eq!(device.status().unwrap().conflicts, 1);
    }

    // A adopts B's version; B learns the resolution from the new head.
    let key = format!("{}:{}", theirs.id, theirs.revision);
    a.vault_resolve(&key, &Keep::Theirs).unwrap();
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
    let first = a.vault_create("note", &item("v1")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();
    relay.deliver(&mut c).unwrap();

    a.vault_delete(&first.id).unwrap();
    let late = b.vault_edit(&first.id, None, &item("late edit")).unwrap();
    let later = c.vault_edit(&first.id, None, &item("later edit")).unwrap();
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
    let restored = b.vault_resolve(&key, &Keep::Theirs).unwrap();
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
    let first = a.vault_create("note", &item("v1")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();
    let edit = b.vault_edit(&first.id, None, &item("pending")).unwrap();
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
    a.vault_create("note", &item("v1")).unwrap();
    a.vault_create("note", &item("v2")).unwrap();
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
fn a_diverged_branch_is_one_conflict() {
    let (da, db) = (TempDir::new(), TempDir::new());
    let (mut a, mut b) = (unlocked(&da.0, false, None), unlocked(&db.0, false, None));
    let mut relay = Relay::default();
    let first = a.vault_create("note", &item("v1")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();
    a.vault_edit(&first.id, None, &item("from a")).unwrap();
    b.vault_edit(&first.id, None, &item("b one")).unwrap();
    let tip = b.vault_edit(&first.id, None, &item("b two")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();
    assert_eq!(titles(&b, false), ["from a"]);
    let conflicts = b.vault_conflicts().unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].copy.revision, tip.revision);
    assert_eq!(relay.accept(&mut b), (2, 0));
    relay.deliver(&mut a).unwrap();
    assert_eq!(a.vault_conflicts().unwrap().len(), 1);
    // Keeping A's version settles the whole branch everywhere.
    let key = format!("{}:{}", tip.id, tip.revision);
    let kept = a.vault_resolve(&key, &Keep::Current).unwrap();
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

#[test]
fn collection_keeps_what_a_pull_or_an_upload_needs() {
    let (da, db) = (TempDir::new(), TempDir::new());
    let (mut a, mut b) = (unlocked(&da.0, false, None), unlocked(&db.0, false, None));
    let mut relay = Relay::default();
    a.vault_create("note", &item("note")).unwrap();
    relay.accept(&mut a);
    relay.deliver(&mut b).unwrap();

    // B has an unsent edit, a cached attachment and two channel uploads,
    // one of them lapsed.
    b.vault_create("note", &item("unsent")).unwrap();
    let put = |client: &Client, bytes: &[u8]| {
        let digest = hex::encode(*sha256(bytes));
        client.workspace.store.put_chunk(&digest, bytes).unwrap();
        digest
    };
    let cached = put(&b, b"a cached attachment");
    let (live, lapsed) = (put(&b, b"an envelope"), put(&b, b"a lapsed envelope"));
    let now = now_ms();
    let plan = |digest: &str, expires_at: u64| UploadPlan {
        upload_id: random_hex(),
        object_id: random_hex(),
        version_id: random_hex(),
        root_generation: 1,
        epoch: 1,
        control_head: Some(random_hex()),
        kind: "envelopes".into(),
        chunks: vec![ChunkRef {
            digest: digest.into(),
            size: 11,
        }],
        manifest_digest: random_hex(),
        manifest_size: 1,
        expires_at,
    };
    b.hold_uploads([&plan(&live, now + 60_000), &plan(&lapsed, now - 1)])
        .unwrap();
    assert_eq!(b.collect_chunks(&relay.snapshot(), now).unwrap(), 2);
    let has = |digest: &str| b.workspace.store.has_chunk(digest).unwrap();
    assert!(relay.chunks.keys().all(|d| has(d)));
    assert!(has(&live));
    assert!(!has(&cached) && !has(&lapsed));
    assert_eq!(b.collect_chunks(&relay.snapshot(), now).unwrap(), 0);
    // The unsent edit still uploads, and A reads it.
    assert_eq!(relay.accept(&mut b), (1, 0));
    relay.deliver(&mut a).unwrap();
    assert!(titles(&a, false).contains(&"unsent".to_string()));
}

#[test]
fn snapshot_entries_verify_against_the_evidence_of_their_epoch() {
    use crate::testing::{account, device, security, signed};
    use candid::Principal;

    const REVISION: &str = "dmsg/vault/revision/v1";
    type Payload = BTreeMap<String, String>;
    let trust = Trust::new(
        vec![Principal::from_slice(&[1])],
        "https://dmsg.test/u/".into(),
        vec![],
    );
    let id = account().id;
    let (a, active) = device(1, None);
    let cose = signed(1, 3, REVISION);
    let entry = json!({ "security_epoch": 3 });
    let mut epochs = BTreeMap::new();
    let verify = |epochs: &mut BTreeMap<u64, Security>, page: &Json, entry: &Json| {
        verify_signed_entry::<Payload>(&trust, &id, epochs, page, entry, &cose, REVISION, 20)
    };
    // An epoch with no evidence on the page, or evidence that does not check,
    // verifies nothing.
    assert_eq!(
        verify(&mut epochs, &json!({}), &entry).unwrap_err(),
        Error::IntegrityFailed
    );
    let foreign = json!({ "security": { "3": {
        "evidence": { "schema": 1, "canister": "aaaaa-aa", "certificate": "", "entries": [] },
        "stored_at": 15,
    } } });
    assert_eq!(
        verify(&mut epochs, &foreign, &entry).unwrap_err(),
        Error::IntegrityFailed
    );
    assert!(epochs.is_empty());
    // An epoch checked once in a pull is not checked again: its devices
    // decide, whatever the account has today.
    epochs.insert(3, security(3, BTreeMap::from([(a, active.clone())])));
    assert_eq!(verify(&mut epochs, &json!({}), &entry).unwrap()["by"], "1");
    // An entry that names another epoch than its command's, or none.
    epochs.insert(4, security(4, BTreeMap::from([(a, active)])));
    assert_eq!(
        verify(&mut epochs, &json!({}), &json!({ "security_epoch": 4 })).unwrap_err(),
        Error::IntegrityFailed
    );
    assert_eq!(
        verify(&mut epochs, &json!({}), &json!({})).unwrap_err(),
        Error::IntegrityFailed
    );
}
