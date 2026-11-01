use super::*;
use crate::sync::tests::{Relay, item};
use crate::testing::{TempDir, unlocked};
use crate::vault::Filter;
use serde_json::json;

fn page(entries: Vec<Entry>, cursor: u64, end: bool) -> Page {
    Page {
        entries: entries
            .into_iter()
            .enumerate()
            .map(|(i, e)| (i as u64 + 1, e))
            .collect(),
        counts: BTreeMap::new(),
        cursor,
        end,
        verify_ms: 0,
    }
}

#[tokio::test]
async fn cross_page_objects_apply_only_new_versions_and_gc_keeps_the_entire_baseline() {
    let (a, b) = (TempDir::new(), TempDir::new());
    let (mut source, mut client) = (unlocked(&a.0, false, None), unlocked(&b.0, false, None));
    let mut relay = Relay::default();
    for n in 0..500 {
        source
            .vault_create("note", &item(&format!("old {n}")))
            .unwrap();
    }
    relay.accept(&mut source);
    relay.deliver(&mut client).unwrap();
    let before = relay.snapshot();
    let old_cursor = before.through;
    let original = client.workspace.store.chunk_digests().unwrap();
    source.vault_create("note", &item("new")).unwrap();
    relay.accept(&mut source);
    let after = relay.snapshot();
    let plan = after.plans.last().unwrap().clone();
    relay.cache(&client).unwrap();
    let mut stage = SyncStage::DeltaPage;
    let mut report = SyncReport::default();
    client
        .workspace
        .store
        .set_meta(TARGET, &after.through)
        .unwrap();
    apply_incremental_page(
        &client.hold(),
        &page(vec![Entry::Object(plan)], old_cursor + 1, false),
        &mut stage,
        &mut report,
    )
    .await
    .unwrap();
    assert_eq!(client.vault_list(&Filter::default()).unwrap().len(), 500);
    let (revision, conflict) = after.revisions.last().unwrap().clone();
    apply_incremental_page(
        &client.hold(),
        &page(
            vec![Entry::Revision(revision, conflict)],
            after.through,
            true,
        ),
        &mut stage,
        &mut report,
    )
    .await
    .unwrap();
    assert_eq!(report.revisions, 1);
    assert_eq!(report.downloaded_bytes, 0);
    assert_eq!(client.vault_list(&Filter::default()).unwrap().len(), 501);
    assert_eq!(
        client.workspace.store.meta::<u64>(CURSOR).unwrap(),
        Some(after.through)
    );
    assert_eq!(client.workspace.store.meta::<u64>(TARGET).unwrap(), None);
    client.collect_chunks(now_ms()).unwrap();
    assert!(
        original
            .iter()
            .all(|d| client.workspace.store.has_chunk(d).unwrap())
    );
    // An empty incremental page leaves the previously verified history intact.
    apply_incremental_page(
        &client.hold(),
        &page(vec![], after.through, true),
        &mut stage,
        &mut report,
    )
    .await
    .unwrap();
    assert_eq!(client.vault_list(&Filter::default()).unwrap().len(), 501);
}

#[tokio::test]
async fn an_ack_cannot_advance_past_an_omitted_revision_and_a_retry_keeps_other_devices_changes() {
    let (a, b) = (TempDir::new(), TempDir::new());
    let (mut client, mut peer) = (unlocked(&a.0, false, None), unlocked(&b.0, false, None));
    let mut relay = Relay::default();
    relay.deliver(&mut client).unwrap();
    peer.vault_create("note", &item("peer first")).unwrap();
    relay.accept(&mut peer);
    client.vault_create("note", &item("my ack")).unwrap();
    relay.accept(&mut client);
    let all = relay.snapshot();
    relay.cache(&client).unwrap();
    let empty = Snapshot {
        through: all.through,
        ..Snapshot::default()
    };
    assert_eq!(
        client.apply_versions(&empty, false, &[]),
        Err(Error::RecoveryIncomplete)
    );
    assert_eq!(client.workspace.store.meta::<u64>(CURSOR).unwrap(), Some(0));
    let mut report = SyncReport::default();
    let mut stage = SyncStage::DeltaPage;
    let entries = all
        .plans
        .iter()
        .cloned()
        .map(Entry::Object)
        .chain(
            all.revisions
                .iter()
                .cloned()
                .map(|(r, c)| Entry::Revision(r, c)),
        )
        .collect();
    apply_incremental_page(
        &client.hold(),
        &page(entries, all.through, true),
        &mut stage,
        &mut report,
    )
    .await
    .unwrap();
    assert_eq!(client.vault_list(&Filter::default()).unwrap().len(), 2);
    assert!(
        !client
            .workspace
            .store
            .sync_missing_confirmation(all.through)
            .unwrap()
    );
}

#[tokio::test]
async fn invalid_new_content_does_not_advance_the_cursor_or_move_heads() {
    let (a, b) = (TempDir::new(), TempDir::new());
    let (mut source, mut client) = (unlocked(&a.0, false, None), unlocked(&b.0, false, None));
    let mut relay = Relay::default();
    relay.deliver(&mut client).unwrap();
    source.vault_create("note", &item("bad manifest")).unwrap();
    relay.accept(&mut source);
    let mut all = relay.snapshot();
    relay.cache(&client).unwrap();
    let bad = b"invalid encrypted manifest";
    let digest = hex::encode(*sha256(bad));
    client.workspace.store.put_chunk(&digest, bad).unwrap();
    all.plans[0].manifest_digest = digest;
    all.plans[0].manifest_size = bad.len() as u64;
    let entries = vec![
        Entry::Object(all.plans[0].clone()),
        Entry::Revision(all.revisions[0].0.clone(), false),
    ];
    assert!(
        apply_incremental_page(
            &client.hold(),
            &page(entries, all.through, true),
            &mut SyncStage::DeltaPage,
            &mut SyncReport::default()
        )
        .await
        .is_err()
    );
    assert_eq!(client.workspace.store.meta::<u64>(CURSOR).unwrap(), Some(0));
    assert!(client.vault_list(&Filter::default()).unwrap().is_empty());
}

#[test]
fn pages_keep_signature_and_range_validation_in_incremental_mode() {
    use crate::testing::{account, command, device, security};
    let account = account();
    let (id, device) = device(1, None);
    let trust = Trust::new(vec![account.home], "https://dmsg.test/u/".into(), vec![]);
    let mut epochs = BTreeMap::from([(3, security(3, BTreeMap::from([(id, device)])))]);
    let revision = Revision {
        item_id: "aa".repeat(32),
        revision_id: "bb".repeat(32),
        base_revision: None,
        upload_id: Some("cc".repeat(32)),
        tombstone: false,
        restore: false,
    };
    let signed = command(1, 3, "dmsg/vault/revision/v1", &revision);
    let mut value = serde_json::to_value(&revision).unwrap();
    value["signed"] = json!({"cose_sign1": signed});
    value["conflict"] = json!(false);
    let mut data = json!({"entries":[{"cursor":7,"kind":"revision","security_epoch":3,"value":value}], "next_cursor":10,"end":true});
    assert_eq!(
        parse_page(&trust, &account.id, &mut epochs, &data, 5, 10, false)
            .unwrap()
            .cursor,
        10
    );
    data["entries"][0]["value"]["item_id"] = json!("dd".repeat(32));
    assert!(matches!(
        parse_page(&trust, &account.id, &mut epochs, &data, 5, 10, false),
        Err(Error::IntegrityFailed)
    ));
    assert!(
        parse_page(
            &trust,
            &account.id,
            &mut epochs,
            &json!({"entries":[],"next_cursor":11,"end":true}),
            5,
            10,
            false
        )
        .is_err()
    );
}

#[tokio::test]
async fn an_interrupted_explicit_snapshot_resumes_before_advancing_an_existing_baseline() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    client
        .apply(&Snapshot {
            through: 42,
            ..Snapshot::default()
        })
        .unwrap();
    let base = client.base().unwrap();
    let export_id = "aa".repeat(32);
    let job = FullJob {
        request_id: Hash::new([1; 32]),
        export_id: export_id.clone(),
        attempted: true,
        start: Some(json!({"through":50,"counts":[],"expires_at":now_ms()+60000})),
        cursor: 0,
        counts: BTreeMap::new(),
        complete: false,
        close_id: Hash::new([2; 32]),
        close_retry_at: 0,
    };
    client.workspace.store.set_meta(FULL, &job).unwrap();
    let server = crate::sync::tests::scripted_relay(
        &mut client,
        vec![
            (
                "GET",
                format!("{base}/exports/{export_id}?after=0&limit=100"),
                Some(json!({"ok":true,"data":{"entries":[],"next_cursor":0,"inventory_end":true}})),
            ),
            (
                "POST",
                format!("{base}/exports/{export_id}/close"),
                Some(json!({"ok":true,"data":{"closed":true}})),
            ),
        ],
    )
    .await;
    let report = crate::sync::sync(&client.hold()).await.unwrap();
    assert!(report.full_snapshot);
    assert_eq!(report.delta_pages, 0);
    assert_eq!(
        client.workspace.store.meta::<u64>(CURSOR).unwrap(),
        Some(50)
    );
    server.await.unwrap();
}

#[tokio::test]
async fn a_failed_close_does_not_fail_or_delay_subsequent_incremental_reads() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let base = client.base().unwrap();
    let account = client.account().unwrap().id.to_string();
    let id = "aa".repeat(32);
    let server = crate::sync::tests::scripted_relay(&mut client,vec![
        ("POST",format!("{base}/exports"),Some(json!({"ok":true,"data":{"content_sync":1,"account":account,"scope":"cloud_snapshot","export_id":id,"through":0,"counts":[],"expires_at":now_ms()+60000}}))),
        ("GET",format!("{base}/exports/{id}?after=0&limit=100"),Some(json!({"ok":true,"data":{"entries":[],"next_cursor":0,"inventory_end":true}}))),
        ("POST",format!("{base}/exports/{id}/close"),Some(json!({"ok":false,"error":{"code":"UNAVAILABLE"}}))),
        ("GET",format!("{base}/content/changes?after=0&limit=100"),Some(json!({"ok":true,"data":{"content_sync":1,"account":account,"through":0,"entries":[],"next_cursor":0,"end":true}}))),
    ]).await;
    assert!(
        crate::sync::sync(&client.hold())
            .await
            .unwrap()
            .full_snapshot
    );
    let job: FullJob = client.workspace.store.meta(FULL).unwrap().unwrap();
    assert!(job.complete && job.close_retry_at > now_ms());
    assert!(client.last_sync().unwrap().unwrap().error.is_none());
    let report = crate::sync::sync(&client.hold()).await.unwrap();
    assert_eq!(report.delta_pages, 1);
    assert!(!report.full_snapshot);
    server.await.unwrap();
}
