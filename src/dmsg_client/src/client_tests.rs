use super::*;
use crate::testing::{MemoryStorage, TempDir, unlocked};
use crate::vault::{Changes, Filter};

fn note(title: &str) -> Changes {
    Changes {
        title: Some(title.into()),
        ..Changes::default()
    }
}

#[tokio::test]
async fn fast_unlock_works_offline_for_thirty_days() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.vault_create("note", &note("kept")).unwrap();
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

/// A [`PresenceKey`] that answers as the person tells it to.
#[derive(Default)]
struct Hardware {
    declines: std::sync::atomic::AtomicBool,
    forgot: std::sync::atomic::AtomicBool,
    opened: std::sync::atomic::AtomicU32,
}

impl PresenceKey for Hardware {
    fn presence(&self) -> Option<Presence> {
        Some(Presence::Biometrics)
    }

    fn create(&self) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)> {
        Ok((vec![7; 10], vec![4; 65], Zeroizing::new([9; 32])))
    }

    fn open(&self, handle: &[u8], peer: &[u8]) -> Result<Zeroizing<Key>> {
        use std::sync::atomic::Ordering::SeqCst;
        assert_eq!((handle, peer), (&[7u8; 10][..], &[4u8; 65][..]));
        self.opened.fetch_add(1, SeqCst);
        match (self.declines.load(SeqCst), self.forgot.load(SeqCst)) {
            (true, _) => Err(Error::Forbidden),
            (_, true) => Err(Error::IntegrityFailed),
            _ => Ok(Zeroizing::new([9; 32])),
        }
    }

    fn confirm(&self, _what: &str) -> Result<()> {
        use std::sync::atomic::Ordering::SeqCst;
        match self.declines.load(SeqCst) {
            true => Err(Error::Forbidden),
            false => Ok(()),
        }
    }
}

/// Log `client` in as the test login `seed`, which expires at `expires_at`.
async fn log_in(client: &mut Client, seed: u8, expires_at: u64) -> Principal {
    let login = crate::login::kept_for_tests(seed, expires_at)
        .delegated()
        .unwrap();
    let principal = login.principal;
    client.login = Some(Login {
        ic: Ic::connect(&client.config, login.identity).await.unwrap(),
        principal,
        expires_at,
        kept: login.kept,
    });
    principal
}

#[tokio::test]
async fn fast_unlock_with_the_person_restores_the_login() {
    use std::sync::atomic::Ordering::SeqCst;
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(hardware.clone());
    let principal = log_in(&mut client, 3, now_ms() + DAY).await;

    // A login unlock turns fast unlock on and keeps the login.
    client.after_login_unlock().unwrap();
    let status = client.status().unwrap();
    assert!(status.fast_unlock);
    assert_eq!(status.fast_presence, Some(Presence::Biometrics));
    // The store keeps only the hardware's handle and peer.
    let stored = storage.get(&client.fast_name().unwrap()).unwrap().unwrap();
    assert!(matches!(
        decode::<FastKey>(&stored, MAX_RECORD).unwrap(),
        FastKey::Present { .. }
    ));
    client.lock();
    assert_eq!(client.status().unwrap().principal, None);

    // Declining leaves the device locked, with fast unlock on.
    hardware.declines.store(true, SeqCst);
    assert_eq!(client.unlock_fast().await.err(), Some(Error::Forbidden));
    let status = client.status().unwrap();
    assert_eq!(status.state, "locked");
    assert!(status.fast_unlock);

    hardware.declines.store(false, SeqCst);
    let status = client.unlock_fast().await.unwrap();
    assert_eq!(status.state, "unlocked");
    assert_eq!(status.principal, Some(principal.to_text()));
    assert_eq!(hardware.opened.load(SeqCst), 2);
    // Turned off and on again, the restored login is kept again.
    client.disable_fast_unlock().unwrap();
    client.enable_fast_unlock().unwrap();
    assert!(client.workspace.store.sealed(KEPT_LOGIN).unwrap().is_some());

    // An expired login is not restored, and goes.
    let expired = crate::login::kept_for_tests(3, now_ms() - 1);
    client
        .workspace
        .put_private(client.keys().unwrap(), KEPT_LOGIN, &expired)
        .unwrap();
    client.lock();
    assert_eq!(client.unlock_fast().await.unwrap().principal, None);
    assert!(client.workspace.store.sealed(KEPT_LOGIN).unwrap().is_none());

    // Turned off by the person: the kept login goes and a login unlock
    // leaves fast unlock off.
    log_in(&mut client, 3, now_ms() + DAY).await;
    client.after_login_unlock().unwrap();
    assert!(client.workspace.store.sealed(KEPT_LOGIN).unwrap().is_some());
    assert!(!client.disable_fast_unlock().unwrap().fast_unlock);
    assert!(client.workspace.store.sealed(KEPT_LOGIN).unwrap().is_none());
    client.after_login_unlock().unwrap();
    assert!(!client.status().unwrap().fast_unlock);
    // Turned on again by the person, it keeps the login again.
    assert!(client.enable_fast_unlock().unwrap().fast_unlock);
    assert!(client.workspace.store.sealed(KEPT_LOGIN).unwrap().is_some());

    // A key the hardware no longer opens turns fast unlock off.
    client.lock();
    hardware.forgot.store(true, SeqCst);
    assert_eq!(client.unlock_fast().await.err(), Some(Error::AuthRequired));
    assert!(!client.status().unwrap().fast_unlock);
}

#[tokio::test]
async fn logout_forgets_login_and_fast_unlock_but_keeps_account_data() {
    for locked in [false, true] {
        let dir = TempDir::new();
        let storage = Arc::new(MemoryStorage::default());
        let mut client = unlocked(&dir.0, true, Some(storage.clone()));
        client.set_presence_key(Arc::new(Hardware::default()));
        log_in(&mut client, 3, now_ms() + DAY).await;
        client.after_login_unlock().unwrap();
        client
            .vault_create("note", &note("kept after logout"))
            .unwrap();
        let account = client.account().unwrap().id;
        let device = client.workspace.meta.device_id;
        let fast_name = client.fast_name().unwrap();
        assert!(storage.get(&fast_name).unwrap().is_some());
        assert!(
            !client
                .workspace
                .store
                .sealed_keys(KEPT_LOGIN)
                .unwrap()
                .is_empty()
        );
        if locked {
            client.lock();
            assert!(client.resident.is_some());
        }
        let status = client.logout().unwrap();
        assert_eq!(status.state, "logged_out");
        assert_eq!(status.account, None);
        assert!(!status.fast_unlock);
        assert!(!status.resident);
        assert!(client.login.is_none());
        assert!(storage.get(&fast_name).unwrap().is_none());
        assert_eq!(client.accounts().unwrap().len(), 1);
        assert_eq!(client.logout().unwrap().state, "logged_out");

        drop(client);
        let mut reopened =
            Client::open(crate::testing::config(true), &dir.0, Some(storage)).unwrap();
        assert_eq!(reopened.status().unwrap().state, "logged_out");
        reopened.switch_account(&account).unwrap();
        assert_eq!(reopened.workspace.meta.device_id, device);
        assert!(
            !reopened.workspace.meta.fast_declined,
            "logout keeps the preference"
        );
        assert!(
            reopened
                .workspace
                .store
                .sealed_keys(KEPT_LOGIN)
                .unwrap()
                .is_empty()
        );
        assert_eq!(reopened.unlock_fast().await.err(), Some(Error::NotFound));
        // The new browser login opens the same encrypted workspace.
        reopened.keys = Some(reopened.workspace.unlock(&crate::testing::SECRET).unwrap());
        crate::testing::warm(&mut reopened);
        reopened.load_root().await.unwrap();
        assert_eq!(
            reopened.vault_list(&Filter::default()).unwrap()[0]
                .item
                .title,
            "kept after logout"
        );
        log_in(&mut reopened, 3, now_ms() + DAY).await;
        reopened.after_login_unlock().unwrap();
        assert!(reopened.status().unwrap().fast_unlock);
    }
}

#[tokio::test]
async fn fast_unlock_without_the_person_keeps_no_login() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    log_in(&mut client, 3, now_ms() + DAY).await;
    client.after_login_unlock().unwrap();
    let status = client.status().unwrap();
    assert!(status.fast_unlock);
    assert_eq!(status.fast_presence, None);
    assert!(client.workspace.store.sealed(KEPT_LOGIN).unwrap().is_none());
    client.lock();
    let status = client.unlock_fast().await.unwrap();
    assert_eq!((status.state, status.principal), ("unlocked", None));

    // A credential store that does not decode is a key that is gone.
    storage.set(&client.fast_name().unwrap(), &[1; 32]).unwrap();
    client.lock();
    assert_eq!(client.unlock_fast().await.err(), Some(Error::AuthRequired));
    assert!(!client.status().unwrap().fast_unlock);
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
    client.vault_create("note", &note("gone")).unwrap();
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
    client.vault_create("note", &note("first")).unwrap();
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
    client.vault_create("note", &note("second")).unwrap();

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
    assert!(
        !dir.path("logins")
            .join(principal.to_text())
            .join(WORKSPACE)
            .exists()
    );
    assert!(
        client
            .agent_runtime()
            .unwrap()
            .starts_with(dir.path("accounts").join(account.id.to_string()))
    );
    assert_eq!(client.accounts().unwrap().len(), 1);
}
