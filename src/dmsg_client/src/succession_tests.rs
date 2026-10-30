use super::*;
use crate::testing::{TempDir, account as test_account, unlocked};
use dmsg_types::user::{ApprovalKey, Device, DeviceInput, SensitivePolicy, VaultWriteState};
use ed25519_dalek::{Signer, SigningKey};

struct Shared(tokio::sync::Mutex<Client>);

impl Hold for Shared {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        step(&mut *self.0.lock().await)
    }
}

fn hash(n: u8) -> Hash {
    Hash::new([n; 32])
}

fn task(side: Side) -> Task {
    Task {
        id: hash(1),
        side,
        account: test_account().id,
        predecessor: hash(2),
        successor: hash(3),
        token: String::new(),
        steps: BTreeMap::new(),
        cancelled_at_ms: None,
        created_at_ms: 1,
        moved_actor: None,
    }
}

fn device(n: u8, added_by: Option<Hash>, revoked_at: Option<u64>) -> Device {
    Device {
        input: DeviceInput {
            device_id: hash(n),
            signing_pub: SigningKey::from_bytes(&[n; 32])
                .verifying_key()
                .to_bytes()
                .into(),
            hpke_pub: hash(n),
            role: ControllerRole::Administrator,
            capabilities: vec![Capability::RootManage, Capability::VaultUnlock],
        },
        added_at: 5,
        added_by,
        revoked_at,
        next_sequence: 0,
    }
}

fn info(devices: Vec<Device>) -> AccountInfo {
    let account = test_account();
    AccountInfo {
        created_at_ms: 1,
        issuer: account.issuer,
        account_id: account.id,
        home_user: account.home,
        home_cose: account.home,
        auth_bindings: vec![],
        account_version: 7,
        security_epoch: 3,
        devices: devices
            .into_iter()
            .map(|d| (d.input.device_id, d))
            .collect(),
        recovery_delay_ms: 0,
        pending_recovery: None,
        recovered_device: None,
        current_root: None,
        root_slot: None,
        vault_write_state: VaultWriteState::Ready,
        sensitive_policy: SensitivePolicy::default(),
        approval_keys: BTreeMap::new(),
        hardware_policy: vec![],
    }
}

#[test]
fn each_side_runs_its_steps_in_order_until_cancelled() {
    let mut old = task(Side::Predecessor);
    assert_eq!(old.available(), vec![Step::Authorize]);
    old.record(Step::Authorize, Json::Null, 2);
    assert_eq!(
        old.available(),
        vec![Step::ApprovalKey, Step::Cosigners, Step::Content]
    );
    // A step is recorded once; the first evidence stays.
    assert!(old.record(Step::Cosigners, json!(1), 3));
    assert!(!old.record(Step::Cosigners, json!(2), 4));
    assert_eq!(old.steps[&Step::Cosigners].evidence, json!(1));

    let mut new = task(Side::Successor);
    // The successor only sees the enrollment, then hands over in order.
    assert!(new.available().is_empty());
    new.record(Step::Authorize, Json::Null, 2);
    assert_eq!(
        new.available(),
        vec![Step::ApprovalKey, Step::Cosigners, Step::Content]
    );
    new.record(Step::Cosigners, Json::Null, 3);
    assert!(new.available().contains(&Step::Wallets));
    new.record(Step::Wallets, Json::Null, 4);
    new.record(Step::ApprovalKey, Json::Null, 5);
    // Revoking waits until content and native addresses are settled.
    assert!(!new.available().contains(&Step::Revoke));
    new.skip(Step::Content, vec!["files".into()], 6).unwrap();
    new.skip(Step::NativeAddresses, vec![], 7).unwrap();
    assert_eq!(new.available(), vec![Step::Revoke]);
    assert_eq!(new.steps[&Step::Content].skipped, vec!["files".to_string()]);

    // A stopped task offers nothing more and keeps what it recorded.
    new.cancelled_at_ms = Some(8);
    assert!(new.available().is_empty());
    assert_eq!(new.steps.len(), 6);
}

#[test]
fn only_content_and_native_addresses_are_left_and_only_when_ready() {
    let mut new = task(Side::Successor);
    assert_eq!(new.skip(Step::Content, vec![], 1), Err(Error::Forbidden));
    new.record(Step::Authorize, Json::Null, 2);
    assert!(matches!(
        new.skip(Step::Wallets, vec![], 3),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        new.skip(Step::NativeAddresses, vec![], 3),
        Err(Error::Forbidden)
    );
    new.skip(Step::Content, vec![], 3).unwrap();
    assert_eq!(new.skip(Step::Content, vec![], 4), Err(Error::Forbidden));
}

#[test]
fn the_account_shows_enrollment_keys_revocation_and_cancellation() {
    let mut new = task(Side::Successor);
    // Another device's enrollment of the same ID is not this succession's.
    let mut state = info(vec![device(2, None, None), device(3, Some(hash(9)), None)]);
    assert!(!new.observe(&state, 10));
    state
        .devices
        .insert(hash(3), device(3, Some(hash(2)), None));
    assert!(new.observe(&state, 10));
    assert_eq!(
        new.steps[&Step::Authorize].evidence,
        json!({"account_version": 7, "added_at": 5})
    );
    assert!(!new.observe(&state, 11));
    state.approval_keys.insert(
        hash(3),
        ApprovalKey {
            key_id: hash(4),
            public_key: vec![2; 33].into(),
            registered_at: 6,
            registered_by: hash(2),
        },
    );
    state.devices.insert(hash(2), device(2, None, Some(12)));
    state.vault_write_state = VaultWriteState::RekeyRequired;
    assert!(new.observe(&state, 12));
    assert!(new.steps.contains_key(&Step::ApprovalKey));
    assert!(!new.steps.contains_key(&Step::Revoke));
    state.vault_write_state = VaultWriteState::Ready;
    assert!(new.observe(&state, 12));
    assert_eq!(new.steps[&Step::Revoke].evidence["revoked_at"], 12);
    assert_eq!(new.cancelled_at_ms, None);
    // Revoking the successor cancels.
    state
        .devices
        .insert(hash(3), device(3, Some(hash(2)), Some(13)));
    assert!(new.observe(&state, 13));
    assert_eq!(new.cancelled_at_ms, Some(13));
}

/// A token from successor device 9 for `predecessor`, valid at `expires_at`.
fn token(predecessor: Hash, expires_at: u64, cosigner: Option<SuccessorKey>) -> Token {
    let account = test_account();
    let successor = SigningKey::from_bytes(&[9; 32]);
    let device = DeviceInput {
        device_id: hash(9),
        signing_pub: successor.verifying_key().to_bytes().into(),
        hpke_pub: hash(9),
        role: ControllerRole::Administrator,
        capabilities: vec![Capability::RootManage, Capability::VaultUnlock],
    };
    let request_id = hash(10);
    let proof = successor.sign(
        successor_device_message(
            account.home,
            &account.id,
            &device,
            4,
            request_id,
            predecessor,
        )
        .as_slice(),
    );
    let key = p256::ecdsa::SigningKey::from_slice(&[5; 32]).unwrap();
    let public_key: serde_bytes::ByteBuf = key
        .verifying_key()
        .to_sec1_point(true)
        .as_bytes()
        .to_vec()
        .into();
    let message = approval_key_message(account.home, &account.id, hash(9), &public_key);
    let signature: p256::ecdsa::Signature =
        p256::ecdsa::signature::Signer::sign(&key, message.as_slice());
    Token {
        format: FORMAT.into(),
        home: account.home,
        pairing: AccountMutation {
            account_id: account.id,
            expected_version: 4,
            command: AccountCommand::AddSuccessor {
                device,
                proof: proof.to_bytes().into(),
            },
            approval: Approval {
                device_id: hash(9),
                security_epoch: 3,
                sequence: 0,
                request_id,
                expires_at,
                signature: Ed25519Signature::new([0; 64]),
                hardware: None,
            },
        },
        approval_key: Some(ApprovalKeyInput {
            public_key,
            proof: dmsg_types::P256Signature::new(signature.normalize_s().to_bytes().into()),
        }),
        cosigner,
    }
}

#[tokio::test]
async fn tokens_name_this_device_and_prove_every_key() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let own = client.workspace.meta.device_id;
    let other = TempDir::new();
    let mut next = unlocked(&other.0, false, None);
    next.wallet_cosigner_create("next", false).unwrap();
    let member = MemberId::Account(test_account().id);
    let cosigner = rotate::successor(&Shared(tokio::sync::Mutex::new(next)), member, "next")
        .await
        .unwrap();
    let text = |t: &Token| b64(&canonical(t));
    let soon = now_ms() + 60_000;

    let valid = token(own, soon, Some(cosigner.clone()));
    let (pairing, parsed) = client.inspect_succession(&text(&valid)).unwrap();
    assert_eq!(pairing.device.device_id, hash(9));
    assert_eq!(pairing.mutation, valid.pairing);
    assert_eq!(parsed.cosigner, Some(cosigner.clone()));

    // A token for another predecessor, an expired one, or a key that does
    // not prove itself is refused.
    let other_device = token(hash(8), soon, None);
    assert_eq!(
        client.inspect_succession(&text(&other_device)).unwrap_err(),
        Error::IntegrityFailed
    );
    let late = token(own, now_ms() - 1, None);
    assert_eq!(
        client.inspect_succession(&text(&late)).unwrap_err(),
        Error::Expired
    );
    let mut forged = token(own, soon, None);
    forged.approval_key.as_mut().unwrap().public_key[1] ^= 1;
    assert!(client.inspect_succession(&text(&forged)).is_err());
    let mut stranger = cosigner;
    stranger.input.member = MemberId::Account(AccountId([7; 12]));
    let foreign = token(own, soon, Some(stranger));
    assert!(client.inspect_succession(&text(&foreign)).is_err());
    let mut pairing = token(own, soon, None);
    pairing.format = "dmsg-device-request/2".into();
    assert_eq!(
        client.inspect_succession(&text(&pairing)).unwrap_err(),
        Error::IntegrityFailed
    );
}

#[test]
fn a_task_reads_back_as_it_was_saved_to_continue_later() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let mut saved = task(Side::Predecessor);
    saved.record(
        Step::Authorize,
        json!({"account_version": 7, "added_at": 5}),
        2,
    );
    saved.record(
        Step::Cosigners,
        json!({"rotations": [{"wallet": "aaaaa-aa", "cosigner": 3, "rotated": true}]}),
        3,
    );
    client.save_succession(&saved).unwrap();
    let read = client.succession(&saved.id).unwrap();
    assert_eq!(read, saved);
    assert_eq!(read.available(), vec![Step::ApprovalKey, Step::Content]);
    assert_eq!(client.succession(&hash(4)), Err(Error::NotFound));
}

#[tokio::test]
async fn a_transfer_with_the_other_device_settles_content() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let mut saved = task(Side::Successor);
    client.save_succession(&saved).unwrap();
    let h = Shared(tokio::sync::Mutex::new(client));
    let report = |peer| Report {
        peer: Some(hash(peer)),
        records: 4,
        ..Report::default()
    };
    // Not before the successor joined, and only with the predecessor.
    assert_eq!(
        transferred(&h, saved.id, &report(2)).await,
        Err(Error::Forbidden)
    );
    saved.record(Step::Authorize, Json::Null, 2);
    h.0.lock().await.save_succession(&saved).unwrap();
    for incomplete in [
        Report {
            skipped: 1,
            ..report(2)
        },
        Report {
            pending_sync: 1,
            ..report(2)
        },
    ] {
        assert_eq!(
            transferred(&h, saved.id, &incomplete).await,
            Err(Error::RecoveryIncomplete)
        );
        assert!(
            h.0.lock()
                .await
                .succession(&saved.id)
                .unwrap()
                .available()
                .contains(&Step::Content)
        );
    }
    assert_eq!(
        transferred(&h, saved.id, &report(9)).await,
        Err(Error::Forbidden)
    );
    let view = transferred(&h, saved.id, &report(2)).await.unwrap();
    assert_eq!(view["steps"][3]["step"], "content");
    assert_eq!(view["steps"][3]["evidence"]["records"], 4);
    assert_eq!(view["steps"][3]["evidence"]["peer"], hex::encode([2; 32]));
    // Once.
    assert_eq!(
        transferred(&h, saved.id, &report(2)).await,
        Err(Error::Forbidden)
    );
}

#[tokio::test]
async fn an_unreachable_wallet_hub_is_not_an_account_without_wallets() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    assert_eq!(wallets(&client.hold()).await.unwrap(), json!([]));
    client.config.wallet.hub = Some(Principal::from_slice(&[7; 10]));
    assert!(wallets(&client.hold()).await.is_err());
}
