use super::*;
use crate::testing::{SECRET, TempDir, account, config, unlocked};
use crate::workspace::AccountRef;
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::SeqCst};

/// The anchors of the hardware numbered `id`: each key is a software P-256
/// key named by its handle, which only this hardware uses.
#[derive(Default)]
struct Enclave {
    id: u8,
    /// The hardware cannot answer now, such as before the first unlock.
    away: AtomicBool,
    /// The hardware makes no anchors.
    absent: bool,
    platform: Option<Hash>,
    made: AtomicU8,
}

impl Enclave {
    fn new(id: u8) -> Arc<Self> {
        Arc::new(Self {
            id,
            ..Default::default()
        })
    }
}

fn signing(handle: &[u8]) -> SigningKey {
    SigningKey::from_slice(&[handle[0], handle[1]].repeat(16)).unwrap()
}

impl HardwareAnchor for Enclave {
    fn platform(&self) -> Option<Hash> {
        self.platform
    }

    fn create(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        ensure(!self.absent, Error::Unavailable("no enclave".into()))?;
        let handle = vec![self.id, self.made.fetch_add(1, SeqCst) + 1];
        let public = signing(&handle).verifying_key().to_sec1_point(true);
        Ok((handle, public.as_bytes().to_vec()))
    }

    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]> {
        ensure(
            !self.away.load(SeqCst),
            Error::Unavailable("not now".into()),
        )?;
        ensure(handle.first() == Some(&self.id), Error::IntegrityFailed)?;
        let signature: Signature = signing(handle).sign(message.as_slice());
        Ok(signature.to_bytes().into())
    }
}

/// The test account's workspace made on hardware 1, then opened on
/// `hardware`, locked.
fn copied(dir: &Path, hardware: Arc<Enclave>) -> Client {
    let mut client = unlocked(dir, false, None);
    client.set_hardware_anchor(Enclave::new(1)).unwrap();
    drop(client);
    let mut client = Client::open(config(false), dir, None).unwrap();
    client.set_hardware_anchor(hardware).unwrap();
    client
}

#[test]
fn a_workspace_gets_one_anchor_and_stays_native_on_its_hardware() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let enclave = Enclave::new(1);
    client.set_hardware_anchor(enclave.clone()).unwrap();
    let anchor = client.workspace.meta.anchor.clone().unwrap();
    assert!(!anchor.late && !anchor.foreign);
    assert_eq!(client.session_state().state, "unlocked");
    drop(client);

    let mut client = Client::open(config(false), &dir.0, None).unwrap();
    client.set_hardware_anchor(enclave.clone()).unwrap();
    assert_eq!(client.workspace.meta.anchor, Some(anchor));
    assert_eq!(enclave.made.load(SeqCst), 1);
    assert_eq!(client.session_state().state, "locked");
    assert_eq!(client.prepare_fast_unlock().err(), Some(Error::NotFound));
    assert!(!client.accounts().unwrap()[0].moved);

    // Hardware that makes no anchor protects nothing and blocks nothing.
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let absent = Arc::new(Enclave {
        absent: true,
        ..Default::default()
    });
    client.set_hardware_anchor(absent).unwrap();
    assert!(client.workspace.meta.anchor.is_none());
    client.lock();
    assert_eq!(client.prepare_fast_unlock().err(), Some(Error::NotFound));
}

#[tokio::test]
async fn a_copy_on_other_hardware_never_unlocks() {
    let dir = TempDir::new();
    let mut client = copied(&dir.0, Enclave::new(2));
    let status = client.status().unwrap();
    assert_eq!(status.state, "moved");
    assert_eq!(status.account, Some(account().id.to_string()));
    assert!(client.accounts().unwrap()[0].moved);
    // Refused before any request: the endpoints are unreachable, so a
    // request would have failed as unavailable.
    assert_eq!(client.prepare_fast_unlock().err(), Some(Error::Forbidden));
    assert_eq!(client.prepare_unlock().await.err(), Some(Error::Forbidden));
    assert_eq!(client.reset().err(), Some(Error::Forbidden));
    assert!(client.keys().is_err());
    assert!(client.workspace.meta.account.is_some());
}

#[test]
fn hardware_that_cannot_answer_changes_nothing() {
    let dir = TempDir::new();
    let enclave = Enclave::new(1);
    enclave.away.store(true, SeqCst);
    let mut client = copied(&dir.0, enclave.clone());
    let meta = client.workspace.meta.clone();
    assert_eq!(client.session_state().state, "locked");
    assert!(matches!(
        client.prepare_fast_unlock(),
        Err(Error::Unavailable(_))
    ));
    assert_eq!(client.workspace.meta, meta);
    enclave.away.store(false, SeqCst);
    assert_eq!(client.prepare_fast_unlock().err(), Some(Error::NotFound));
}

#[test]
fn setting_a_copy_aside_keeps_it_and_starts_a_new_device() {
    let dir = TempDir::new();
    let mut client = copied(&dir.0, Enclave::new(2));
    let old = client.workspace.meta.clone();
    assert!(client.moved_copies().unwrap().is_empty());

    let status = client.set_aside_moved().unwrap();
    assert_eq!(status.state, "logged_out");
    assert_ne!(status.device_id, Some(hex::encode(*old.device_id)));
    assert!(client.workspace.provisional());
    let anchor = client.workspace.meta.anchor.as_ref().unwrap();
    assert_eq!(anchor.handle[0], 2);
    let local = &client.accounts().unwrap()[0];
    assert!(!local.moved && !local.bound && local.current);

    let copies = client.moved_copies().unwrap();
    assert_eq!(copies.len(), 1);
    let copy = &copies[0];
    assert_eq!(copy.account, Some(account().id.to_string()));
    assert_eq!(copy.device_id, hex::encode(*old.device_id));
    assert_eq!(copy.reason, "anchor");
    let kept = dir.0.join(MOVED).join(&copy.id);
    let store = Store::open(&kept.join(crate::client::WORKSPACE)).unwrap();
    assert_eq!(store.meta::<Meta>(META).unwrap(), Some(old));
    // A native workspace is not set aside.
    assert_eq!(client.set_aside_moved().err(), Some(Error::Forbidden));

    for id in ["", ".", "..", "../accounts", "x/y"] {
        assert_eq!(client.discard_moved(id).err(), Some(Error::NotFound));
    }
    client.discard_moved(&copy.id).unwrap();
    assert!(!kept.exists());
    assert!(client.moved_copies().unwrap().is_empty());
}

#[test]
fn other_accounts_on_the_device_are_checked_too() {
    let dir = TempDir::new();
    let other = AccountRef {
        id: AccountId::from_str("080g2081040g2081040g").unwrap(),
        ..account()
    };
    let path = dir.0.join("accounts").join(other.id.to_string());
    std::fs::create_dir_all(&path).unwrap();
    let mut workspace = Workspace::open(
        &path.join(crate::client::WORKSPACE),
        &config(false).environment,
        now_ms(),
    )
    .unwrap();
    let keys = workspace.unlock_provisional().unwrap();
    workspace.bind(&keys, other.clone(), &SECRET).unwrap();
    drop(workspace);

    let client = copied(&dir.0, Enclave::new(2));
    let accounts = client.accounts().unwrap();
    assert_eq!(accounts.len(), 2);
    assert!(accounts.iter().all(|a| a.moved && a.bound));
    assert!(client.moved.contains(&other.id));
}

#[tokio::test]
async fn a_late_anchor_moves_on_the_first_hardware_key_this_hardware_cannot_use() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    client.workspace.meta.fast_unlock = Some(now_ms());
    client.workspace.meta.fast_presence = true;
    client.workspace.save_meta().unwrap();
    client.set_hardware_anchor(Enclave::new(1)).unwrap();
    assert!(client.workspace.meta.anchor.as_ref().unwrap().late);

    // Declining says nothing about the hardware.
    assert_eq!(
        client.hardware_key::<()>(Err(Error::Forbidden)),
        Err(Error::Forbidden)
    );
    assert!(client.workspace.meta.anchor.as_ref().unwrap().late);
    assert_eq!(
        client.hardware_key::<()>(Err(Error::IntegrityFailed)),
        Err(Error::Forbidden)
    );
    assert_eq!(client.session_state().state, "moved");
    assert!(client.keys().is_err());
    assert!(!client.session_state().resident);
    drop(client);

    let mut client = Client::open(config(false), &dir.0, None).unwrap();
    client.set_hardware_anchor(Enclave::new(1)).unwrap();
    assert_eq!(client.session_state().state, "moved");
    assert_eq!(client.prepare_unlock().await.err(), Some(Error::Forbidden));
    client.set_aside_moved().unwrap();
    assert_eq!(client.moved_copies().unwrap()[0].reason, "hardware_key");

    // The first key that opens settles the workspace as native.
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    client.workspace.meta.fast_unlock = Some(now_ms());
    client.workspace.meta.fast_presence = true;
    client.set_hardware_anchor(Enclave::new(1)).unwrap();
    assert_eq!(client.hardware_key(Ok(7)), Ok(7));
    let anchor = client.workspace.meta.anchor.as_ref().unwrap();
    assert!(!anchor.late && !anchor.foreign);
    assert_eq!(
        client.hardware_key::<()>(Err(Error::IntegrityFailed)),
        Err(Error::IntegrityFailed)
    );
    assert_eq!(client.session_state().state, "unlocked");
}

#[test]
fn a_mac_without_an_enclave_detects_a_copy_by_platform() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let platform = |n| {
        Arc::new(Enclave {
            absent: true,
            platform: Some(Hash::new([n; 32])),
            ..Default::default()
        })
    };
    client.set_hardware_anchor(platform(1)).unwrap();
    assert!(client.workspace.meta.anchor.is_none());
    assert!(client.workspace.meta.platform.is_some());
    drop(client);
    let mut client = Client::open(config(false), &dir.0, None).unwrap();
    client.set_hardware_anchor(platform(1)).unwrap();
    assert_eq!(client.hardware, Verdict::Native);
    client.set_hardware_anchor(platform(2)).unwrap();
    assert_eq!(client.hardware, Verdict::Moved);
    assert_eq!(client.prepare_fast_unlock().err(), Some(Error::Forbidden));
}

#[test]
fn moved_and_unverified_workspaces_cannot_use_resident_agent_keys() {
    let dir = TempDir::new();
    let secure = Arc::new(crate::testing::MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(secure.clone()));
    let agent = client.agent_create("resident").unwrap();
    client.agent_resident(&agent.agent_id, true).unwrap();
    client.set_hardware_anchor(Enclave::new(1)).unwrap();
    drop(client);
    let mut client = Client::open(config(false), &dir.0, Some(secure)).unwrap();
    let native = Enclave::new(1);
    native.away.store(true, SeqCst);
    client.set_hardware_anchor(native.clone()).unwrap();
    assert_eq!(
        client.agent_signer(&agent.agent_id).err(),
        Some(Error::Locked)
    );
    native.away.store(false, SeqCst);
    client.check_hardware().unwrap();
    assert!(client.agent_signer(&agent.agent_id).is_ok());
    client.set_hardware_anchor(Enclave::new(2)).unwrap();
    assert_eq!(
        client.agent_signer(&agent.agent_id).err(),
        Some(Error::Locked)
    );
    assert_eq!(client.agent_mail_keys(&agent.agent_id), Err(Error::Locked));
}

#[test]
fn sealed_approval_and_cosigner_keys_also_mark_an_anchor_as_late() {
    for name in ["approval-keys", "wallet-cosigners"] {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, true, None);
        client
            .workspace
            .put_private(client.keys().unwrap(), name, &vec![1u8])
            .unwrap();
        client.set_hardware_anchor(Enclave::new(1)).unwrap();
        assert!(client.workspace.meta.anchor.as_ref().unwrap().late);
        assert_eq!(
            client.hardware_key::<()>(Err(Error::IntegrityFailed)),
            Err(Error::Forbidden)
        );
        assert!(client.keys().is_err());
        assert!(!client.session_state().resident);
    }
}
