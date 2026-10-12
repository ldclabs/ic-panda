use super::*;
use crate::testing::{unlocked, MemoryStorage, TempDir};
use crate::vault::{Filter, Item};

fn note(title: &str) -> Item {
    Item {
        kind: "note".into(),
        title: title.into(),
        tags: vec![],
        body: String::new(),
        username: String::new(),
        secret: "s".into(),
        url: String::new(),
        favorite: false,
        created_at: 1,
        updated_at: 1,
        file: None,
        resolved_conflicts: None,
    }
}

#[tokio::test]
async fn fast_unlock_works_offline_for_seven_days() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.vault_put(None, None, &note("kept")).unwrap();
    assert!(client.enable_fast_unlock().unwrap().fast_unlock);
    client.lock();
    assert_eq!(client.status().unwrap().state, "locked");
    assert_eq!(
        client.vault_list(&Filter::default()).err(),
        Some(Error::Locked)
    );

    // Offline: the stored root still reads; the sync failure is recorded.
    let status = client.unlock_fast().await.unwrap();
    assert_eq!(status.state, "unlocked");
    assert!(status.last_sync.unwrap().error.is_some());
    assert_eq!(client.vault_list(&Filter::default()).unwrap().len(), 1);

    client.lock();
    client.workspace.meta.login_unlocked_at = Some(now_ms() - FAST_UNLOCK_MS - 1);
    assert_eq!(client.unlock_fast().await.err(), Some(Error::AuthRequired));
    client.workspace.meta.login_unlocked_at = Some(now_ms());

    // A key gone from the credential store turns fast unlock off.
    storage.delete(&client.fast_name().unwrap()).unwrap();
    assert_eq!(client.unlock_fast().await.err(), Some(Error::AuthRequired));
    assert!(!client.status().unwrap().fast_unlock);
    assert_eq!(client.unlock_fast().await.err(), Some(Error::NotFound));
}

#[test]
fn background_online_keeps_only_the_device_key() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, true, None);
    let signing = client.keys().unwrap().signing.clone();
    client.lock();
    let status = client.status().unwrap();
    assert_eq!(status.state, "locked");
    assert!(status.resident && status.background_online);
    assert_eq!(*client.signing().unwrap(), *signing);
    assert_eq!(client.keys().err(), Some(Error::Locked));
    assert_eq!(client.roots().err(), Some(Error::Locked));
    assert_eq!(client.vault_show("x").err(), Some(Error::NotFound));
    client.shutdown();
    assert_eq!(client.signing().err(), Some(Error::Locked));

    let other = TempDir::new();
    let mut plain = unlocked(&other.0, false, None);
    plain.lock();
    assert!(!plain.status().unwrap().resident);
    assert_eq!(plain.signing().err(), Some(Error::Locked));
}

#[test]
fn reset_starts_a_new_device() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.vault_put(None, None, &note("gone")).unwrap();
    client.enable_fast_unlock().unwrap();
    let name = client.fast_name().unwrap();
    let before = client.status().unwrap().device_id;
    let status = client.reset().unwrap();
    assert_ne!(status.device_id, before);
    assert_eq!(status.state, "logged_out");
    assert_eq!(status.pending_sync, 0);
    assert!(storage.get(&name).unwrap().is_none());
    // The new workspace is the one on disk.
    drop(client);
    let reopened = Client::open(crate::testing::config(false), &dir.0, None).unwrap();
    assert_eq!(reopened.status().unwrap().device_id, status.device_id);
}
