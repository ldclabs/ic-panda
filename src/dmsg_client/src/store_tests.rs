use super::*;
use crate::testing::TempDir;

#[test]
fn a_new_workspace_gives_collected_pages_back() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path("workspace.sqlite")).unwrap();
    let pragma = |name: &str| -> i64 {
        store
            .conn()
            .query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(pragma("auto_vacuum"), 2);
    let digests: Vec<String> = (0..4u8)
        .map(|i| {
            let bytes = vec![i; 256 * 1024];
            let digest = hex::encode(*crate::codec::sha256(&bytes));
            store.put_chunk(&digest, &bytes).unwrap();
            digest
        })
        .collect();
    let full = pragma("page_count");
    store.delete_chunks(&digests).unwrap();
    assert_eq!(pragma("freelist_count"), 0);
    assert!(pragma("page_count") < full / 4);
}

#[test]
fn sealed_ranges_stop_at_their_prefix() {
    let store = Store::memory().unwrap();
    for key in ["a:", "a:1", "a:2", "a:3", "a;", "ab:1", "b:1"] {
        store.set_sealed(key, key).unwrap();
    }
    assert_eq!(
        store.sealed_keys("a:").unwrap(),
        ["a:", "a:1", "a:2", "a:3"]
    );
    assert_eq!(store.sealed_keys("ab:").unwrap(), ["ab:1"]);
    assert!(store.sealed_keys("c:").unwrap().is_empty());
    let page = |after: &str, limit| {
        store
            .sealed_after("a:", after, limit)
            .unwrap()
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>()
    };
    assert_eq!(page("a:1", 10), ["a:2", "a:3"]);
    assert_eq!(page("a:1", 1), ["a:2"]);
    assert_eq!(page("", 10), ["a:", "a:1", "a:2", "a:3"]);
    assert!(page("a:3", 10).is_empty());
}

#[test]
fn approvals_are_counted_and_pruned_by_state() {
    let store = Store::memory().unwrap();
    let row = |id: &str, state: &str, created_at: u64, expires_at: u64| ApprovalRow {
        id: id.into(),
        state: state.into(),
        class: "cli".into(),
        created_at,
        expires_at,
    };
    for approval in [
        row("a", "awaiting_user", 1, 100),
        row("b", "awaiting_user", 2, 50),
        row("c", "unknown", 3, 10),
        row("d", "done", 4, 10),
        row("e", "done", 40, 50),
    ] {
        store.put_approval(&approval).unwrap();
    }
    assert_eq!(store.count_approvals(&["awaiting_user"], 60).unwrap(), 1);
    assert_eq!(
        store.count_approvals(&["executing", "unknown"], 0).unwrap(),
        1
    );
    let ids = |rows: Vec<ApprovalRow>| rows.into_iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(
        ids(store.approvals(&["awaiting_user"]).unwrap()),
        ["a", "b"]
    );
    assert_eq!(ids(store.approvals(&[]).unwrap()).len(), 5);
    assert_eq!(store.prune_approvals(&["done"], 10).unwrap(), ["d"]);
    assert_eq!(ids(store.approvals(&[]).unwrap()), ["a", "b", "c", "e"]);
}

#[test]
fn the_audit_keeps_its_newest_entries() {
    let store = Store::memory().unwrap();
    let entry = |at: u64| AuditRow {
        seq: 0,
        at,
        actor: "owner".into(),
        via: "cli".into(),
        action: "test".into(),
        target: None,
        detail: None,
        outcome: "ok".into(),
    };
    for at in 1..=10 {
        store.put_audit(&entry(at), 0, 4).unwrap();
    }
    let kept: Vec<u64> = store
        .audit(u64::MAX, None, 100)
        .unwrap()
        .iter()
        .map(|r| r.at)
        .collect();
    assert_eq!(kept, [10, 9, 8, 7]);
    store.put_audit(&entry(11), 9, 4).unwrap();
    let kept: Vec<u64> = store
        .audit(u64::MAX, None, 100)
        .unwrap()
        .iter()
        .map(|r| r.at)
        .collect();
    assert_eq!(kept, [11, 10, 9]);
}

#[test]
fn mcp_events_keep_the_newest() {
    let store = Store::memory().unwrap();
    for n in 0..6u64 {
        store
            .put_mcp_event("e", &n.to_string(), "event", n, "sealed")
            .unwrap();
    }
    store.prune_mcp_events(1, 3).unwrap();
    let kept: Vec<String> = store
        .mcp_events(&["e".into()], 0, 10)
        .unwrap()
        .into_iter()
        .map(|r| r.webhook_id)
        .collect();
    assert_eq!(kept, ["3", "4", "5"]);
}
