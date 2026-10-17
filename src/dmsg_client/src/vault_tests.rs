use super::*;
use crate::store::LOCAL;
use crate::testing::{TempDir, unlocked};

fn item(title: &str) -> Item {
    Item {
        kind: "note".into(),
        title: title.into(),
        tags: vec!["work".into()],
        body: "body".into(),
        username: String::new(),
        secret: "s3cret".into(),
        url: String::new(),
        favorite: false,
        created_at: 1,
        updated_at: 1,
        resolved_conflicts: None,
    }
}

#[test]
fn items_have_limits() {
    assert!(item("x").validate().is_ok());
    assert!(item(" ").validate().is_err());
    assert!(item(&"x".repeat(201)).validate().is_err());
    let mut big = item("x");
    big.body = "x".repeat(20_000);
    big.secret = "y".repeat(10_000);
    assert!(big.validate().is_ok());
    big.body = "汉".repeat(12_000);
    assert_eq!(big.validate(), Err(Error::QuotaExceeded));
    let mut kind = item("x");
    kind.kind = "file".into();
    assert!(kind.validate().is_err());
}

#[test]
fn edits_deletes_and_restores_are_versions() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let first = client.vault_put(None, None, &item("alpha")).unwrap();
    assert!(!first.synced && !first.conflict && !first.deleted);
    let mut changed = item("alpha 2");
    changed.secret = "new".into();
    let second = client.vault_put(Some(&first.id), None, &changed).unwrap();
    assert_eq!(second.id, first.id);
    assert_ne!(second.revision, first.revision);
    assert_eq!(client.vault_show(&first.id).unwrap().item.title, "alpha 2");

    let found = client
        .vault_list(&Filter {
            query: Some("WORK".into()),
            ..Filter::default()
        })
        .unwrap();
    assert_eq!(found.len(), 1);
    let none = client
        .vault_list(&Filter {
            query: Some("nothing".into()),
            ..Filter::default()
        })
        .unwrap();
    assert!(none.is_empty());

    let deleted = client.vault_delete(&first.id).unwrap();
    assert!(deleted.deleted);
    assert!(client.vault_list(&Filter::default()).unwrap().is_empty());
    let trash = client
        .vault_list(&Filter {
            trash: true,
            ..Filter::default()
        })
        .unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(client.vault_delete(&first.id).err(), Some(Error::NotFound));
    assert_eq!(
        client.vault_put(Some(&first.id), None, &item("x")).err(),
        Some(Error::NotFound)
    );

    let restored = client.vault_restore(&first.id).unwrap();
    assert!(!restored.deleted);
    assert_eq!(restored.item.secret, "new");
    assert_eq!(client.vault_list(&Filter::default()).unwrap().len(), 1);
    // Every version waits for the relay, each with its job.
    assert_eq!(client.workspace.store.count_in(LOCAL).unwrap(), 4);
    let jobs: Vec<(String, crate::sync::ContentJob)> = client.workspace.store.jobs().unwrap();
    assert_eq!(jobs.len(), 4);

    let masked = restored.masked(false);
    assert_eq!(masked.item.secret, "********");
}

#[test]
fn a_stale_edit_is_a_conflict_copy() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let first = client.vault_put(None, None, &item("alpha")).unwrap();
    let head = client
        .vault_put(Some(&first.id), None, &item("ours"))
        .unwrap();
    let stale = client
        .vault_put(Some(&first.id), Some(&first.revision), &item("theirs"))
        .unwrap();
    assert!(stale.conflict);
    let key = format!("{}:{}", stale.id, stale.revision);
    // A conflict copy is still sent, so the relay keeps it for every device.
    let row = client.workspace.store.object(&key).unwrap().unwrap();
    assert_eq!((row.state.as_str(), row.conflict), (LOCAL, true));
    assert!(
        client
            .workspace
            .store
            .job::<crate::sync::ContentJob>(&key)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        client.vault_show(&first.id).unwrap().revision,
        head.revision
    );
    assert_eq!(client.vault_conflicts().unwrap().len(), 1);

    let resolved = client.vault_resolve(&key, false).unwrap();
    assert_eq!(resolved.item.title, "theirs");
    assert_eq!(resolved.item.resolved_conflicts, Some(vec![key.clone()]));
    assert!(client.vault_conflicts().unwrap().is_empty());
    assert_eq!(
        client.vault_resolve(&key, true).err(),
        Some(Error::NotFound)
    );
    // Later edits keep the record of what was resolved.
    let edited = client
        .vault_put(Some(&first.id), None, &item("later"))
        .unwrap();
    assert_eq!(edited.item.resolved_conflicts, Some(vec![key]));
}
