//! Approval keys on the real user home module, and the in-place upgrades of
//! schema 15 accounts to them and of schema 16 accounts to device
//! successions. The upgrade tests need the old modules: run them through
//! scripts/test-dmsg-hardware-upgrade.sh and
//! scripts/test-dmsg-succession-upgrade.sh.
use super::*;
use serde::Serialize;

/// Approval key `k`: a P-256 key, apart from every device's Ed25519 key.
fn hw(k: u8) -> p256::ecdsa::SigningKey {
    p256::ecdsa::SigningKey::from_slice(&[k; 32]).unwrap()
}

fn hw_public(k: u8) -> ByteBuf {
    hw(k)
        .verifying_key()
        .to_sec1_point(true)
        .as_bytes()
        .to_vec()
        .into()
}

fn hw_sign(k: u8, message: Hash) -> P256Signature {
    let signature: p256::ecdsa::Signature = hw(k).sign(message.as_slice());
    P256Signature::new(signature.normalize_s().to_bytes().into())
}

/// Key `k` for device `n` of account `id`, with its proof of possession.
fn key_input(f: &Fixture, id: &AccountId, n: u8, k: u8) -> ApprovalKeyInput {
    let public_key = hw_public(k);
    ApprovalKeyInput {
        proof: hw_sign(
            k,
            approval_key_message(f.user, id, Hash::new([n; 32]), &public_key),
        ),
        public_key,
    }
}

/// Key `k`'s proof for an approval whose `approval_message` is `message`.
fn proof(k: u8, message: Hash) -> HardwareProof {
    let key_id = approval_key_id(&hw_public(k)).unwrap();
    HardwareProof {
        key_id,
        signature: hw_sign(k, hardware_message(message, key_id)),
    }
}

/// `command` approved by device `n`, with approval key `k`'s proof if any,
/// sent by the account's login 1.
fn apply(
    f: &Fixture,
    n: u8,
    id: &AccountId,
    command: AccountCommand,
    k: Option<u8>,
) -> Result<OperationReceipt> {
    let s = f.account_id(1, id);
    let mut m = AccountMutation {
        account_id: *id,
        expected_version: s.account_version,
        command,
        approval: Approval {
            device_id: Hash::new([n; 32]),
            security_epoch: s.security_epoch,
            sequence: s.devices[&Hash::new([n; 32])].next_sequence,
            request_id: digest("test-operation", &(id, s.account_version)),
            expires_at: time(&f.ic) + MINUTE,
            signature: Default::default(),
            hardware: None,
        },
    };
    let message = approval_message(
        f.user,
        id,
        "dmsg/account/v2",
        &(&m.expected_version, &m.command),
        &m.approval,
    );
    m.approval.signature = key(n).sign(message.as_slice()).to_bytes().into();
    m.approval.hardware = k.map(|k| proof(k, message));
    update(&f.ic, f.user, person(1), "mutate_account", (m,))
}

/// Enroll administrator device `n`, approved by device 1.
fn enroll(f: &Fixture, id: &AccountId, n: u8) {
    let s = f.account_id(1, id);
    let request_id = digest("test-operation", &(id, s.account_version));
    let proof = key(n)
        .sign(
            digest(
                "dmsg/add-device/v1",
                &(f.user, id, &device(n), s.account_version, request_id),
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    f.mutate(
        1,
        id,
        AccountCommand::AddDevice {
            device: device(n),
            proof,
        },
    )
    .unwrap();
}

fn policy(extra: &[HardwarePurpose]) -> AccountCommand {
    let mut purposes = CONTROL_PURPOSES.to_vec();
    purposes.extend_from_slice(extra);
    AccountCommand::SetHardwarePolicy { purposes }
}

fn delay(days: u64) -> AccountCommand {
    AccountCommand::SetRecoveryDelay {
        delay_ms: days * DAY,
    }
}

// The approval and command of a client without approval keys: Candid
// without the `hardware` field and the new commands.
#[derive(CandidType, Serialize, Clone)]
struct OldApproval {
    device_id: Hash,
    security_epoch: u64,
    sequence: u64,
    request_id: OpId,
    expires_at: u64,
    signature: Ed25519Signature,
}

#[derive(CandidType, Serialize, Clone)]
enum OldCommand {
    BindAuth {
        principal: Principal,
        nonce: Hash,
    },
    SetRecoveryDelay {
        delay_ms: u64,
    },
    ReserveRoot {
        expected_generation: u64,
        op_id: OpId,
    },
    CommitRoot {
        expected_generation: u64,
        op_id: OpId,
        root: ContentRootRef,
    },
}

#[derive(CandidType, Clone)]
struct OldMutation {
    account_id: AccountId,
    expected_version: u64,
    command: OldCommand,
    approval: OldApproval,
}

/// `command` as a client without approval keys sends it.
fn old_mutation(f: &Fixture, n: u8, id: &AccountId, command: OldCommand) -> OldMutation {
    let s = f.account_id(1, id);
    let approval = Approval {
        device_id: Hash::new([n; 32]),
        security_epoch: s.security_epoch,
        sequence: s.devices[&Hash::new([n; 32])].next_sequence,
        request_id: digest("test-operation", &(id, s.account_version)),
        expires_at: time(&f.ic) + MINUTE,
        signature: Default::default(),
        hardware: None,
    };
    let message = approval_message(
        f.user,
        id,
        "dmsg/account/v2",
        &(&s.account_version, &command),
        &approval,
    );
    OldMutation {
        account_id: *id,
        expected_version: s.account_version,
        command,
        approval: OldApproval {
            device_id: approval.device_id,
            security_epoch: approval.security_epoch,
            sequence: approval.sequence,
            request_id: approval.request_id,
            expires_at: approval.expires_at,
            signature: key(n).sign(message.as_slice()).to_bytes().into(),
        },
    }
}

fn old_apply(f: &Fixture, m: &OldMutation) -> Result<OperationReceipt> {
    update(&f.ic, f.user, person(1), "mutate_account", (m.clone(),))
}

#[test]
fn the_home_requires_approval_keys_once_the_account_turns_them_on() {
    let f = Fixture::new();
    let id = f.create(1);
    // Opt in while account control still needs only the device's Ed25519 key.
    f.enable_recovery(1, &id);
    enroll(&f, &id, 2);
    // Without a policy the device's Ed25519 approval registers its key.
    apply(
        &f,
        1,
        &id,
        AccountCommand::SetApprovalKey {
            device_id: Hash::new([1; 32]),
            key: Some(key_input(&f, &id, 1, 31)),
        },
        None,
    )
    .unwrap();
    assert_eq!(apply(&f, 1, &id, policy(&[]), None), Err(Error::Forbidden));
    apply(
        &f,
        1,
        &id,
        policy(&[HardwarePurpose::Attestation]),
        Some(31),
    )
    .unwrap();
    let info = f.account_id(1, &id);
    assert_eq!(info.hardware_policy.len(), 4);
    assert_eq!(
        info.approval_keys[&Hash::new([1; 32])].key_id,
        approval_key_id(&hw_public(31)).unwrap()
    );
    // Neither the old Candid nor a request without a proof gets through.
    let old = old_mutation(
        &f,
        1,
        &id,
        OldCommand::SetRecoveryDelay { delay_ms: 2 * DAY },
    );
    assert_eq!(old_apply(&f, &old), Err(Error::Forbidden));
    assert_eq!(apply(&f, 1, &id, delay(2), None), Err(Error::Forbidden));
    apply(&f, 1, &id, delay(2), Some(31)).unwrap();
    // A device without a key can change nothing protected, its own key
    // included, until a device with one registers it.
    let revoke_2 = AccountCommand::RevokeDevice {
        device_id: Hash::new([2; 32]),
    };
    assert_eq!(apply(&f, 2, &id, delay(3), Some(31)), Err(Error::Forbidden));
    let own = AccountCommand::SetApprovalKey {
        device_id: Hash::new([2; 32]),
        key: Some(key_input(&f, &id, 2, 32)),
    };
    assert_eq!(apply(&f, 2, &id, own.clone(), None), Err(Error::Forbidden));
    apply(&f, 1, &id, own, Some(31)).unwrap();
    apply(&f, 2, &id, delay(3), Some(32)).unwrap();
    // Root rotation is not protected.
    f.rekey(1, &id);
    // Attestations are, here.
    let mut request = f.attest_request(
        1,
        &id,
        Statement {
            issuer: account_issuer(NAMESPACE, &id),
            subject: None,
            issued_at: None,
            content: StatementContent::Text("with a key".into()),
        },
    );
    let refused: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (request.clone(),));
    assert_eq!(refused, Err(Error::Forbidden));
    let message = approval_message(
        f.user,
        &id,
        ATTEST_APPROVAL_DOMAIN,
        &attest_approval_command(&request.statement, &request.origin, &request.signature),
        &request.approval,
    );
    request.approval.hardware = Some(proof(31, message));
    let signed: Result<SignedArtifact> = update(&f.ic, f.user, person(1), "attest", (request,));
    signed.unwrap();
    // Revoking a device retires its key.
    apply(&f, 1, &id, revoke_2, Some(31)).unwrap();
    assert!(
        !f.account_id(1, &id)
            .approval_keys
            .contains_key(&Hash::new([2; 32]))
    );
    // A login recovery takes over regardless, and the keys and the policy
    // go with the old devices.
    f.recover(1, 3, &id);
    let info = f.account_id(1, &id);
    assert!(info.approval_keys.is_empty() && info.hardware_policy.is_empty());
    apply(&f, 3, &id, delay(1), None).unwrap();
}

pub(super) fn old_wasm(name: &str) -> Vec<u8> {
    let directory = PathBuf::from(
        std::env::var_os("DMSG_PRE_HARDWARE_WASM_DIR")
            .expect("run scripts/test-dmsg-hardware-upgrade.sh"),
    );
    std::fs::read(directory.join(format!("{name}.wasm"))).unwrap()
}

fn snapshot(f: &Fixture, id: &AccountId) -> Vec<u8> {
    let result: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "security_snapshot_batch",
        (vec![id],),
    );
    dmsg_client::certified::certified_path(
        &result.unwrap(),
        id.as_slice(),
        &[id.as_slice()],
        f.user,
        &f.ic.root_key().unwrap(),
        time(&f.ic),
    )
    .unwrap()
    .unwrap()
}

fn unlock(f: &Fixture, login: u8, id: &AccountId, device: u8) -> Hash {
    let result: Result<Hash> = query(
        &f.ic,
        f.user,
        person(login),
        "unlock_secret",
        (id, Hash::new([device; 32])),
    );
    result.unwrap()
}

#[test]
#[ignore = "requires the schema 15 Wasm; run scripts/test-dmsg-hardware-upgrade.sh"]
fn schema_15_upgrade_keeps_accounts_and_starts_without_approval_keys() {
    let f = Fixture::with_identity_modules(
        true,
        100,
        MAX_PAYMENT_ESCROWS,
        |_| {},
        [
            old_wasm("dmsg_user"),
            wasm("dmsg_directory"),
            wasm("dmsg_handle"),
        ],
    );
    let id = f.create(1);
    let other = f.create(2);
    // A committed root, a second login, a changed delay and a receipt.
    let op_id = Hash::new([80; 32]);
    old_apply(
        &f,
        &old_mutation(
            &f,
            1,
            &id,
            OldCommand::ReserveRoot {
                expected_generation: 0,
                op_id,
            },
        ),
    )
    .unwrap();
    let recipients_digest = root_recipients_digest(&[Hash::new([1; 32])], 1);
    let body_digest = Hash::new([13; 32]);
    let root = ContentRootRef {
        generation: 1,
        suite: "dmsg-root-v2".into(),
        recovery_enabled: None,
        bundle_digest: root_bundle_digest(recipients_digest, body_digest),
        recipients_digest,
        body_digest,
    };
    old_apply(
        &f,
        &old_mutation(
            &f,
            1,
            &id,
            OldCommand::CommitRoot {
                expected_generation: 0,
                op_id,
                root,
            },
        ),
    )
    .unwrap();
    let nonce = Hash::new([81; 32]);
    old_apply(
        &f,
        &old_mutation(
            &f,
            1,
            &id,
            OldCommand::BindAuth {
                principal: person(9),
                nonce,
            },
        ),
    )
    .unwrap();
    let accepted: Result<()> = update(&f.ic, f.user, person(9), "accept_auth_binding", (id, nonce));
    accepted.unwrap();
    let replay = old_mutation(
        &f,
        1,
        &id,
        OldCommand::SetRecoveryDelay { delay_ms: 2 * DAY },
    );
    let receipt = old_apply(&f, &replay).unwrap();
    let before = f.account_id(1, &id);
    assert!(before.approval_keys.is_empty() && before.hardware_policy.is_empty());
    let before_other = f.account_id(2, &other);
    let security = snapshot(&f, &id);
    let secret = unlock(&f, 9, &id, 1);
    let config: UserInit = query(&f.ic, f.user, Principal::anonymous(), "user_config", ());

    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    // The new module answers with the new fields, empty.
    let info: Result<AccountInfo> = query(&f.ic, f.user, person(1), "get_account", (id,));
    assert_eq!(info, Ok(before.clone()));
    assert_eq!(f.account_id(9, &id), before);
    assert_eq!(f.account_id(2, &other), before_other);
    assert_eq!(snapshot(&f, &id), security);
    assert_eq!(unlock(&f, 1, &id, 1), secret);
    assert_eq!(
        query::<_, UserInit>(&f.ic, f.user, Principal::anonymous(), "user_config", ()),
        config
    );
    assert_eq!(old_apply(&f, &replay), Ok(receipt));

    // The upgraded account takes keys; old clients are then refused.
    apply(
        &f,
        1,
        &id,
        AccountCommand::SetApprovalKey {
            device_id: Hash::new([1; 32]),
            key: Some(key_input(&f, &id, 1, 31)),
        },
        None,
    )
    .unwrap();
    apply(&f, 1, &id, policy(&[]), Some(31)).unwrap();
    let old = old_mutation(&f, 1, &id, OldCommand::SetRecoveryDelay { delay_ms: DAY });
    assert_eq!(old_apply(&f, &old), Err(Error::Forbidden));
    // Another upgrade keeps them.
    let keyed = f.account_id(1, &id);
    f.ic.advance_time(Duration::from_secs(600));
    f.ic.tick();
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.account_id(1, &id), keyed);
}

#[test]
#[ignore = "requires the schema 16 Wasm; run scripts/test-dmsg-succession-upgrade.sh"]
fn schema_16_upgrade_keeps_accounts_and_admits_successions() {
    let directory = PathBuf::from(
        std::env::var_os("DMSG_PRE_SUCCESSION_WASM_DIR")
            .expect("run scripts/test-dmsg-succession-upgrade.sh"),
    );
    let f = Fixture::with_identity_modules(
        false,
        100,
        MAX_PAYMENT_ESCROWS,
        |_| {},
        [
            std::fs::read(directory.join("dmsg_user.wasm")).unwrap(),
            wasm("dmsg_directory"),
            wasm("dmsg_handle"),
        ],
    );
    // A full Free account with an approval key, the hardware policy and a
    // pending recovery, beside a plain one.
    let id = f.create(1);
    let other = f.create(2);
    enroll(&f, &id, 2);
    apply(
        &f,
        1,
        &id,
        AccountCommand::SetApprovalKey {
            device_id: Hash::new([1; 32]),
            key: Some(key_input(&f, &id, 1, 31)),
        },
        None,
    )
    .unwrap();
    apply(&f, 1, &id, policy(&[]), Some(31)).unwrap();
    let s = f.account_id(1, &id);
    let request = RecoveryRequest {
        op_id: digest("test-recovery", &(id, 9u8)),
        new_auth: person(1),
        device: device(9),
        expires_at: time(&f.ic) + s.recovery_delay_ms + DAY,
    };
    let proof = ByteBuf::from(
        key(9)
            .sign(recovery_device_message(f.user, &id, &request).as_slice())
            .to_bytes()
            .to_vec(),
    );
    let begun: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "request_recovery",
        (id, request.clone(), proof),
    );
    begun.unwrap();
    let before = (f.account_id(1, &id), f.account_id(2, &other));
    let security = (snapshot(&f, &id), snapshot(&f, &other));
    let secrets = [
        unlock(&f, 1, &id, 1),
        unlock(&f, 1, &id, 2),
        unlock(&f, 2, &other, 2),
    ];

    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!((f.account_id(1, &id), f.account_id(2, &other)), before);
    assert_eq!((snapshot(&f, &id), snapshot(&f, &other)), security);
    assert_eq!(
        [
            unlock(&f, 1, &id, 1),
            unlock(&f, 1, &id, 2),
            unlock(&f, 2, &other, 2)
        ],
        secrets
    );
    // Existing commands work as before.
    apply(
        &f,
        2,
        &id,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
        },
        None,
    )
    .unwrap();
    // The full account replaces device 1 with device 3, under its key.
    let s = f.account_id(1, &id);
    let request_id = digest("test-operation", &(id, s.account_version));
    let proof = key(3)
        .sign(
            successor_device_message(
                f.user,
                &id,
                &device(3),
                s.account_version,
                request_id,
                Hash::new([1; 32]),
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    let successor = AccountCommand::AddSuccessor {
        device: device(3),
        proof,
    };
    assert_eq!(
        apply(&f, 1, &id, successor.clone(), None),
        Err(Error::Forbidden)
    );
    apply(&f, 1, &id, successor, Some(31)).unwrap();
    assert_eq!(apply(&f, 1, &id, delay(2), Some(31)), Err(Error::Forbidden));
    // Another upgrade keeps the succession.
    let handing_over = f.account_id(1, &id);
    f.ic.advance_time(Duration::from_secs(600));
    f.ic.tick();
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.account_id(1, &id), handing_over);
    assert_eq!(apply(&f, 1, &id, delay(2), Some(31)), Err(Error::Forbidden));
    apply(
        &f,
        1,
        &id,
        AccountCommand::SetApprovalKey {
            device_id: Hash::new([3; 32]),
            key: Some(key_input(&f, &id, 3, 33)),
        },
        Some(31),
    )
    .unwrap();
}
