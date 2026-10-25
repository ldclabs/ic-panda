use super::*;
use crate::approval::{Ask, request_purpose};
use crate::secure::Authorized;
use crate::testing::{TempDir, account, unlocked};
use dmsg_types::user::{
    ApprovalKey, Capability, ControllerRole, Device, DeviceInput, SensitivePolicy, VaultWriteState,
};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering::SeqCst};

/// Approval keys in software. Its signatures come with a high `s`, which
/// the client turns low.
#[derive(Default)]
struct Software {
    asked: AtomicU32,
    declines: AtomicBool,
    away: AtomicBool,
    made: AtomicU8,
}

fn signing(handle: &[u8]) -> SigningKey {
    SigningKey::from_slice(&[handle[0]; 32]).unwrap()
}

fn public(handle: &[u8]) -> Vec<u8> {
    signing(handle)
        .verifying_key()
        .to_sec1_point(true)
        .as_bytes()
        .to_vec()
}

impl ApprovalKeys for Software {
    fn create(&self, _: bool) -> Result<(Vec<u8>, Vec<u8>)> {
        let handle = vec![self.made.fetch_add(1, SeqCst) + 40];
        Ok((public(&handle), handle))
    }

    fn authorize(&self, _: &str, _: bool) -> Result<Box<dyn Authorized>> {
        self.asked.fetch_add(1, SeqCst);
        match self.declines.load(SeqCst) {
            true => Err(Error::Forbidden),
            false => Ok(Box::new(Answer(self.away.load(SeqCst)))),
        }
    }
}

struct Answer(bool);

impl Authorized for Answer {
    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]> {
        if self.0 {
            return Err(Error::Unavailable("not now".into()));
        }
        let signature: Signature = signing(handle).sign(message.as_slice());
        let (r, s) = signature.normalize_s().split_scalars();
        Ok(Signature::from_scalars(r, -s).unwrap().to_bytes().into())
    }
}

/// Fingerprints that change when told to.
#[derive(Default)]
struct Fingerprints(AtomicU8);

impl PresenceKey for Fingerprints {
    fn presence(&self) -> Option<crate::secure::Presence> {
        Some(crate::secure::Presence::Biometrics)
    }

    fn create(
        &self,
        _: bool,
    ) -> Result<(Vec<u8>, Vec<u8>, zeroize::Zeroizing<crate::crypto::Key>)> {
        Err(Error::Unavailable("test".into()))
    }

    fn open(&self, _: &[u8], _: &[u8], _: bool) -> Result<zeroize::Zeroizing<crate::crypto::Key>> {
        Err(Error::Unavailable("test".into()))
    }

    fn biometry_state(&self) -> Option<Hash> {
        Some(Hash::new([self.0.load(SeqCst); 32]))
    }

    fn confirm(&self, _: &str) -> Result<()> {
        Ok(())
    }
}

/// A client with `software` keys and one key made, not yet registered.
fn client(dir: &TempDir, software: &Arc<Software>, biometry: Option<Hash>) -> (Client, LocalKey) {
    let mut client = unlocked(&dir.0, false, None);
    client.set_approval_keys(software.clone());
    let (public_key, handle) = software.create(biometry.is_some()).unwrap();
    let key = LocalKey {
        key_id: approval_key_id(&public_key).unwrap(),
        public_key: public_key.into(),
        handle: handle.into(),
        biometry,
        created_at: 1,
    };
    client.keep_approval_key(key.clone(), None).unwrap();
    (client, key)
}

/// The account as its user home answers: this device, registered with
/// `key` if any, under `policy`.
fn info(client: &Client, key: Option<&LocalKey>, policy: &[HardwarePurpose]) -> AccountInfo {
    let device_id = client.workspace.meta.device_id;
    let device = Device {
        input: DeviceInput {
            device_id,
            signing_pub: client.workspace.meta.signing_pub,
            hpke_pub: client.workspace.meta.hpke_pub,
            role: ControllerRole::Administrator,
            capabilities: vec![Capability::RootManage, Capability::VaultUnlock],
        },
        added_at: 1,
        added_by: None,
        revoked_at: None,
        next_sequence: 0,
    };
    AccountInfo {
        created_at_ms: 1,
        issuer: account().issuer,
        account_id: account().id,
        home_user: account().home,
        home_cose: account().home,
        auth_bindings: vec![],
        account_version: 1,
        security_epoch: 1,
        devices: BTreeMap::from([(device_id, device)]),
        recovery_delay_ms: 0,
        pending_recovery: None,
        recovered_device: None,
        current_root: None,
        root_slot: None,
        vault_write_state: VaultWriteState::Ready,
        sensitive_policy: SensitivePolicy::default(),
        approval_keys: key
            .map(|k| {
                BTreeMap::from([(
                    device_id,
                    ApprovalKey {
                        key_id: k.key_id,
                        public_key: k.public_key.clone(),
                        registered_at: 1,
                        registered_by: device_id,
                    },
                )])
            })
            .unwrap_or_default(),
        hardware_policy: policy.to_vec(),
    }
}

#[test]
fn proofs_follow_the_policy_and_the_persons_answer() {
    let dir = TempDir::new();
    let software = Arc::new(Software::default());
    let (mut client, key) = client(&dir, &software, None);
    let control = HardwarePurpose::AccountControl;
    let message = Hash::new([3; 32]);
    // Unregistered, the key signs nothing; where it is required that is
    // an error before anything is sent.
    let unregistered = info(&client, None, &[control]);
    assert_eq!(
        client.hardware_proof(&unregistered, Some(control), message),
        Err(Error::InvalidInput(APPROVAL_KEY_REQUIRED.into()))
    );
    assert_eq!(
        client.hardware_proof(&info(&client, None, &[]), Some(control), message),
        Ok(None)
    );
    assert_eq!(
        client.hardware_proof(&unregistered, None, message),
        Ok(None)
    );
    // Registered and required: the person is asked, and the user home's
    // check passes with a low s.
    let required = info(&client, Some(&key), &[control]);
    let proof = client
        .hardware_proof(&required, Some(control), message)
        .unwrap()
        .unwrap();
    assert_eq!(software.asked.load(SeqCst), 1);
    let registered = &required.approval_keys[&client.workspace.meta.device_id];
    assert_eq!(
        dmsg_protocol::verify_hardware(registered, message, &proof),
        Ok(())
    );
    // Not required, and nobody asked: nothing to sign.
    let optional = info(&client, Some(&key), &[]);
    assert_eq!(
        client.hardware_proof(&optional, Some(control), message),
        Ok(None)
    );
    assert_eq!(software.asked.load(SeqCst), 1);
    // The answer the person gave for this decision signs without asking
    // again, required or not.
    client.authorized = Some(software.authorize("device_revoke", false).unwrap());
    let answered = client
        .hardware_proof(&optional, Some(control), message)
        .unwrap()
        .unwrap();
    assert_eq!(software.asked.load(SeqCst), 2);
    assert_eq!(
        dmsg_protocol::verify_hardware(registered, message, &answered),
        Ok(())
    );
    client.authorized = None;
    // Declining stops the approval.
    software.declines.store(true, SeqCst);
    assert_eq!(
        client.hardware_proof(&required, Some(control), message),
        Err(Error::Forbidden)
    );
    // Another key registered for this device is not ours.
    let mut other = key.clone();
    other.key_id = Hash::new([9; 32]);
    assert_eq!(
        client.hardware_proof(
            &info(&client, Some(&other), &[control]),
            Some(control),
            message
        ),
        Err(Error::InvalidInput(APPROVAL_KEY_REQUIRED.into()))
    );
}

#[test]
fn the_person_answers_a_registered_device_through_its_key() {
    let dir = TempDir::new();
    let software = Arc::new(Software::default());
    let (mut client, key) = client(&dir, &software, None);
    let revoke = client
        .approval_create(
            "gui",
            Action::DeviceRevoke {
                device_id: Hash::new([7; 32]),
            },
            now_ms() + 60_000,
        )
        .unwrap();
    let attest = client
        .attest_request(
            "gui",
            crate::attest::StatementInput::Text {
                text: "hello".into(),
                subject: None,
            },
            None,
        )
        .unwrap();
    let attest = attest["id"].as_str().unwrap().to_string();
    // Before the account says the key is registered, nothing changes.
    assert!(client.approval_presence(&revoke.id).unwrap().is_none());
    client.observe_hardware(&info(&client, Some(&key), &[]));
    // A high-risk request is answered through the key; an attestation only
    // where the account protects attestations.
    assert!(matches!(
        client.approval_presence(&revoke.id).unwrap(),
        Some((Ask::Hardware(_, false), what)) if what == "device_revoke"
    ));
    assert!(client.approval_presence(&attest).unwrap().is_none());
    client.observe_hardware(&info(&client, Some(&key), &[HardwarePurpose::Attestation]));
    assert!(matches!(
        client.approval_presence(&attest).unwrap(),
        Some((Ask::Hardware(..), _))
    ));
    // Registering this device's new key asks through it, registered or not.
    let register = client
        .approval_create(
            "gui",
            Action::ApprovalKey {
                device_id: client.workspace.meta.device_id,
                public_key: key.public_key.clone(),
                proof: None,
            },
            now_ms() + 60_000,
        )
        .unwrap();
    client.hardware_seen = None;
    assert!(matches!(
        client.approval_presence(&register.id).unwrap(),
        Some((Ask::Hardware(_, false), what)) if what == "approval_key"
    ));
    client.observe_hardware(&info(&client, Some(&key), &[HardwarePurpose::Attestation]));
    // A lock forgets what it read.
    client.lock();
    client.keys = Some(client.workspace.unlock(&crate::testing::SECRET).unwrap());
    assert!(client.approval_presence(&attest).unwrap().is_none());
    // The purposes of what a person approves.
    let record = client.approval_record(&revoke.id).unwrap();
    assert_eq!(
        request_purpose(&record, &serde_json::Value::Null),
        Some(HardwarePurpose::DeviceControl)
    );
    for name in [
        "account_control",
        "device_control",
        "principal_control",
        "payment",
        "attestation",
    ] {
        assert_eq!(purpose_name(purpose_named(name).unwrap()), name);
    }
    assert!(purpose_named("wallet").is_err());
}

#[test]
fn key_requests_prove_the_key_for_this_device_of_this_account() {
    let dir = TempDir::new();
    let software = Arc::new(Software::default());
    let (client, key) = client(&dir, &software, None);
    let account = account();
    let device_id = Hash::new([7; 32]);
    let token = |device: Hash, account_id: AccountId| {
        let message = approval_key_message(account.home, &account.id, device_id, &key.public_key);
        let raw = software
            .authorize("approval_key", false)
            .unwrap()
            .sign(&key.handle, &message)
            .unwrap();
        b64(&canonical(&KeyRequest {
            format: REQUEST.into(),
            home: account.home,
            account: account_id,
            device_id: device,
            key: ApprovalKeyInput {
                public_key: key.public_key.clone(),
                proof: low_s(&key.public_key, &message, &raw).unwrap(),
            },
        }))
    };
    let request = client
        .inspect_key_request(&token(device_id, account.id))
        .unwrap();
    assert_eq!(request.device_id, device_id);
    // The proof binds the device and the account.
    assert!(
        client
            .inspect_key_request(&token(Hash::new([8; 32]), account.id))
            .is_err()
    );
    assert_eq!(
        client
            .inspect_key_request(&token(device_id, AccountId([2; 12])))
            .err(),
        Some(Error::IntegrityFailed)
    );
}

#[test]
fn a_fingerprint_only_key_is_lost_once_the_fingerprints_change() {
    let dir = TempDir::new();
    let software = Arc::new(Software::default());
    let fingerprints = Arc::new(Fingerprints::default());
    let (mut client, key) = client(&dir, &software, Some(Hash::new([0; 32])));
    client.set_presence_key(fingerprints.clone());
    let required = info(&client, Some(&key), &[HardwarePurpose::Payment]);
    let message = Hash::new([3; 32]);
    let payment = Some(HardwarePurpose::Payment);
    // Without its keyboard the key is only unavailable now.
    software.away.store(true, SeqCst);
    assert!(matches!(
        client.hardware_proof(&required, payment, message),
        Err(Error::Unavailable(_))
    ));
    fingerprints.0.store(1, SeqCst);
    assert_eq!(
        client.hardware_proof(&required, payment, message),
        Err(Error::IntegrityFailed)
    );
    let view = client.approval_key_view(&required).unwrap();
    assert_eq!(view["key"]["fingerprints_changed"], true);
    assert_eq!(view["key"]["biometry_only"], true);
    // The view never shows a handle.
    assert!(!view.to_string().contains(&hex::encode(&key.handle)));
}
