use crate::{cache::*, display, hold::Hold, testing};
use serde_json::json;

#[test]
fn snapshots_page_large_data_and_reject_missing_or_tampered_pages() {
    let dir = testing::TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    let value = json!({"rows":(0..30_000).map(|n|json!({"id":n,"text":"private-cache-marker"})).collect::<Vec<_>>()});
    c.cache_save(Kind::History, "private-wallet-marker", &value, 42)
        .unwrap();
    let key = c.cache_key(Kind::History, "private-wallet-marker").unwrap();
    assert!(!key.contains("private-wallet-marker"));
    let pages = c
        .workspace
        .store
        .sealed_after(&format!("{key}:"), "", 100)
        .unwrap();
    assert!(pages.len() > 1);
    assert!(
        pages
            .iter()
            .all(|(_, bytes)| !bytes.contains("private-cache-marker"))
    );
    assert_eq!(
        c.cache_read(Kind::History, "private-wallet-marker")
            .unwrap()
            .unwrap()
            .data,
        value
    );
    c.workspace.store.delete_sealed(&pages[0].0).unwrap();
    assert!(
        c.cache_read(Kind::History, "private-wallet-marker")
            .unwrap()
            .is_none()
    );
    assert!(
        c.workspace
            .store
            .sealed_after(&format!("{key}:"), "", 100)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn clearing_and_invalidation_fence_inflight_work_and_preserve_protocol_state() {
    let dir = testing::TempDir::new();
    let mut c = testing::unlocked(&dir.0, false, None);
    for key in [
        "wallet-audit:proof",
        "profile_pins",
        "inbox-letter:only-copy",
        "root",
        "journal",
    ] {
        // These existing records must survive; their contents are immaterial to clearing.
        c.workspace.store.set_sealed(key, "must-stay").unwrap();
    }
    c.cache_save(Kind::Plan, "", &json!({"plan":"Pro"}), 42)
        .unwrap();
    let session = c.display_session().unwrap();
    let plan = c.cache_scope(Kind::Plan).unwrap();
    let inbox = c.cache_scope(Kind::Inbox).unwrap();
    c.cache_invalidate(&[Kind::Plan]).unwrap();
    assert_eq!(
        c.cache_read(Kind::Plan, "")
            .unwrap()
            .unwrap()
            .observed_at_ms,
        0
    );
    // Only reads of the invalidated kind are fenced; other work goes on.
    assert!(
        c.cache_save_checked(&plan, Kind::Plan, "", &json!("late"), 43)
            .is_err()
    );
    c.cache_save_checked(&inbox, Kind::Inbox, "", &json!({"mode":"paid"}), 43)
        .unwrap();
    c.check_display_session(session).unwrap();
    c.cache_invalidate(KINDS).unwrap();
    assert_eq!(
        c.cache_read(Kind::Inbox, "")
            .unwrap()
            .unwrap()
            .observed_at_ms,
        0
    );
    c.cache_clear().unwrap();
    assert!(c.cache_read(Kind::Plan, "").unwrap().is_none());
    let h = c.hold();
    let pending = display::Scoped { inner: &h, session };
    assert!(
        pending
            .with(|c| c.cache_save(Kind::Plan, "", &json!("late"), 43))
            .await
            .is_err()
    );
    drop(h);
    for key in [
        "wallet-audit:proof",
        "profile_pins",
        "inbox-letter:only-copy",
        "root",
        "journal",
    ] {
        assert_eq!(
            c.workspace.store.sealed(key).unwrap().as_deref(),
            Some("must-stay")
        );
    }
}

#[test]
fn source_binding_ignores_new_shards_but_rejects_a_different_authority() {
    let dir = testing::TempDir::new();
    let mut c = testing::unlocked(&dir.0, false, None);
    c.cache_save(Kind::Plan, "", &json!("saved"), 42).unwrap();
    c.config.user_homes.push(candid::Principal::anonymous());
    assert!(c.cache_read(Kind::Plan, "").unwrap().is_some());
    c.config.root_key = Some(vec![1, 2, 3]);
    assert!(c.cache_read(Kind::Plan, "").unwrap().is_none());
}

#[test]
fn native_writes_cannot_repopulate_cleared_caches_or_relabel_changed_sources() {
    let dir = testing::TempDir::new();
    let mut c = testing::unlocked(&dir.0, false, None);
    let old = c.cache_scope(Kind::Inbox).unwrap();
    c.cache_clear().unwrap();
    assert!(
        c.cache_save_checked(&old, Kind::Inbox, "", &json!({"version":1}), 1)
            .is_err()
    );
    assert!(c.cache_read(Kind::Inbox, "").unwrap().is_none());
    let current = c.cache_scope(Kind::Inbox).unwrap();
    c.cache_save_checked(&current, Kind::Inbox, "", &json!({"version":2}), 2)
        .unwrap();
    assert!(
        c.cache_save_checked(&old, Kind::Inbox, "", &json!({"version":1}), 3)
            .is_err()
    );
    assert_eq!(
        c.cache_read(Kind::Inbox, "").unwrap().unwrap().data["version"],
        2
    );
    let source = c.cache_scope(Kind::Asset).unwrap();
    c.config.wallet.bitcoin_api = "https://changed.example".into();
    assert!(
        c.cache_save_checked(&source, Kind::Asset, "btc", &json!("old endpoint"), 4)
            .is_err()
    );
}

#[test]
fn a_b_a_switches_keep_records_separate_and_reject_old_a_work() {
    use crate::{client::Slot, root::RootKeys};
    let dir = testing::TempDir::new();
    let mut c = testing::unlocked(&dir.0, false, None);
    let a = testing::account();
    let old = c.cache_scope(Kind::Plan).unwrap();
    c.cache_save(Kind::Plan, "", &json!("A only"), 1).unwrap();
    c.lock();
    let mut b = a.clone();
    b.id = dmsg_types::AccountId([2; 12]);
    b.issuer = format!("https://dmsg.test/u/{}", b.id);
    c.select(Slot::Account(b.id)).unwrap();
    let keys = c.workspace.unlock_provisional().unwrap();
    c.workspace.bind(&keys, b, &testing::SECRET).unwrap();
    c.keys = Some(c.workspace.unlock(&testing::SECRET).unwrap());
    c.root = Some(RootKeys::single(1, [6; 32]));
    assert!(c.cache_read(Kind::Plan, "").unwrap().is_none());
    assert!(
        c.cache_save_checked(&old, Kind::Plan, "", &json!("late A"), 2)
            .is_err()
    );
    c.cache_save(Kind::Plan, "", &json!("B only"), 2).unwrap();
    c.lock();
    c.select(Slot::Account(a.id)).unwrap();
    c.keys = Some(c.workspace.unlock(&testing::SECRET).unwrap());
    c.root = Some(RootKeys::single(1, [5; 32]));
    assert_eq!(
        c.cache_read(Kind::Plan, "").unwrap().unwrap().data,
        "A only"
    );
    assert!(
        c.cache_save_checked(&old, Kind::Plan, "", &json!("late A"), 3)
            .is_err()
    );
}

#[test]
fn capacity_counts_records_beyond_the_first_inventory_page() {
    let dir = testing::TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    c.workspace
        .store
        .atomic(|store| {
            // Orphan pages are reconstructible too; they must count against capacity.
            for n in 0..4096 {
                store.set_sealed(&format!("display:plan:{n:064x}:0"), "orphan")?;
            }
            Ok(())
        })
        .unwrap();
    c.cache_save(Kind::Plan, "", &json!("x".repeat(1024 * 1024)), 2)
        .unwrap();
    let (count, bytes) = c.workspace.store.sealed_usage("display:plan:").unwrap();
    assert!(count <= 4096 && bytes <= 32 * 1024 * 1024);
    assert!(c.cache_read(Kind::Plan, "").unwrap().is_some());
}
