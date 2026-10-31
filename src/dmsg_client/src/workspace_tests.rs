use super::*;
use crate::testing::TempDir;
use dmsg_types::AccountId;
use std::str::FromStr;

fn account() -> AccountRef {
    AccountRef {
        id: AccountId::from_str("040g2081040g2081040g").unwrap(),
        home: Principal::from_text("aaaaa-aa").unwrap(),
        issuer: "https://dmsg.test/u/040g2081040g2081040g".into(),
    }
}

fn disk_contains(dir: &TempDir, bytes: &[u8]) -> bool {
    ["workspace.sqlite", "workspace.sqlite-wal"]
        .iter()
        .filter_map(|name| match std::fs::read(dir.path(name)) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!("read test database: {error}"),
        })
        .any(|file| file.windows(bytes.len()).any(|window| window == bytes))
}

#[test]
fn binding_erases_provisional_material_from_database_and_wal() {
    let dir = TempDir::new();
    let mut ws = Workspace::open(&dir.path("workspace.sqlite"), &Environment::Local, 1).unwrap();
    let keys = ws.unlock_provisional().unwrap();
    let provisional: ByteArray<32> = ws.store.meta(PROVISIONAL).unwrap().unwrap();
    let wrapped = ws.store.sealed(LDK).unwrap().unwrap();
    assert!(disk_contains(&dir, provisional.as_slice()));
    assert!(disk_contains(&dir, wrapped.as_bytes()));

    ws.bind(&keys, account(), &[9; 32]).unwrap();
    // Keep the connection open, as when an existing account admits a new
    // device. SQL deletion alone must not leave the bootstrap in old WAL pages.
    ws.put_private(&keys, "root", &ByteArray::new([73; 32]))
        .unwrap();
    assert!(!disk_contains(&dir, provisional.as_slice()));
    assert!(!disk_contains(&dir, wrapped.as_bytes()));
    let again = ws.unlock(&[9; 32]).unwrap();
    assert_eq!(
        ws.private::<ByteArray<32>>(&again, "root").unwrap(),
        Some(ByteArray::new([73; 32]))
    );
}

#[test]
fn hardware_binding_erases_old_login_and_fast_wrappings_from_database_and_wal() {
    let dir = TempDir::new();
    let mut ws = Workspace::open(&dir.path("workspace.sqlite"), &Environment::Local, 1).unwrap();
    let keys = ws.unlock_provisional().unwrap();
    ws.bind(&keys, account(), &[9; 32]).unwrap();
    ws.enable_fast(&keys, &[3; 32], 2, false).unwrap();
    let login = ws.store.sealed(LDK).unwrap().unwrap();
    let fast = ws.store.sealed(FAST_LDK).unwrap().unwrap();
    let slot = Hash::new([6; 32]);
    ws.bind_device(&keys, &[9; 32], &[4; 32], slot, 3).unwrap();
    assert!(!disk_contains(&dir, login.as_bytes()));
    assert!(!disk_contains(&dir, fast.as_bytes()));
    assert!(ws.unlock(&[9; 32]).is_err());
    assert!(ws.unlock_fast(&[3; 32]).is_err());
    assert_eq!(
        ws.unlock_bound(&[9; 32], &[4; 32], slot)
            .unwrap()
            .ldk
            .as_slice(),
        keys.ldk.as_slice()
    );
    ws.unlock_fast_bound(&[4; 32], slot).unwrap();
}

#[test]
fn replacing_and_removing_fast_wrappings_erases_old_copies() {
    let dir = TempDir::new();
    let mut ws = Workspace::open(&dir.path("workspace.sqlite"), &Environment::Local, 1).unwrap();
    let keys = ws.unlock_provisional().unwrap();
    ws.bind(&keys, account(), &[9; 32]).unwrap();
    ws.put_private(&keys, "note", &"kept").unwrap();
    ws.enable_fast(&keys, &[3; 32], 2, false).unwrap();
    let old = ws.store.sealed(FAST_LDK).unwrap().unwrap();
    ws.enable_fast(&keys, &[4; 32], 3, false).unwrap();
    assert!(!disk_contains(&dir, old.as_bytes()));
    let old = ws.store.sealed(FAST_LDK).unwrap().unwrap();
    ws.disable_fast().unwrap();
    assert!(!disk_contains(&dir, old.as_bytes()));

    let slot = Hash::new([6; 32]);
    ws.bind_device(&keys, &[9; 32], &[5; 32], slot, 4).unwrap();
    let old = ws.store.sealed(FAST_LDK).unwrap().unwrap();
    ws.enable_fast_bound(&keys, &[5; 32], slot, 5).unwrap();
    assert!(!disk_contains(&dir, old.as_bytes()));
    let bound = ws.store.sealed(BOUND_LDK).unwrap().unwrap();
    let fast = ws.store.sealed(FAST_LDK).unwrap().unwrap();
    ws.unbind_device(&keys, &[9; 32]).unwrap();
    assert!(!disk_contains(&dir, bound.as_bytes()));
    assert!(!disk_contains(&dir, fast.as_bytes()));
    let again = ws.unlock(&[9; 32]).unwrap();
    assert_eq!(
        ws.private::<String>(&again, "note").unwrap().as_deref(),
        Some("kept")
    );
}

#[test]
fn loading_an_older_workspace_erases_bootstrap_pages_without_closing_the_store() {
    let dir = TempDir::new();
    let ws = Workspace::open(&dir.path("workspace.sqlite"), &Environment::Local, 1).unwrap();
    let keys = ws.unlock_provisional().unwrap();
    let provisional: ByteArray<32> = ws.store.meta(PROVISIONAL).unwrap().unwrap();
    let old = ws.store.sealed(LDK).unwrap().unwrap();
    // The pre-fix binding transaction: current rows are safe, old WAL pages
    // still carry the provisional key and its wrapping of the same LDK.
    let mut meta = ws.meta.clone();
    meta.account = Some(account());
    let name = name_of(&meta).unwrap();
    let wrapped = seal(
        &crate::crypto::unlock_key(&[9; 32], &name),
        keys.ldk.as_slice(),
        &("dmsg/local-key/2", name.as_str()),
    );
    ws.store
        .atomic(|store| {
            store.set_sealed(LDK, &wrapped)?;
            store.delete_meta(PROVISIONAL)?;
            store.set_meta(META, &meta)
        })
        .unwrap();
    assert!(disk_contains(&dir, provisional.as_slice()));
    assert!(disk_contains(&dir, old.as_bytes()));
    let ws = Workspace::load(ws.store, &Environment::Local, 2).unwrap();
    assert!(!disk_contains(&dir, provisional.as_slice()));
    assert!(!disk_contains(&dir, old.as_bytes()));
    assert_eq!(
        ws.unlock(&[9; 32]).unwrap().ldk.as_slice(),
        keys.ldk.as_slice()
    );
}

#[test]
fn a_read_only_import_preserves_its_database_and_wal() {
    let dir = TempDir::new();
    let path = dir.path("workspace.sqlite");
    let mut ws = Workspace::open(&path, &Environment::Local, 1).unwrap();
    let keys = ws.unlock_provisional().unwrap();
    ws.bind(&keys, account(), &[9; 32]).unwrap();
    ws.put_private(&keys, "note", &"import me").unwrap();
    let database = std::fs::read(&path).unwrap();
    let wal = std::fs::read(dir.path("workspace.sqlite-wal")).unwrap();
    assert!(!wal.is_empty());
    let copy = Workspace::load(Store::read_only(&path).unwrap(), &Environment::Local, 2).unwrap();
    let keys = copy.unlock(&[9; 32]).unwrap();
    assert_eq!(
        copy.private::<String>(&keys, "note").unwrap().as_deref(),
        Some("import me")
    );
    assert_eq!(std::fs::read(&path).unwrap(), database);
    assert_eq!(
        std::fs::read(dir.path("workspace.sqlite-wal")).unwrap(),
        wal
    );
    assert!(copy.put_private(&keys, "note", &"not allowed").is_err());
}

#[test]
fn blocked_binding_cleanup_fails_and_unlock_retries_the_committed_change() {
    let dir = TempDir::new();
    let path = dir.path("workspace.sqlite");
    let mut ws = Workspace::open(&path, &Environment::Local, 1).unwrap();
    let keys = ws.unlock_provisional().unwrap();
    let provisional: ByteArray<32> = ws.store.meta(PROVISIONAL).unwrap().unwrap();
    let reader = rusqlite::Connection::open(&path).unwrap();
    reader
        .execute_batch("BEGIN; SELECT value FROM meta;")
        .unwrap();
    assert!(matches!(
        ws.bind(&keys, account(), &[9; 32]),
        Err(Error::Unavailable(_))
    ));
    // COMMIT succeeded: metadata must agree, and failure must not be reported
    // as a successful erasure or allow another unlock before cleanup.
    assert!(!ws.provisional());
    assert!(
        ws.store
            .meta::<ByteArray<32>>(PROVISIONAL)
            .unwrap()
            .is_none()
    );
    assert!(disk_contains(&dir, provisional.as_slice()));
    assert!(matches!(ws.unlock(&[9; 32]), Err(Error::Unavailable(_))));
    reader.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        ws.unlock(&[9; 32]).unwrap().ldk.as_slice(),
        keys.ldk.as_slice()
    );
    assert!(!disk_contains(&dir, provisional.as_slice()));
}

#[test]
fn provisional_workspace_binds_to_the_unlock_secret() {
    let mut ws = Workspace::load(Store::memory().unwrap(), &Environment::Local, 1).unwrap();
    assert!(ws.provisional());
    assert_eq!(ws.name(), Err(Error::RecoveryIncomplete));
    let keys = ws.unlock_provisional().unwrap();
    assert_eq!(signing_public(&keys.signing), *ws.meta.signing_pub);
    ws.put_private(&keys, "note", &"before binding").unwrap();

    let secret = [9u8; 32];
    ws.bind(&keys, account(), &secret).unwrap();
    assert_eq!(
        ws.name().unwrap(),
        format!(
            "dmsg:local:040g2081040g2081040g:{}",
            hex::encode(*ws.meta.device_id)
        )
    );
    assert_eq!(ws.unlock_provisional().err(), Some(Error::Forbidden));
    assert!(
        ws.store
            .meta::<ByteArray<32>>(PROVISIONAL)
            .unwrap()
            .is_none()
    );
    assert!(matches!(ws.unlock(&[8; 32]), Err(Error::IntegrityFailed)));

    let again = ws.unlock(&secret).unwrap();
    assert_eq!(*again.hpke, *keys.hpke);
    assert_eq!(
        ws.private::<String>(&again, "note").unwrap().as_deref(),
        Some("before binding")
    );
    // The LUK follows the public formula, so the wrapped LDK opens with it.
    let luk = crate::crypto::unlock_key(&secret, &ws.name().unwrap());
    let ldk = open(
        &luk,
        &ws.store.sealed(LDK).unwrap().unwrap(),
        &("dmsg/local-key/2", ws.name().unwrap()),
    )
    .unwrap();
    assert_eq!(ldk.as_slice(), again.ldk.as_slice());
}

#[test]
fn reopening_keeps_the_device() {
    let store = Store::memory().unwrap();
    let ws = Workspace::load(store, &Environment::Local, 1).unwrap();
    let meta = ws.meta.clone();
    let reopened = Workspace::load(ws.store, &Environment::Local, 2).unwrap();
    assert_eq!(reopened.meta, meta);
    assert!(Workspace::load(reopened.store, &Environment::Staging, 3).is_err());
}

#[test]
fn administrators_manage_the_root() {
    let ws = Workspace::load(Store::memory().unwrap(), &Environment::Local, 1).unwrap();
    let admin = ws.device_input(ControllerRole::Administrator);
    assert_eq!(
        admin.capabilities,
        vec![
            Capability::ContentSign,
            Capability::VaultUnlock,
            Capability::RootManage
        ]
    );
    let member = ws.device_input(ControllerRole::Member);
    assert_eq!(
        member.capabilities,
        vec![Capability::ContentSign, Capability::VaultUnlock]
    );
}

#[test]
fn fast_unlock_opens_a_second_copy() {
    let mut ws = Workspace::load(Store::memory().unwrap(), &Environment::Local, 1).unwrap();
    let keys = ws.unlock_provisional().unwrap();
    assert_eq!(
        ws.enable_fast(&keys, &[4; 32], 2, false).err(),
        Some(Error::RecoveryIncomplete)
    );
    ws.bind(&keys, account(), &[9; 32]).unwrap();
    assert_eq!(ws.unlock_fast(&[4; 32]).err(), Some(Error::NotFound));
    ws.enable_fast(&keys, &[4; 32], 2, false).unwrap();
    assert_eq!(ws.meta.fast_unlock, Some(2));
    let fast = ws.unlock_fast(&[4; 32]).unwrap();
    assert_eq!(*fast.signing, *keys.signing);
    assert_eq!(fast.ldk.as_slice(), keys.ldk.as_slice());
    assert!(matches!(
        ws.unlock_fast(&[5; 32]),
        Err(Error::IntegrityFailed)
    ));
    // The login path is untouched.
    ws.unlock(&[9; 32]).unwrap();
    ws.disable_fast().unwrap();
    assert_eq!(ws.unlock_fast(&[4; 32]).err(), Some(Error::NotFound));
    assert!(ws.store.sealed(FAST_LDK).unwrap().is_none());
}

#[test]
fn a_device_bound_workspace_needs_the_hardware_secret_on_every_unlock() {
    let mut ws = Workspace::load(Store::memory().unwrap(), &Environment::Local, 1).unwrap();
    let keys = ws.unlock_provisional().unwrap();
    let secret = [9u8; 32];
    ws.bind(&keys, account(), &secret).unwrap();
    let keys = ws.unlock(&secret).unwrap();
    ws.put_private(&keys, "note", &"kept").unwrap();
    ws.enable_fast(&keys, &[3; 32], 2, true).unwrap();
    let (device, slot) = ([4u8; 32], Hash::new([6; 32]));
    ws.bind_device(&keys, &secret, &device, slot, 3).unwrap();
    assert!(ws.bound().unwrap());
    // No copy opens without the hardware secret, the old fast key's neither.
    assert!(ws.store.sealed(LDK).unwrap().is_none());
    assert!(matches!(ws.unlock(&secret), Err(Error::IntegrityFailed)));
    assert!(ws.unlock_fast(&[3; 32]).is_err());
    assert!(ws.unlock_bound(&secret, &[5; 32], slot).is_err());
    assert!(
        ws.unlock_bound(&secret, &device, Hash::new([7; 32]))
            .is_err()
    );
    assert!(ws.unlock_bound(&[8; 32], &device, slot).is_err());
    // The same LDK: what it sealed before still opens.
    let again = ws.unlock_bound(&secret, &device, slot).unwrap();
    assert_eq!(
        ws.private::<String>(&again, "note").unwrap().as_deref(),
        Some("kept")
    );
    let fast = ws.unlock_fast_bound(&device, slot).unwrap();
    assert_eq!(*fast.signing, *keys.signing);
    assert_eq!(
        ws.bind_device(&keys, &secret, &device, slot, 4).err(),
        Some(Error::Forbidden)
    );
    // Turning fast unlock off deletes its copy alone.
    ws.disable_fast().unwrap();
    assert!(ws.unlock_fast_bound(&device, slot).is_err());
    ws.unlock_bound(&secret, &device, slot).unwrap();
    ws.enable_fast_bound(&keys, &device, slot, 5).unwrap();
    ws.unlock_fast_bound(&device, slot).unwrap();
    // Unbinding seals it under the LUK alone again.
    ws.unbind_device(&keys, &secret).unwrap();
    assert!(!ws.bound().unwrap());
    assert!(ws.unlock_bound(&secret, &device, slot).is_err());
    assert!(ws.meta.fast_unlock.is_none());
    let unbound = ws.unlock(&secret).unwrap();
    assert_eq!(
        ws.private::<String>(&unbound, "note").unwrap().as_deref(),
        Some("kept")
    );
}
