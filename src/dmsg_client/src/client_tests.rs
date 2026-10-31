use super::*;
use crate::testing::{MemoryStorage, TempDir, unlocked};
use crate::vault::{Changes, Filter};

#[tokio::test]
async fn mobile_presence_policy_refuses_plain_keys_and_lock_invalidates_a_prompt() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    // Exercise the explicit unbound compatibility path; default enrollment
    // has separate coverage below.
    client.workspace.meta.bound_declined = true;
    client.require_fast_presence();
    assert_eq!(client.enable_fast_unlock().err(), Some(Error::Forbidden));
    assert!(!client.status().unwrap().fast_unlock);
    client.set_presence_key(Arc::new(Hardware::default()));
    client.enable_fast_unlock().unwrap();
    client.lock();
    let prepared = client.prepare_fast_unlock().unwrap();
    let key = prepared.open();
    client.lock();
    assert_eq!(
        client.complete_fast_unlock(prepared, key).await.err(),
        Some(Error::Locked)
    );
    assert!(client.keys().is_err());

    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.enable_fast_unlock().unwrap();
    let name = client.fast_name().unwrap();
    client.require_fast_presence();
    client.lock();
    assert_eq!(
        client.prepare_fast_unlock().err(),
        Some(Error::AuthRequired)
    );
    assert!(!client.status().unwrap().fast_unlock);
    assert!(storage.get(&name).unwrap().is_none());
}

#[test]
fn idle_lock_keeps_a_locked_generation_for_work_that_runs_while_locked() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let open = client.session_generation;
    client.lock_idle();
    assert!(client.keys().is_err());
    let locked = client.session_generation;
    assert_ne!(locked, open);
    // The daemon repeats its idle check every pass while locked.
    client.lock_idle();
    assert_eq!(client.session_generation, locked);
    client.lock();
    assert_ne!(client.session_generation, locked);
}

#[tokio::test]
async fn prepared_device_bound_unlock_preserves_the_binding_and_rejects_late_answers() {
    use std::sync::atomic::Ordering::SeqCst;
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.require_fast_presence();
    client.set_presence_key(hardware.clone());
    bind_offline(&mut client, &hardware, &storage, false);
    let name = client.bound_name().unwrap();
    let slot = storage.get(&name).unwrap().unwrap();
    // Bound fast unlock has no independent fast-key record.
    assert!(storage.get(&client.fast_name().unwrap()).unwrap().is_none());
    client.lock();
    let count = hardware.opened.load(SeqCst);
    let prepared = client.prepare_fast_unlock().unwrap();
    assert_eq!(hardware.opened.load(SeqCst), count);
    let key = prepared.open();
    assert_eq!(hardware.opened.load(SeqCst), count + 1);
    client.lock();
    assert_eq!(
        client.complete_fast_unlock(prepared, key).await.err(),
        Some(Error::Locked)
    );
    assert_eq!(storage.get(&name).unwrap().unwrap(), slot);
    assert!(client.keys().is_err());

    let prepared = client.prepare_fast_unlock().unwrap();
    let key = prepared.open();
    let status = client.complete_fast_unlock(prepared, key).await.unwrap();
    assert_eq!(status.state, "unlocked");
    assert!(status.device_bound && status.fast_unlock);
}

#[tokio::test]
async fn prepared_login_factor_cannot_unlock_after_lock_or_slot_replacement() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(hardware.clone());
    bind_offline(&mut client, &hardware, &storage, false);
    let slot = client.bound_slot().unwrap().unwrap();
    let prepared = LoginUnlock {
        presence: client.device_presence(slot).unwrap(),
        secret: Zeroizing::new(crate::testing::SECRET),
        initializing: None,
    };
    let key = prepared.open();
    client.lock();
    assert_eq!(
        client.complete_login_unlock(prepared, key).await.err(),
        Some(Error::Locked)
    );
    assert!(client.keys().is_err());
    let slot = client.bound_slot().unwrap().unwrap();
    let prepared = LoginUnlock {
        presence: client.device_presence(slot).unwrap(),
        secret: Zeroizing::new(crate::testing::SECRET),
        initializing: None,
    };
    let key = prepared.open();
    let mut replacement = client.bound_slot().unwrap().unwrap();
    replacement.handle = vec![99; 10].into();
    storage
        .set(&client.bound_name().unwrap(), &canonical(&replacement))
        .unwrap();
    assert_eq!(
        client.complete_login_unlock(prepared, key).await.err(),
        Some(Error::Locked)
    );
    assert!(client.workspace.bound().unwrap());
    assert!(client.keys().is_err());
}

#[tokio::test]
async fn enabling_bound_fast_unlock_waits_off_client_and_preserves_slot_on_cancellation() {
    use std::sync::atomic::Ordering::SeqCst;
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(hardware.clone());
    bind_offline(&mut client, &hardware, &storage, false);
    client.disable_fast_unlock().unwrap();
    let slot = storage.get(&client.bound_name().unwrap()).unwrap().unwrap();
    let count = hardware.opened.load(SeqCst);
    let prepared = client.prepare_enable_fast_unlock().unwrap().unwrap();
    assert_eq!(hardware.opened.load(SeqCst), count);
    hardware.declines.store(true, SeqCst);
    let key = prepared.open();
    assert_eq!(
        client.complete_enable_fast_unlock(prepared, key).err(),
        Some(Error::Forbidden)
    );
    assert!(!client.status().unwrap().fast_unlock);
    assert_eq!(
        storage.get(&client.bound_name().unwrap()).unwrap().unwrap(),
        slot
    );
    hardware.declines.store(false, SeqCst);
    let prepared = client.prepare_enable_fast_unlock().unwrap().unwrap();
    let key = prepared.open();
    assert!(
        client
            .complete_enable_fast_unlock(prepared, key)
            .unwrap()
            .fast_unlock
    );
    client.disable_fast_unlock().unwrap();
    let prepared = client.prepare_enable_fast_unlock().unwrap().unwrap();
    let key = prepared.open();
    client.lock();
    assert_eq!(
        client.complete_enable_fast_unlock(prepared, key).err(),
        Some(Error::Locked)
    );
    assert!(!client.status().unwrap().fast_unlock);
}

#[tokio::test]
async fn new_binding_is_not_persisted_before_presence_or_after_a_session_change() {
    use std::sync::atomic::Ordering::SeqCst;
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(hardware.clone());
    log_in(&mut client, 3, now_ms() + DAY).await;
    let name = client.bound_name().unwrap();
    let prepared = client.prepare_device_binding(true).unwrap();
    assert_eq!(hardware.opened.load(SeqCst), 0);
    assert!(storage.get(&name).unwrap().is_none());
    let opened = prepared.open().unwrap();
    assert_eq!(hardware.opened.load(SeqCst), 1);
    assert!(storage.get(&name).unwrap().is_none());
    client.lock();
    assert_eq!(
        client.complete_device_binding(opened).await.err(),
        Some(Error::Locked)
    );
    assert!(!client.workspace.bound().unwrap());
    assert!(storage.get(&name).unwrap().is_none());
}

#[tokio::test]
async fn pausing_host_work_drops_its_hardware_approval_authorization() {
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    struct Answer(Arc<AtomicBool>);
    impl crate::secure::Authorized for Answer {
        fn sign(&self, _: &[u8], _: &Hash) -> Result<[u8; 64]> {
            Err(Error::Forbidden)
        }
    }
    impl Drop for Answer {
        fn drop(&mut self) {
            self.0.store(true, SeqCst);
        }
    }

    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let host = crate::host::Host::start(client, &tokio::runtime::Handle::current());
    let dropped = Arc::new(AtomicBool::new(false));
    let (started, ready) = tokio::sync::oneshot::channel();
    let pending = tokio::spawn({
        let host = host.clone();
        let dropped = dropped.clone();
        async move {
            host.run(move |client| {
                Box::pin(async move {
                    client.authorized = Some(Box::new(Answer(dropped)));
                    started.send(()).unwrap();
                    std::future::pending::<Result<()>>().await
                })
            })
            .await
        }
    });
    ready.await.unwrap();
    host.pause();
    assert_eq!(pending.await.unwrap(), Err(Error::Locked));
    host.resume_if(host.generation()).unwrap();
    assert!(
        host.run(|client| Box::pin(async move {
            Ok(client.authorized.is_none() && client.keys().is_ok())
        }))
        .await
        .unwrap()
    );
    assert!(dropped.load(SeqCst));
    host.close();
    host.closed().await.unwrap();
}

fn note(title: &str) -> Changes {
    Changes {
        title: Some(title.into()),
        ..Changes::default()
    }
}

#[tokio::test]
async fn legacy_pairing_requires_a_ready_account() {
    let dir = TempDir::new();
    let mut client = Client::open(crate::testing::config(false), &dir.0, None).unwrap();
    let kept = crate::login::kept_for_tests(61, now_ms() + 60_000);
    let delegated = kept.delegated().unwrap();
    let principal = delegated.principal;
    let ic = Ic::connect(&client.config, delegated.identity)
        .await
        .unwrap();
    client.login = Some(Login {
        ic,
        principal,
        expires_at: delegated.expires_at,
        kept,
    });
    client.select(Slot::Login(principal)).unwrap();
    client.keys = Some(client.workspace.unlock_provisional().unwrap());
    assert_eq!(client.status().unwrap().state, "no_account");
    assert_eq!(
        client.legacy_pair("https://dmsg.net").err(),
        Some(Error::RecoveryIncomplete)
    );
    let account = crate::testing::account();
    client.located = Some((account.home, account.id));
    assert_eq!(client.status().unwrap().state, "pairing_required");
    assert_eq!(
        client.legacy_pair("https://dmsg.net").err(),
        Some(Error::RecoveryIncomplete)
    );
    assert!(client.legacy_pairs().unwrap().is_empty());

    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    crate::testing::warm(&mut client);
    client.root = None;
    let status = client.status().unwrap();
    assert_eq!(status.state, "root_required");
    assert!(status.setup_pending);
    assert_eq!(
        client.legacy_pair("https://dmsg.net").err(),
        Some(Error::RecoveryIncomplete)
    );
    assert!(client.legacy_pairs().unwrap().is_empty());
    // The account may have a committed root that this device cannot open.
    // That calls for device approval/recovery, never initial setup.
    client
        .security
        .as_mut()
        .unwrap()
        .snapshot
        .content_root_digest = Some(Hash::new([1; 32]));
    assert!(!client.status().unwrap().setup_pending);
    client
        .security
        .as_mut()
        .unwrap()
        .snapshot
        .content_root_digest = None;
    client.load_root().await.unwrap();
    assert_eq!(client.status().unwrap().state, "unlocked");
    assert!(!client.status().unwrap().setup_pending);
    assert!(client.legacy_pair("https://dmsg.net").is_ok());
    client.lock();
    assert!(!client.status().unwrap().setup_pending);
    assert_eq!(
        client.legacy_pair("https://dmsg.net").err(),
        Some(Error::Locked)
    );
    assert_eq!(client.initialize_account().await.err(), Some(Error::Locked));
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

    // Offline: the stored root reads before the host starts its background sync.
    let status = client.unlock_fast().await.unwrap();
    assert_eq!(status.state, "unlocked");
    assert!(status.last_sync.is_none());
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

#[tokio::test]
async fn a_matching_local_root_opens_without_publishing_security_to_the_relay() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    crate::testing::warm(&mut client);
    let root = client.root.as_ref().unwrap();
    let state = client.security.as_mut().unwrap();
    state.snapshot.content_root_generation = root.generation;
    state.snapshot.content_root_digest = Some(root.digest);
    client.root = None;
    client.published = None;
    // Both endpoints are unreachable. Local data still opens, without
    // waiting for evidence publication, an export or an upload.
    client.load_root().await.unwrap();
    assert!(client.root.is_some());
    assert_eq!(client.published, None);
    assert!(client.last_sync().unwrap().is_none());
    // A different certified root must still be fetched and verified.
    client
        .security
        .as_mut()
        .unwrap()
        .snapshot
        .content_root_generation += 1;
    assert!(client.load_root().await.is_err());
}

/// A [`PresenceKey`] that answers as the person tells it to. Each key it
/// makes has its own secret: the first byte of the handle and peer.
#[derive(Default)]
struct Hardware {
    declines: std::sync::atomic::AtomicBool,
    forgot: std::sync::atomic::AtomicBool,
    /// The person cannot be asked now, such as without a Touch ID keyboard.
    away: std::sync::atomic::AtomicBool,
    opened: std::sync::atomic::AtomicU32,
    made: std::sync::atomic::AtomicU8,
    /// The enrolled fingerprints.
    fingerprints: std::sync::atomic::AtomicU8,
    create_fails: std::sync::atomic::AtomicBool,
    wrong_secret: std::sync::atomic::AtomicBool,
    no_presence: std::sync::atomic::AtomicBool,
}

impl PresenceKey for Hardware {
    fn presence(&self) -> Option<Presence> {
        (!self.no_presence.load(std::sync::atomic::Ordering::SeqCst))
            .then_some(Presence::Biometrics)
    }

    fn create(&self, _: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)> {
        ensure(
            !self.create_fails.load(std::sync::atomic::Ordering::SeqCst),
            Error::Unavailable("cannot create a key".into()),
        )?;
        let n = self.made.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 7;
        Ok((vec![n; 10], vec![n; 65], Zeroizing::new([n + 2; 32])))
    }

    fn open(&self, handle: &[u8], peer: &[u8], _: bool) -> Result<Zeroizing<Key>> {
        use std::sync::atomic::Ordering::SeqCst;
        assert_eq!((handle.len(), peer.len(), handle[0]), (10, 65, peer[0]));
        self.opened.fetch_add(1, SeqCst);
        match (
            self.declines.load(SeqCst),
            self.away.load(SeqCst),
            self.forgot.load(SeqCst),
        ) {
            (true, _, _) => Err(Error::Forbidden),
            (_, true, _) => Err(Error::Unavailable("not now".into())),
            (_, _, true) => Err(Error::IntegrityFailed),
            _ if self.wrong_secret.load(SeqCst) => Ok(Zeroizing::new([0; 32])),
            _ => Ok(Zeroizing::new([handle[0] + 2; 32])),
        }
    }

    fn biometry_state(&self) -> Option<Hash> {
        use std::sync::atomic::Ordering::SeqCst;
        (!self.away.load(SeqCst)).then(|| Hash::new([self.fingerprints.load(SeqCst); 32]))
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
async fn default_binding_requires_proven_hardware_before_releasing_content_keys() {
    use std::sync::atomic::Ordering::SeqCst;
    for fast_declined in [false, true] {
        let dir = TempDir::new();
        let storage = Arc::new(MemoryStorage::default());
        let hardware = Arc::new(Hardware::default());
        let mut client = unlocked(&dir.0, false, Some(storage.clone()));
        client.set_presence_key(hardware.clone());
        log_in(&mut client, 3, now_ms() + DAY).await;
        client.workspace.meta.fast_declined = fast_declined;
        let prepared = client
            .prepare_login_keys(Zeroizing::new(crate::testing::SECRET))
            .unwrap()
            .unwrap();
        assert!(prepared.initializing.is_some());
        assert_eq!(hardware.opened.load(SeqCst), 0);
        assert!(client.keys().is_err());
        assert!(client.root.is_none());
        assert!(
            storage
                .get(&client.bound_name().unwrap())
                .unwrap()
                .is_none()
        );
        let device = prepared.open().unwrap();
        client
            .apply_login_keys(prepared, &device, now_ms())
            .unwrap();
        client.load_root().await.unwrap();
        assert_eq!(client.session_state().state, "unlocked");
        assert!(client.workspace.bound().unwrap());
        assert_eq!(client.status().unwrap().fast_unlock, !fast_declined);
        assert!(client.workspace.unlock(&crate::testing::SECRET).is_err());
        assert!(
            storage
                .get(&client.bound_name().unwrap())
                .unwrap()
                .is_some()
        );
        assert!(storage.get(&client.fast_name().unwrap()).unwrap().is_none());

        // Later logins use the existing slot, with no new key or extra prompt.
        let made = hardware.made.load(SeqCst);
        let prepared = client
            .prepare_login_keys(Zeroizing::new(crate::testing::SECRET))
            .unwrap()
            .unwrap();
        assert!(prepared.initializing.is_none());
        let device = prepared.open().unwrap();
        client
            .apply_login_keys(prepared, &device, now_ms())
            .unwrap();
        assert_eq!(hardware.made.load(SeqCst), made);
        assert_eq!(hardware.opened.load(SeqCst), 2);
    }
}

#[tokio::test]
async fn default_binding_reuses_old_hardware_proof_instead_of_blessing_a_copy() {
    use std::sync::atomic::Ordering::SeqCst;
    for moved in [false, true] {
        let dir = TempDir::new();
        let storage = Arc::new(MemoryStorage::default());
        let hardware = Arc::new(Hardware::default());
        let mut client = unlocked(&dir.0, false, Some(storage.clone()));
        client.set_presence_key(hardware.clone());
        client.enable_fast_unlock().unwrap();
        client.workspace.meta.anchor = Some(crate::workspace::Anchor {
            handle: vec![1].into(),
            public: vec![2].into(),
            created_at: 1,
            late: true,
            foreign: false,
        });
        let made = hardware.made.load(SeqCst);
        let prepared = client
            .prepare_login_keys(Zeroizing::new(crate::testing::SECRET))
            .unwrap()
            .unwrap();
        assert!(matches!(
            prepared.initializing,
            Some(BindingInitialization::Fast)
        ));
        assert_eq!(hardware.made.load(SeqCst), made);
        hardware.forgot.store(moved, SeqCst);
        let key = client.binding_hardware_key(&prepared.presence.slot, prepared.open());
        if moved {
            assert_eq!(key.err(), Some(Error::Forbidden));
            assert!(client.workspace.meta.anchor.as_ref().unwrap().foreign);
            assert!(!client.workspace.bound().unwrap());
            assert!(client.keys().is_err());
            continue;
        }
        assert!(!client.workspace.meta.anchor.as_ref().unwrap().late);
        client
            .apply_login_keys(prepared, &key.unwrap(), now_ms())
            .unwrap();
        assert!(client.workspace.bound().unwrap());
        assert!(storage.get(&client.fast_name().unwrap()).unwrap().is_none());
        assert!(client.workspace.unlock(&crate::testing::SECRET).is_err());
    }
}

#[tokio::test]
async fn a_new_binding_key_cannot_settle_an_older_late_anchor() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage));
    client.set_presence_key(Arc::new(Hardware::default()));
    client.workspace.meta.anchor = Some(crate::workspace::Anchor {
        handle: vec![1].into(),
        public: vec![2].into(),
        created_at: 1,
        late: true,
        foreign: false,
    });
    let prepared = client
        .prepare_login_keys(Zeroizing::new(crate::testing::SECRET))
        .unwrap()
        .unwrap();
    let key = client
        .binding_hardware_key(&prepared.presence.slot, prepared.open())
        .unwrap();
    assert!(client.workspace.meta.anchor.as_ref().unwrap().late);
    client.apply_login_keys(prepared, &key, now_ms()).unwrap();
    client.lock();
    assert_eq!(client.unlock_fast().await.unwrap().state, "unlocked");
    assert!(client.workspace.meta.anchor.as_ref().unwrap().late);
}

#[tokio::test]
async fn default_binding_failure_never_falls_back_to_login_only_keys() {
    use std::sync::atomic::Ordering::SeqCst;
    for failure in ["cancel", "unavailable", "lost", "mismatch", "creation"] {
        let dir = TempDir::new();
        let storage = Arc::new(MemoryStorage::default());
        let hardware = Arc::new(Hardware::default());
        let mut client = unlocked(&dir.0, false, Some(storage.clone()));
        client.set_presence_key(hardware.clone());
        log_in(&mut client, 3, now_ms() + DAY).await;
        hardware.declines.store(failure == "cancel", SeqCst);
        hardware.away.store(failure == "unavailable", SeqCst);
        hardware.forgot.store(failure == "lost", SeqCst);
        hardware.wrong_secret.store(failure == "mismatch", SeqCst);
        hardware.create_fails.store(failure == "creation", SeqCst);
        let prepared = client.prepare_login_keys(Zeroizing::new(crate::testing::SECRET));
        if failure == "creation" {
            assert!(matches!(prepared, Err(Error::Unavailable(_))));
        } else {
            let prepared = prepared.unwrap().unwrap();
            let key = prepared.open();
            assert!(key.is_err());
            // Rejection happens before any network authorization or commit.
            assert!(client.complete_login_unlock(prepared, key).await.is_err());
        }
        assert!(client.keys().is_err());
        assert!(client.root.is_none());
        assert!(!client.workspace.bound().unwrap());
        assert!(!client.workspace.meta.bound_declined);
        assert!(
            storage
                .get(&client.bound_name().unwrap())
                .unwrap()
                .is_none()
        );
        assert_eq!(client.prepare_fast_unlock().err(), Some(Error::NotFound));
    }
}

#[tokio::test]
async fn default_binding_rejects_late_or_mismatched_hardware_answers() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(Arc::new(Hardware::default()));
    log_in(&mut client, 3, now_ms() + DAY).await;
    let prepared = client
        .prepare_login_keys(Zeroizing::new(crate::testing::SECRET))
        .unwrap()
        .unwrap();
    assert_eq!(
        client.apply_login_keys(prepared, &[0; 32], now_ms()),
        Err(Error::IntegrityFailed)
    );
    assert!(client.keys().is_err());
    let prepared = client
        .prepare_login_keys(Zeroizing::new(crate::testing::SECRET))
        .unwrap()
        .unwrap();
    let key = prepared.open();
    client.lock();
    assert_eq!(
        client.complete_login_unlock(prepared, key).await.err(),
        Some(Error::Locked)
    );
    assert!(!client.workspace.bound().unwrap());
    assert!(
        storage
            .get(&client.bound_name().unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn default_binding_does_not_treat_temporarily_unavailable_enclave_as_unsupported() {
    use std::sync::atomic::Ordering::SeqCst;
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage));
    client.set_presence_key(hardware.clone());
    client.workspace.meta.anchor = Some(crate::workspace::Anchor {
        handle: vec![1].into(),
        public: vec![2].into(),
        created_at: 1,
        late: false,
        foreign: false,
    });
    hardware.no_presence.store(true, SeqCst);
    hardware.create_fails.store(true, SeqCst);
    assert!(matches!(
        client.prepare_login_keys(Zeroizing::new(crate::testing::SECRET)),
        Err(Error::Unavailable(_))
    ));
    assert!(client.keys().is_err());
    assert!(!client.workspace.meta.bound_declined);
}

#[test]
fn default_binding_respects_a_persisted_explicit_unbind() {
    use std::sync::atomic::Ordering::SeqCst;
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(hardware.clone());
    bind_offline(&mut client, &hardware, &storage, false);
    let keys = client.keys.take().unwrap();
    client
        .workspace
        .unbind_device(&keys, &crate::testing::SECRET)
        .unwrap();
    storage.delete(&client.bound_name().unwrap()).unwrap();
    drop(client);
    let mut client = Client::open(crate::testing::config(false), &dir.0, Some(storage)).unwrap();
    client.set_presence_key(hardware.clone());
    let made = hardware.made.load(SeqCst);
    assert!(client.workspace.meta.bound_declined);
    assert!(
        client
            .prepare_login_keys(Zeroizing::new(crate::testing::SECRET))
            .unwrap()
            .is_none()
    );
    // Fast unlock may create its own key; automatic binding remains off.
    assert!(!client.workspace.bound().unwrap());
    assert_eq!(hardware.made.load(SeqCst), made + 1);
    client.workspace.unlock(&crate::testing::SECRET).unwrap();
}

#[tokio::test]
async fn old_unbound_fast_unlock_requires_login_before_hardware_migration() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage));
    client.enable_fast_unlock().unwrap();
    client.lock();
    let prepared = client.prepare_fast_unlock().unwrap();
    let key = prepared.open();
    client.set_presence_key(Arc::new(Hardware::default()));
    assert_eq!(
        client.prepare_fast_unlock().err(),
        Some(Error::AuthRequired)
    );
    assert_eq!(
        client.complete_fast_unlock(prepared, key).await.err(),
        Some(Error::AuthRequired)
    );
    assert!(client.keys().is_err());
}

#[test]
fn default_binding_keeps_compatibility_without_hardware_or_a_credential_store() {
    for has_store in [false, true] {
        let dir = TempDir::new();
        let storage =
            has_store.then(|| Arc::new(MemoryStorage::default()) as Arc<dyn SecureStorage>);
        let mut client = unlocked(&dir.0, false, storage);
        if !has_store {
            client.set_presence_key(Arc::new(Hardware::default()));
        }
        assert!(
            client
                .prepare_login_keys(Zeroizing::new(crate::testing::SECRET))
                .unwrap()
                .is_none()
        );
        assert!(!client.workspace.bound().unwrap());
        client.workspace.unlock(&crate::testing::SECRET).unwrap();
    }
}

#[tokio::test]
async fn fast_unlock_with_the_person_restores_the_login() {
    use std::sync::atomic::Ordering::SeqCst;
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    // Hardware fast unlock is still available after an explicit device unbind.
    client.workspace.meta.bound_declined = true;
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

/// Bind `client`'s workspace to a new key of `hardware`, as
/// [`Client::bind_device`] does once the user home answered.
fn bind_offline(client: &mut Client, hardware: &Hardware, storage: &MemoryStorage, biometry: bool) {
    let (handle, peer, secret) = hardware.create(biometry).unwrap();
    let slot = BoundSlot {
        handle: handle.into(),
        peer: peer.into(),
        biometry: biometry.then(|| hardware.biometry_state().unwrap()),
        created_at: 0,
    };
    storage
        .set(&client.bound_name().unwrap(), &canonical(&slot))
        .unwrap();
    let keys = client.keys.take().unwrap();
    client
        .workspace
        .bind_device(&keys, &crate::testing::SECRET, &secret, slot.id(), now_ms())
        .unwrap();
    client.keys = Some(keys);
}

#[tokio::test]
async fn a_committed_binding_keeps_its_hardware_slot_when_wal_cleanup_is_blocked() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(hardware.clone());
    let (handle, peer, secret) = hardware.create(false).unwrap();
    let slot = BoundSlot {
        handle: handle.into(),
        peer: peer.into(),
        biometry: None,
        created_at: 0,
    };
    let name = client.bound_name().unwrap();
    let reader = rusqlite::Connection::open(client.slot_dir(&client.slot).join(WORKSPACE)).unwrap();
    reader
        .execute_batch("BEGIN; SELECT value FROM sealed;")
        .unwrap();
    assert!(matches!(
        client.bind_device_keys(slot, &crate::testing::SECRET, &secret, now_ms()),
        Err(Error::Unavailable(_))
    ));
    assert!(client.workspace.bound().unwrap());
    assert!(storage.get(&name).unwrap().is_some());
    assert!(client.keys().is_err());
    assert!(client.root.is_none());
    reader.execute_batch("ROLLBACK").unwrap();
    // The same slot recovers the committed LDK and content after retrying
    // cleanup. Deleting it on the earlier error would lose this workspace.
    assert_eq!(client.unlock_fast().await.unwrap().state, "unlocked");
}

#[test]
fn failed_fast_unlock_cleanup_locks_the_client_until_it_can_retry() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.enable_fast_unlock().unwrap();
    let reader = rusqlite::Connection::open(client.slot_dir(&client.slot).join(WORKSPACE)).unwrap();
    reader
        .execute_batch("BEGIN; SELECT value FROM sealed;")
        .unwrap();
    assert!(matches!(
        client.disable_fast_unlock(),
        Err(Error::Unavailable(_))
    ));
    assert!(client.keys().is_err());
    assert!(client.root.is_none());
    assert!(storage.get(&client.fast_name().unwrap()).unwrap().is_none());
    reader.execute_batch("ROLLBACK").unwrap();
    let status = client.disable_fast_unlock().unwrap();
    assert!(!status.fast_unlock);
    client.workspace.unlock(&crate::testing::SECRET).unwrap();
}

#[test]
fn a_rolled_back_binding_discards_only_its_unused_hardware_slot() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(hardware.clone());
    let (handle, peer, secret) = hardware.create(false).unwrap();
    let slot = BoundSlot {
        handle: handle.into(),
        peer: peer.into(),
        biometry: None,
        created_at: 0,
    };
    let name = client.bound_name().unwrap();
    let database =
        rusqlite::Connection::open(client.slot_dir(&client.slot).join(WORKSPACE)).unwrap();
    database.execute_batch("CREATE TRIGGER reject_binding BEFORE INSERT ON sealed WHEN NEW.key = 'ldk-bound' BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    assert!(matches!(
        client.bind_device_keys(slot, &crate::testing::SECRET, &secret, now_ms()),
        Err(Error::Unavailable(_))
    ));
    assert!(!client.workspace.bound().unwrap());
    assert!(storage.get(&name).unwrap().is_none());
    assert!(client.keys().is_err());
    // The original login wrapping remains usable after the rollback.
    client.workspace.unlock(&crate::testing::SECRET).unwrap();
}

#[tokio::test]
async fn a_device_bound_workspace_keeps_its_slot_through_failures_and_logout() {
    use std::sync::atomic::Ordering::SeqCst;
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(hardware.clone());
    log_in(&mut client, 3, now_ms() + DAY).await;
    client.after_login_unlock().unwrap();
    let fast_name = client.fast_name().unwrap();
    bind_offline(&mut client, &hardware, &storage, false);
    // What binding leaves: the slot, and fast unlock through it alone.
    storage.delete(&fast_name).unwrap();
    let bound_name = client.bound_name().unwrap();
    let status = client.status().unwrap();
    assert!(status.device_bound && status.fast_unlock);

    // Fast unlock asks the hardware for the slot's secret.
    client.lock();
    let opened = hardware.opened.load(SeqCst);
    assert_eq!(client.unlock_fast().await.unwrap().state, "unlocked");
    assert_eq!(hardware.opened.load(SeqCst), opened + 1);

    // Declined, unavailable now, or even lost: nothing is deleted and
    // nothing falls back.
    for flag in [&hardware.declines, &hardware.away, &hardware.forgot] {
        client.lock();
        flag.store(true, SeqCst);
        assert!(client.unlock_fast().await.is_err());
        flag.store(false, SeqCst);
        assert!(storage.get(&bound_name).unwrap().is_some());
        let status = client.status().unwrap();
        assert!(status.device_bound && status.fast_unlock);
    }

    // Fast unlock off and on again, then a logout: the slot stays.
    client.unlock_fast().await.unwrap();
    assert!(!client.disable_fast_unlock().unwrap().fast_unlock);
    assert!(client.enable_fast_unlock().unwrap().fast_unlock);
    assert!(storage.get(&fast_name).unwrap().is_none());
    client.logout().unwrap();
    assert!(storage.get(&bound_name).unwrap().is_some());
}

#[tokio::test]
async fn a_fingerprint_only_slot_is_lost_once_the_fingerprints_change() {
    use std::sync::atomic::Ordering::SeqCst;
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let hardware = Arc::new(Hardware::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    client.set_presence_key(hardware.clone());
    bind_offline(&mut client, &hardware, &storage, true);
    let name = client.bound_name().unwrap();
    let slot = storage.get(&name).unwrap().unwrap();
    client.lock();
    // Without its Touch ID keyboard the key is only unavailable now.
    hardware.away.store(true, SeqCst);
    assert!(hardware.biometry_state().is_none());
    assert!(matches!(
        client.unlock_fast().await,
        Err(Error::Unavailable(_))
    ));
    assert_eq!(storage.get(&name).unwrap().unwrap(), slot);
    assert!(client.status().unwrap().fast_unlock);
    assert!(client.workspace.bound().unwrap());
    // Reconnecting restores fast unlock with the original slot.
    hardware.away.store(false, SeqCst);
    assert_eq!(client.unlock_fast().await.unwrap().state, "unlocked");
    client.lock();
    // Other fingerprints: the key is gone for good, and still nothing
    // is deleted.
    hardware.fingerprints.store(1, SeqCst);
    hardware.forgot.store(true, SeqCst);
    assert_eq!(
        client.unlock_fast().await.err(),
        Some(Error::IntegrityFailed)
    );
    assert_eq!(storage.get(&name).unwrap().unwrap(), slot);
    assert!(client.workspace.bound().unwrap());
}
