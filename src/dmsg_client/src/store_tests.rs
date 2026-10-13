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
