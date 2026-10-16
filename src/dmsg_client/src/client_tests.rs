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
    assert!(status.device_id.is_some());
    assert_ne!(status.device_id, before);
    assert_eq!(status.state, "logged_out");
    assert_eq!(status.pending_sync, 0);
    assert!(storage.get(&name).unwrap().is_none());
    // The new workspace is the one on disk.
    drop(client);
    let reopened = Client::open(crate::testing::config(false), &dir.0, None).unwrap();
    assert_eq!(reopened.status().unwrap().device_id, status.device_id);
}

fn other_account() -> AccountRef {
    AccountRef {
        id: "0k0g2081040g2081040g".parse().unwrap(),
        home: Principal::from_text("aaaaa-aa").unwrap(),
        issuer: "https://dmsg.test/u/0k0g2081040g2081040g".into(),
    }
}

#[test]
fn accounts_keep_separate_workspaces() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let first = crate::testing::account();
    client.vault_put(None, None, &note("first")).unwrap();
    let first_device = client.status().unwrap().device_id;

    // A second account on the same device: its own device and data.
    let second = other_account();
    client.select(Slot::Account(second.id)).unwrap();
    assert_eq!(client.keys().err(), Some(Error::Locked));
    let keys = client.workspace.unlock_provisional().unwrap();
    client
        .workspace
        .bind(&keys, second.clone(), &crate::testing::SECRET)
        .unwrap();
    client.keys = Some(client.workspace.unlock(&crate::testing::SECRET).unwrap());
    let root = crate::root::RootKeys::single(1, [6; 32]);
    client.save_root(&root).unwrap();
    client.root = Some(root);
    assert_ne!(client.status().unwrap().device_id, first_device);
    assert!(client.vault_list(&Filter::default()).unwrap().is_empty());
    client.vault_put(None, None, &note("second")).unwrap();

    let listed = client.accounts().unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|a| a.bound));
    assert_eq!(
        listed.iter().find(|a| a.current).unwrap().account,
        second.id.to_string()
    );

    // Switching locks; the other account's data is not readable from here.
    let status = client.switch_account(&first.id).unwrap();
    assert_eq!(status.state, "locked");
    assert_eq!(status.account, Some(first.id.to_string()));
    assert_eq!(status.device_id, first_device);
    assert_eq!(
        client
            .switch_account(&"0m0g2081040g2081040g".parse().unwrap())
            .err(),
        Some(Error::NotFound)
    );

    // A restart opens the account selected last.
    drop(client);
    let reopened = Client::open(crate::testing::config(false), &dir.0, None).unwrap();
    assert_eq!(
        reopened.status().unwrap().account,
        Some(first.id.to_string())
    );
}

#[test]
fn a_login_workspace_becomes_its_account() {
    let dir = TempDir::new();
    let mut client = Client::open(crate::testing::config(false), &dir.0, None).unwrap();
    let status = client.status().unwrap();
    assert_eq!((status.state, status.device_id), ("logged_out", None));
    assert_eq!(client.agent_runtime().err(), Some(Error::AuthRequired));

    let principal = Principal::from_slice(&[7; 29]);
    client.select(Slot::Login(principal)).unwrap();
    let device = client.status().unwrap().device_id;
    assert!(device.is_some());
    client.keys = Some(client.workspace.unlock_provisional().unwrap());
    let account = crate::testing::account();
    let keys = client.keys.take().unwrap();
    client
        .workspace
        .bind(&keys, account.clone(), &crate::testing::SECRET)
        .unwrap();
    client.keys = Some(keys);
    client.adopt(&account.id).unwrap();
    assert!(client.keys().is_ok());
    assert_eq!(client.status().unwrap().device_id, device);
    assert!(!dir
        .path("logins")
        .join(principal.to_text())
        .join(WORKSPACE)
        .exists());
    assert!(client
        .agent_runtime()
        .unwrap()
        .starts_with(dir.path("accounts").join(account.id.to_string())));
    assert_eq!(client.accounts().unwrap().len(), 1);
}
