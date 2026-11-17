use super::*;
use crate::{
    secure::Authorized,
    testing::{TempDir, unlocked},
};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

#[derive(Default)]
struct Hardware {
    asked: AtomicUsize,
    deleted: std::sync::Mutex<Vec<Vec<u8>>>,
}

impl ApprovalKeys for Hardware {
    fn create(&self, _: bool) -> Result<(Vec<u8>, Vec<u8>)> {
        unreachable!()
    }

    fn delete_key(&self, handle: &[u8]) -> Result<()> {
        self.deleted.lock().unwrap().push(handle.to_vec());
        Ok(())
    }

    fn authorize(&self, _: &str, _: bool) -> Result<Box<dyn Authorized>> {
        self.asked.fetch_add(1, SeqCst);
        Err(Error::Unavailable("test must not prompt".into()))
    }
}

fn key(seed: u8, biometric: bool) -> LocalKey {
    let signing = p256::ecdsa::SigningKey::from_bytes(&[seed; 32].into()).unwrap();
    let public_key = signing
        .verifying_key()
        .to_sec1_point(true)
        .as_bytes()
        .to_vec();
    LocalKey {
        key_id: approval_key_id(&public_key).unwrap(),
        public_key: public_key.into(),
        handle: vec![seed; 16].into(),
        biometry: biometric.then_some(Hash::new([5; 32])),
        biometry_only: false,
        created_at: now_ms(),
    }
}

#[test]
fn a_mobile_operation_never_starts_a_fallback_prompt_on_the_client_executor() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let hardware = Arc::new(Hardware::default());
    client.set_approval_keys(hardware.clone());
    client.require_prepared_approval();
    assert_eq!(
        client
            .approval_sign(&key(7, false), "account_control", &Hash::new([1; 32]))
            .err(),
        Some(Error::Unavailable(HARDWARE_AUTHORIZATION_REQUIRED.into()))
    );
    assert_eq!(hardware.asked.load(SeqCst), 0);
}

#[test]
fn replacing_a_required_biometric_key_asks_for_the_stronger_acl_once() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    client.set_approval_keys(Arc::new(Hardware::default()));
    let old = key(7, true);
    let new = key(8, false);
    client
        .keep_approval_key(old.clone(), Some(old.key_id))
        .unwrap();
    client
        .keep_approval_key(new.clone(), Some(old.key_id))
        .unwrap();
    client.hardware_seen = Some(Seen {
        account: client.account().unwrap().id,
        key: Some(old.key_id),
        policy: vec![HardwarePurpose::AccountControl],
    });
    assert!(client.new_key_answer(&new.public_key).unwrap().unwrap().1);
    client.hardware_seen.as_mut().unwrap().policy.clear();
    assert!(!client.new_key_answer(&new.public_key).unwrap().unwrap().1);
}

#[test]
fn a_required_account_without_its_registered_key_is_rejected_before_presence() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    client.set_approval_keys(Arc::new(Hardware::default()));
    client.hardware_seen = Some(Seen {
        account: client.account().unwrap().id,
        key: None,
        policy: vec![HardwarePurpose::AccountControl],
    });
    assert_eq!(
        client
            .approval_answer(Some(HardwarePurpose::AccountControl), true)
            .err(),
        Some(Error::InvalidInput(APPROVAL_KEY_REQUIRED.into()))
    );
    let new = key(8, false);
    client.keep_approval_key(new.clone(), None).unwrap();
    assert_eq!(
        client.new_key_answer(&new.public_key).err(),
        Some(Error::InvalidInput(APPROVAL_KEY_REQUIRED.into()))
    );
}

#[tokio::test]
async fn a_proved_registration_cannot_be_saved_in_a_later_session() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let hardware = Arc::new(Hardware::default());
    let local = key(7, false);
    let proved = ProvedKeyRequest {
        preparation: KeyRequestPreparation {
            signer: None,
            hardware: hardware.clone(),
            account: client.account().unwrap().clone(),
            device_id: client.workspace.meta.device_id,
            generation: client.session_generation(),
            pending: PendingKey::new(hardware.clone(), &local.handle),
            key: local,
        },
        proof: P256Signature::new([0; 64]),
    };
    client.lock();
    assert_eq!(
        complete_key_request(&client.hold(), proved).await.err(),
        Some(Error::Locked)
    );
    assert_eq!(*hardware.deleted.lock().unwrap(), vec![vec![7; 16]]);
}

#[test]
fn abandoning_an_unsaved_key_removes_only_its_provider_object() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let hardware = Arc::new(Hardware::default());
    let local = key(9, false);
    let prepared = KeyRequestPreparation {
        signer: None,
        hardware: hardware.clone(),
        account: client.account().unwrap().clone(),
        device_id: client.workspace.meta.device_id,
        generation: client.session_generation(),
        pending: PendingKey::new(hardware.clone(), &local.handle),
        key: local,
    };
    assert!(prepared.prove().is_err());
    assert_eq!(*hardware.deleted.lock().unwrap(), vec![vec![9; 16]]);
    assert_eq!(hardware.asked.load(SeqCst), 1);
}

#[test]
fn pruning_committed_keys_preserves_the_registered_key() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let hardware = Arc::new(Hardware::default());
    client.set_approval_keys(hardware.clone());
    let registered = key(1, false).key_id;
    for seed in 1..=6 {
        client
            .keep_approval_key(key(seed, false), Some(registered))
            .unwrap();
    }
    assert_eq!(client.local_approval_keys().unwrap().len(), MAX_KEYS);
    assert!(
        client
            .local_approval_keys()
            .unwrap()
            .iter()
            .any(|key| key.key_id == registered)
    );
    assert_eq!(
        *hardware.deleted.lock().unwrap(),
        vec![vec![2; 16], vec![3; 16]]
    );
    // Ownership has moved to durable storage, so a caller's cleanup guard must not remove it.
    let mut kept = PendingKey::new(hardware.clone(), &[6; 16]);
    kept.kept = true;
    drop(kept);
    assert_eq!(hardware.deleted.lock().unwrap().len(), 2);
}
