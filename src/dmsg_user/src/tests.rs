use super::*;
use dmsg_runtime::Budget;
use ed25519_dalek::{Signer, SigningKey};
use std::collections::BTreeMap;

const NAMESPACE: &str = "https://dmsg.test/u/";
#[derive(Clone, Debug, PartialEq, Eq)]
struct TestAccount {
    account: AccountState,
    executions: BTreeMap<OpId, AuthorizedExecution>,
}

impl std::ops::Deref for TestAccount {
    type Target = AccountState;

    fn deref(&self) -> &AccountState {
        &self.account
    }
}

impl std::ops::DerefMut for TestAccount {
    fn deref_mut(&mut self) -> &mut AccountState {
        &mut self.account
    }
}

fn test_init() -> UserInit {
    UserInit {
        commerce_canister: Principal::from_slice(&[88]),
        membership_canister: Principal::from_slice(&[89]),
        environment: Environment::Local,
        issuer_namespace: NAMESPACE.into(),
        home_cose: p(2),
        handle_canister: p(7),
        payment_canister: p(8),
        max_accounts: 100,
        daily_new_accounts: 10,
        principal_origin: "https://id.dmsg.test".into(),
        directory_canister: p(9),
        governance: p(10),
        admission_key: None,
    }
}

fn p(n: u8) -> Principal {
    Principal::from_slice(&[n, 1])
}

fn sk(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

fn device(n: u8, admin: bool) -> DeviceInput {
    DeviceInput {
        device_id: Hash::new([n; 32]),
        signing_pub: sk(n).verifying_key().to_bytes().into(),
        hpke_pub: Hash::new([n; 32]),
        role: if admin {
            ControllerRole::Administrator
        } else {
            ControllerRole::Member
        },
        capabilities: if admin {
            vec![
                Capability::RootManage,
                Capability::FormalApprove,
                Capability::VaultUnlock,
                Capability::PaymentOffer,
            ]
        } else {
            vec![Capability::ContentSign]
        },
    }
}

fn fixture() -> TestAccount {
    let input = CreateAccount {
        device: device(1, true),
        op_id: Hash::new([9; 32]),
        expires_at: MINUTE,
        proof: sk(1)
            .sign(
                digest(
                    "dmsg/create-account/v1",
                    &(p(5), p(1), device(1, true), Hash::new([9; 32]), MINUTE),
                )
                .as_slice(),
            )
            .to_bytes()
            .into(),
        admission: None,
    };
    TestAccount {
        account: account::create(p(5), p(6), AccountId([8; 12]), p(1), &input, 1).unwrap(),
        executions: BTreeMap::new(),
    }
}

fn mutation(s: &AccountState, command: AccountCommand, n: u8, time: u64) -> AccountMutation {
    let mut m = AccountMutation {
        account_id: s.account_id,
        expected_version: s.account_version,
        command,
        approval: Approval {
            device_id: Hash::new([n; 32]),
            security_epoch: s.security_epoch,
            sequence: s.devices[&Hash::new([n; 32])].next_sequence,
            request_id: digest("test", &(s.account_version, n)),
            expires_at: time + MINUTE,
            signature: Default::default(),
        },
    };
    m.approval.signature = sk(n)
        .sign(
            approval_message(
                s.home_user,
                &s.account_id,
                "dmsg/account/v2",
                &(&m.expected_version, &m.command),
                &m.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    m
}

fn apply(s: &mut AccountState, command: AccountCommand, time: u64) -> Result<OperationReceipt> {
    let m = mutation(s, command, 1, time);
    account::apply(s, p(1), &m, time, p(7))
}

/// A generation-`generation` root reference wrapped to the account's root
/// recipients and the vetKD identity, with an arbitrary body digest.
fn root_ref(s: &AccountState, generation: u64) -> ContentRootRef {
    let recipients_digest = root_recipients_digest(&s.root_recipients(), generation);
    let body_digest = Hash::new([generation as u8; 32]);
    ContentRootRef {
        generation,
        suite: "dmsg-root-v2".into(),
        bundle_digest: root_bundle_digest(recipients_digest, body_digest),
        recipients_digest,
        body_digest,
    }
}

fn committed(s: &mut AccountState, time: u64) -> ContentRootRef {
    committed_by(s, 1, time)
}

/// Reserve and commit the next root generation with administrator device `n`.
fn committed_by(s: &mut AccountState, n: u8, time: u64) -> ContentRootRef {
    let expected = s.current_root.as_ref().map_or(0, |r| r.generation);
    let op_id = digest("reserve", &(expected, time));
    let reserve = mutation(
        s,
        AccountCommand::ReserveRoot {
            expected_generation: expected,
            op_id,
        },
        n,
        time,
    );
    account::apply(s, p(n), &reserve, time, p(7)).unwrap();
    let root = root_ref(s, s.root_slot.as_ref().unwrap().generation);
    let commit = mutation(
        s,
        AccountCommand::CommitRoot {
            expected_generation: expected,
            op_id,
            root: root.clone(),
        },
        n,
        time + 1,
    );
    account::apply(s, p(n), &commit, time + 1, p(7)).unwrap();
    root
}

fn statement(s: &AccountState) -> Statement {
    Statement {
        issuer: account_issuer(NAMESPACE, &s.account_id),
        subject: Some("release".into()),
        issued_at: None,
        content: StatementContent::Text("Approved release v1".into()),
    }
}

fn attest_request(s: &AccountState, n: u8, statement: Statement, time: u64) -> AttestRequest {
    let sequence = s.devices[&Hash::new([n; 32])].next_sequence;
    let prepared =
        prepare_attestation(&statement, &sk(n).verifying_key().to_bytes().into()).unwrap();
    let mut r = AttestRequest {
        account_id: s.account_id,
        statement,
        origin: "https://example.com".into(),
        signature: sk(n).sign(&prepared.to_be_signed).to_bytes().into(),
        approval: Approval {
            device_id: Hash::new([n; 32]),
            security_epoch: s.security_epoch,
            sequence,
            request_id: execution_request_id(
                &s.account_id,
                s.security_epoch,
                Hash::new([n; 32]),
                sequence,
            ),
            expires_at: time + MINUTE,
            signature: Default::default(),
        },
    };
    r.approval.signature = sk(n)
        .sign(
            approval_message(
                s.home_user,
                &r.account_id,
                ATTEST_APPROVAL_DOMAIN,
                &attest_approval_command(&r.statement, &r.origin, &r.signature),
                &r.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    r
}

fn attest_fingerprint(r: &AttestRequest) -> Hash {
    digest(
        "dmsg/attest-request/v1",
        &(
            &r.account_id,
            &r.statement,
            &r.origin,
            &r.signature,
            &r.approval,
        ),
    )
}

fn attest(s: &mut TestAccount, caller: Principal, r: &AttestRequest, now: u64) -> Result<SignedArtifact> {
    let fingerprint = attest_fingerprint(r);
    if let Some(e) = s.executions.get(&r.approval.request_id) {
        ensure(s.auth_bindings.contains(&caller), Error::AuthRequired)?;
        ensure(e.command_digest == fingerprint, Error::IdempotencyConflict)?;
        let ExecutionRecord::Attestation(a) = &e.record else {
            panic!()
        };
        return Ok(a.artifact.clone());
    }
    let checked = execution::check_attestation(
        &s.account,
        caller,
        &r.account_id,
        &r.statement,
        &r.origin,
        &r.signature,
        &r.approval,
        fingerprint,
        now,
        &test_init(),
    )?;
    let e = execution::commit_attestation(
        &mut s.account,
        &r.approval,
        r.origin.clone(),
        &r.signature,
        checked,
        fingerprint,
        now,
    )?;
    s.account
        .execution_expirations
        .retain(|_, deadline| deadline.is_none_or(|at| at > now));
    s.executions
        .retain(|id, _| s.account.execution_expirations.contains_key(id));
    let ExecutionRecord::Attestation(a) = &e.record else {
        panic!()
    };
    let artifact = a.artifact.clone();
    s.executions.insert(e.request_id, e);
    Ok(artifact)
}

// Standard compressed BLS12-381 G1 generator; no production transport key.
fn transport() -> serde_bytes::ByteArray<48> {
    let generator = "97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb";
    let bytes: [u8; 48] = (0..generator.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&generator[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    bytes.into()
}

fn derive_request(s: &AccountState, n: u8, generation: u64, time: u64) -> DeriveRootRequest {
    let sequence = s.devices[&Hash::new([n; 32])].next_sequence;
    let mut r = DeriveRootRequest {
        account_id: s.account_id,
        generation,
        transport_public_key: transport(),
        max_cycles: 70_000_000_000,
        approval: Approval {
            device_id: Hash::new([n; 32]),
            security_epoch: s.security_epoch,
            sequence,
            request_id: execution_request_id(
                &s.account_id,
                s.security_epoch,
                Hash::new([n; 32]),
                sequence,
            ),
            expires_at: time + MINUTE,
            signature: Default::default(),
        },
    };
    r.approval.signature = sk(n)
        .sign(
            approval_message(
                s.home_user,
                &r.account_id,
                DERIVE_APPROVAL_DOMAIN,
                &derive_approval_command(&r),
                &r.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    r
}

fn derive(s: &mut TestAccount, caller: Principal, r: &DeriveRootRequest, now: u64) -> Result<AuthorizedExecution> {
    let fingerprint = digest("dmsg/derive-request/v1", r);
    if let Some(e) = s.executions.get(&r.approval.request_id) {
        ensure(s.auth_bindings.contains(&caller), Error::AuthRequired)?;
        ensure(e.command_digest == fingerprint, Error::IdempotencyConflict)?;
        return Ok(e.clone());
    }
    let budget = execution::check_derivation(&s.account, caller, r, fingerprint, now)?;
    let e = execution::commit_derivation(&mut s.account, r.clone(), fingerprint, budget, now);
    s.executions.insert(e.request_id, e.clone());
    Ok(e)
}

fn record_execution_response(
    s: &mut TestAccount,
    request_id: OpId,
    response: Result<ExecutionResult>,
) -> Result<ExecutionResult> {
    let e = s.executions.get_mut(&request_id).ok_or(Error::NotFound)?;
    let ExecutionRecord::Derivation { result, .. } = &e.record else {
        panic!()
    };
    let previous = result.clone();
    let result = execution::record_execution_response(e, response);
    // As in api::record_response, only a new terminal result settles once.
    if result != previous && result.is_terminal() {
        let ExecutionRecord::Derivation { grant, .. } = &e.record else {
            panic!()
        };
        execution::settle_budget(&mut s.account, grant, &result);
        s.account
            .execution_expirations
            .insert(request_id, Some(e.retention()));
    }
    Ok(result)
}

fn completed(r: &DeriveRootRequest, s: &AccountState) -> ExecutionResult {
    ExecutionResult {
        request_id: r.approval.request_id,
        cycles_cost_upper_bound: 1,
        cycles_charged: 0,
        outcome: ExecutionOutcome::Completed(EncryptedRootKey {
            encrypted_key: vec![1; 192].into(),
            key: KeyDescriptor {
                account_id: s.account_id,
                home_cose: s.home_cose,
                master_key_name: "key_1".into(),
                environment: Environment::Local,
                derivation_version: 2,
                key_generation: r.generation,
                public_key: vec![9; 96].into(),
                public_key_fingerprint: Hash::new([10; 32]),
            },
        }),
    }
}

/// Recover the fixture onto device `n` with login `p(n)`.
fn recovered(s: &mut TestAccount, n: u8, now: u64) -> OpId {
    let request = RecoveryRequest {
        op_id: digest("recovery", &(n, now)),
        new_auth: p(n),
        device: device(n, true),
        expires_at: now + 10 * DAY,
    };
    let proof = sk(n)
        .sign(recovery_device_message(s.home_user, &s.account_id, &request).as_slice())
        .to_bytes();
    s.auth_bindings.push(p(n));
    recovery::begin_recovery(&mut s.account, p(n), &request, &proof, now).unwrap();
    let after = s.pending_recovery.as_ref().unwrap().execute_after;
    recovery::complete_recovery(&mut s.account, p(n), request.op_id, after).unwrap();
    request.op_id
}

#[test]
fn root_cas_errors_have_no_partial_writes_and_retry_is_idempotent() {
    let mut s = fixture();
    let m = mutation(
        &s,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id: Hash::new([5; 32]),
        },
        1,
        1,
    );
    let receipt = account::apply(&mut s, p(1), &m, 1, p(7)).unwrap();
    assert_eq!(account::apply(&mut s, p(1), &m, 1, p(7)).unwrap(), receipt);
    let before = s.clone();
    assert_eq!(
        apply(
            &mut s,
            AccountCommand::ReserveRoot {
                expected_generation: 0,
                op_id: Hash::new([6; 32])
            },
            1
        ),
        Err(Error::VersionConflict)
    );
    assert_eq!(s, before);
    let root = root_ref(&s, 1);
    apply(
        &mut s,
        AccountCommand::CommitRoot {
            expected_generation: 0,
            op_id: Hash::new([5; 32]),
            root: root.clone(),
        },
        2,
    )
    .unwrap();
    assert_eq!(s.current_root, Some(root));
    assert_eq!(s.vault_write_state, VaultWriteState::Ready);
}

#[test]
fn commit_root_binds_the_vault_devices_and_vetkd_identity() {
    let mut s = fixture();
    let enroll = |s: &mut TestAccount, input: DeviceInput, revoked_at: Option<u64>| {
        s.devices.insert(
            input.device_id,
            Device {
                input,
                added_at: 1,
                added_by: Some(Hash::new([1; 32])),
                revoked_at,
                next_sequence: 0,
            },
        );
    };
    let vault_member = |n: u8| DeviceInput {
        capabilities: vec![Capability::ContentSign, Capability::VaultUnlock],
        ..device(n, false)
    };
    enroll(&mut s, vault_member(2), None);
    enroll(&mut s, vault_member(3), Some(2));
    // An active device without VaultUnlock, and an administrator without it.
    enroll(&mut s, device(4, false), None);
    let mut admin = device(5, true);
    admin.capabilities.retain(|c| *c != Capability::VaultUnlock);
    enroll(&mut s, admin, None);
    apply(
        &mut s,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id: Hash::new([5; 32]),
        },
        3,
    )
    .unwrap();
    let good = root_ref(&s, 1);
    assert_eq!(
        good.recipients_digest,
        root_recipients_digest(&[Hash::new([1; 32]), Hash::new([2; 32])], 1)
    );
    let commit = |s: &mut TestAccount, root: ContentRootRef| {
        apply(
            s,
            AccountCommand::CommitRoot {
                expected_generation: 0,
                op_id: Hash::new([5; 32]),
                root,
            },
            4,
        )
    };
    let rewrap = |recipients: &[Hash], generation: u64, body: Hash| {
        let recipients_digest = root_recipients_digest(recipients, generation);
        ContentRootRef {
            generation: 1,
            suite: "dmsg-root-v2".into(),
            bundle_digest: root_bundle_digest(recipients_digest, body),
            recipients_digest,
            body_digest: body,
        }
    };
    let before = s.clone();
    // An administrator outside the recipients cannot commit even the right root.
    let foreign = mutation(
        &s,
        AccountCommand::CommitRoot {
            expected_generation: 0,
            op_id: Hash::new([5; 32]),
            root: good.clone(),
        },
        5,
        4,
    );
    assert_eq!(
        account::apply(&mut s, p(1), &foreign, 4, p(7)),
        Err(Error::IntegrityFailed)
    );
    assert_eq!(s, before);
    for bad in [
        // A revoked device, a device without VaultUnlock, a missing device, an
        // unknown device, the wrong generation and the old suite are refused.
        rewrap(
            &[Hash::new([1; 32]), Hash::new([2; 32]), Hash::new([3; 32])],
            1,
            good.body_digest,
        ),
        rewrap(
            &[Hash::new([1; 32]), Hash::new([2; 32]), Hash::new([4; 32])],
            1,
            good.body_digest,
        ),
        rewrap(&[Hash::new([1; 32])], 1, good.body_digest),
        rewrap(
            &[Hash::new([1; 32]), Hash::new([2; 32]), Hash::new([6; 32])],
            1,
            good.body_digest,
        ),
        rewrap(&[Hash::new([1; 32]), Hash::new([2; 32])], 2, good.body_digest),
        ContentRootRef {
            suite: "dmsg-root-v1".into(),
            ..good.clone()
        },
        ContentRootRef {
            bundle_digest: Hash::new([9; 32]),
            ..good.clone()
        },
        ContentRootRef {
            body_digest: Hash::new([0; 32]),
            ..good.clone()
        },
    ] {
        assert!(commit(&mut s, bad).is_err());
        assert_eq!(s, before);
    }
    commit(&mut s, good.clone()).unwrap();
    assert_eq!(s.current_root, Some(good));
}

#[test]
fn abandoned_root_generation_is_never_reused() {
    let mut s = fixture();
    apply(
        &mut s,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id: Hash::new([5; 32]),
        },
        1,
    )
    .unwrap();
    let m = mutation(
        &s,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy::default(),
        },
        1,
        2,
    );
    account::apply(&mut s, p(1), &m, 2, p(7)).unwrap();
    assert!(s.root_slot.is_none());
    apply(
        &mut s,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id: Hash::new([6; 32]),
        },
        3,
    )
    .unwrap();
    assert_eq!(s.root_slot.as_ref().unwrap().generation, 2);
}

#[test]
fn content_device_cannot_administer_or_attest() {
    let mut s = fixture();
    s.devices.insert(
        Hash::new([2; 32]),
        Device {
            input: device(2, false),
            added_at: 1,
            added_by: None,
            revoked_at: None,
            next_sequence: 0,
        },
    );
    let m = mutation(
        &s,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy::default(),
        },
        2,
        1,
    );
    let before = s.clone();
    assert_eq!(
        account::apply(&mut s, p(1), &m, 1, p(7)),
        Err(Error::Forbidden)
    );
    assert_eq!(s, before);
    let req = attest_request(&s, 2, statement(&s), 1);
    assert_eq!(attest(&mut s, p(1), &req, 1), Err(Error::Forbidden));
}

#[test]
fn attestations_bind_statement_origin_signature_and_device() {
    let mut s = fixture();
    let r = attest_request(&s, 1, statement(&s), 1);
    let artifact = attest(&mut s, p(1), &r, 1).unwrap();
    assert_eq!(verify_artifact(&artifact).unwrap(), r.statement);
    assert_eq!((s.budget.executions, s.budget.cycles), (1, 0));
    // An exact retry returns the stored artifact without consuming anything.
    let after = s.clone();
    assert_eq!(attest(&mut s, p(1), &r, 1).unwrap(), artifact);
    assert_eq!(s, after);
    let e = &s.executions[&r.approval.request_id];
    let receipt = execution::receipt(e, NAMESPACE).unwrap();
    assert_eq!(receipt.schema, 2);
    assert_eq!(receipt.device_id, Hash::new([1; 32]));
    match_execution_receipt(&artifact, &receipt).unwrap();
    assert_eq!(
        receipt.public_key_fingerprint,
        key_thumbprint(&artifact.cose_key).unwrap()
    );
    // Tampering with any bound field is refused before any state changes.
    let mut altered = r.clone();
    altered.origin = "https://other.test".into();
    assert_eq!(
        attest(&mut s, p(1), &altered, 1),
        Err(Error::IdempotencyConflict)
    );
    let mut s = fixture();
    let r = attest_request(&s, 1, statement(&s), 1);
    let before = s.clone();
    let mut altered = r.clone();
    altered.statement.content = StatementContent::Text("different".into());
    assert_eq!(attest(&mut s, p(1), &altered, 1), Err(Error::IntegrityFailed));
    let mut altered = r.clone();
    altered.signature = sk(2).sign(b"x").to_bytes().into();
    assert_eq!(attest(&mut s, p(1), &altered, 1), Err(Error::IntegrityFailed));
    let mut forged = attest_request(&s, 1, statement(&s), 1);
    forged.signature = sk(2)
        .sign(
            &prepare_attestation(&forged.statement, &sk(1).verifying_key().to_bytes().into())
                .unwrap()
                .to_be_signed,
        )
        .to_bytes()
        .into();
    forged.approval.signature = sk(1)
        .sign(
            approval_message(
                s.home_user,
                &forged.account_id,
                ATTEST_APPROVAL_DOMAIN,
                &attest_approval_command(&forged.statement, &forged.origin, &forged.signature),
                &forged.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    assert_eq!(attest(&mut s, p(1), &forged, 1), Err(Error::IntegrityFailed));
    assert_eq!(s, before);
    s.devices.get_mut(&Hash::new([1; 32])).unwrap().revoked_at = Some(1);
    assert_eq!(attest(&mut s, p(1), &r, 1), Err(Error::DeviceNotApproved));
    let mut s = fixture();
    s.sensitive_policy.allowed_purposes = vec![KeyPurpose::FileAttestation];
    assert_eq!(attest(&mut s, p(1), &r, 1), Err(Error::Forbidden));
    let mut s = fixture();
    s.sensitive_policy.frozen = true;
    assert_eq!(attest(&mut s, p(1), &r, 1), Err(Error::Locked));
}

#[test]
fn attestations_count_against_the_daily_policy_and_the_window() {
    let mut s = fixture();
    s.sensitive_policy.daily_executions = 2;
    for _ in 0..2 {
        let r = attest_request(&s, 1, statement(&s), 1);
        attest(&mut s, p(1), &r, 1).unwrap();
    }
    let refused = attest_request(&s, 1, statement(&s), 1);
    let before = s.clone();
    assert_eq!(
        attest(&mut s, p(1), &refused, 1),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(s, before);
    let tomorrow = attest_request(&s, 1, statement(&s), DAY + 1);
    attest(&mut s, p(1), &tomorrow, DAY + 1).unwrap();

    let mut s = fixture();
    s.sensitive_policy.daily_executions = 100;
    let mut requests = vec![];
    for _ in 0..dmsg_runtime::WINDOW {
        let r = attest_request(&s, 1, statement(&s), 1);
        attest(&mut s, p(1), &r, 1).unwrap();
        requests.push(r);
    }
    let next = attest_request(&s, 1, statement(&s), 2);
    assert_eq!(attest(&mut s, p(1), &next, 2), Err(Error::QuotaExceeded));
    // Results retire a day after their approval expired; receipts stay.
    let later = 2 * DAY;
    let next = attest_request(&s, 1, statement(&s), later);
    attest(&mut s, p(1), &next, later).unwrap();
    assert_eq!(s.executions.len(), 1);
    assert_eq!(
        attest(&mut s, p(1), &requests[0], later),
        Err(Error::ResultExpired)
    );
}

#[test]
fn login_recovery_waits_for_the_delay_and_a_dispute_cancels_it() {
    let mut s = fixture();
    committed(&mut s, 1);
    let request = RecoveryRequest {
        op_id: Hash::new([4; 32]),
        new_auth: p(4),
        device: device(4, true),
        expires_at: 10 * DAY,
    };
    let proof = sk(4)
        .sign(recovery_device_message(s.home_user, &s.account_id, &request).as_slice())
        .to_bytes();
    // Only a bound login may start a takeover.
    assert_eq!(
        recovery::begin_recovery(&mut s, p(4), &request, &proof, 1),
        Err(Error::AuthRequired)
    );
    s.auth_bindings.push(p(4));
    let mut wrong = proof;
    wrong[0] ^= 1;
    assert_eq!(
        recovery::begin_recovery(&mut s, p(4), &request, &wrong, 1),
        Err(Error::IntegrityFailed)
    );
    recovery::begin_recovery(&mut s, p(4), &request, &proof, 1).unwrap();
    assert_eq!(
        s.pending_recovery.as_ref().unwrap().execute_after,
        1 + DEFAULT_RECOVERY_DELAY_MS
    );
    assert_eq!(
        s.snapshot(NAMESPACE).pending_recovery_digest,
        Some(digest(
            "dmsg/pending-recovery/v1",
            s.pending_recovery.as_ref().unwrap()
        ))
    );
    // A retry keeps the original delay; completing early fails.
    recovery::begin_recovery(&mut s, p(4), &request, &proof, 2).unwrap();
    assert_eq!(
        recovery::complete_recovery(&mut s, p(4), request.op_id, DAY),
        Err(Error::Expired)
    );
    // Any active device cancels the takeover outright.
    apply(
        &mut s,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
        },
        3,
    )
    .unwrap();
    assert!(s.pending_recovery.is_none());
    assert_eq!(
        recovery::complete_recovery(&mut s, p(4), request.op_id, 4 * DAY),
        Err(Error::NotFound)
    );
    // Without a dispute the replacement device takes over after the delay.
    recovery::begin_recovery(&mut s, p(4), &request, &proof, 4).unwrap();
    let after = s.pending_recovery.as_ref().unwrap().execute_after;
    assert_eq!(
        recovery::complete_recovery(&mut s, p(5), request.op_id, after),
        Err(Error::AuthRequired)
    );
    let epoch = s.security_epoch;
    recovery::complete_recovery(&mut s, p(4), request.op_id, after).unwrap();
    assert_eq!(s.auth_bindings, vec![p(4)]);
    assert_eq!(s.devices.len(), 1);
    assert_eq!(s.security_epoch, epoch + 1);
    assert_eq!(s.vault_write_state, VaultWriteState::RekeyRequired);
    assert_eq!(
        s.info(NAMESPACE).recovered_device,
        Some((Hash::new([4; 32]), 1))
    );
    let done = s.clone();
    assert_eq!(
        recovery::complete_recovery(&mut s, p(4), request.op_id, after + 1),
        Ok(None)
    );
    assert_eq!(s, done);
    assert_eq!(
        recovery::begin_recovery(&mut s, p(4), &request, &proof, after),
        Err(Error::IdempotencyConflict)
    );
    assert_eq!(s, done);
}

#[test]
fn recovery_delay_is_bounded_and_frozen_while_a_takeover_is_pending() {
    let mut s = fixture();
    for delay in [DAY - 1, 8 * DAY] {
        assert!(apply(
            &mut s,
            AccountCommand::SetRecoveryDelay { delay_ms: delay },
            1
        )
        .is_err());
    }
    let epoch = s.security_epoch;
    apply(&mut s, AccountCommand::SetRecoveryDelay { delay_ms: DAY }, 1).unwrap();
    assert_eq!(s.recovery_delay_ms, DAY);
    assert_eq!(s.snapshot(NAMESPACE).recovery_delay_ms, DAY);
    assert_eq!(s.security_epoch, epoch + 1);
    assert_eq!(s.vault_write_state, VaultWriteState::Uninitialized);
    let request = RecoveryRequest {
        op_id: Hash::new([4; 32]),
        new_auth: p(4),
        device: device(4, true),
        expires_at: 10 * DAY,
    };
    let proof = sk(4)
        .sign(recovery_device_message(s.home_user, &s.account_id, &request).as_slice())
        .to_bytes();
    s.auth_bindings.push(p(4));
    recovery::begin_recovery(&mut s, p(4), &request, &proof, 2).unwrap();
    assert_eq!(
        apply(
            &mut s,
            AccountCommand::SetRecoveryDelay { delay_ms: 2 * DAY },
            3
        ),
        Err(Error::Pending)
    );
}

#[test]
fn removing_the_requesting_login_voids_its_takeover() {
    let mut s = fixture();
    let request = RecoveryRequest {
        op_id: Hash::new([4; 32]),
        new_auth: p(4),
        device: device(4, true),
        expires_at: 10 * DAY,
    };
    let proof = sk(4)
        .sign(recovery_device_message(s.home_user, &s.account_id, &request).as_slice())
        .to_bytes();
    s.auth_bindings.push(p(4));
    recovery::begin_recovery(&mut s, p(4), &request, &proof, 2).unwrap();
    let after = s.pending_recovery.as_ref().unwrap().execute_after;
    // A device unbinds the login, e.g. a stolen one, without disputing.
    apply(&mut s, AccountCommand::RemoveAuth { principal: p(4) }, 3).unwrap();
    assert!(s.pending_recovery.is_none());
    assert_eq!(
        recovery::complete_recovery(&mut s, p(4), request.op_id, after),
        Err(Error::NotFound)
    );
    assert_eq!(s.devices.len(), 1);
}

#[test]
fn a_recovered_device_derives_the_committed_root_until_it_rekeys() {
    let mut s = fixture();
    committed(&mut s, 1);
    let now = 10 * DAY;
    // Only a recovered device may derive, and only the generation it recovered.
    let unrecovered = derive_request(&s, 1, 1, now);
    assert_eq!(
        derive(&mut s, p(1), &unrecovered, now),
        Err(Error::Forbidden)
    );
    recovered(&mut s, 4, now);
    let at = now + 4 * DAY;
    let other_generation = derive_request(&s, 4, 2, at);
    assert_eq!(
        derive(&mut s, p(4), &other_generation, at),
        Err(Error::Forbidden)
    );
    let mut bad = derive_request(&s, 4, 1, at);
    bad.transport_public_key = [1; 48].into();
    assert_eq!(
        derive(&mut s, p(4), &bad, at),
        Err(Error::IntegrityFailed)
    );
    let r = derive_request(&s, 4, 1, at);
    let e = derive(&mut s, p(4), &r, at).unwrap();
    let ExecutionRecord::Derivation { grant, result } = &e.record else {
        panic!()
    };
    assert_eq!((grant.generation, grant.execution_sequence), (1, 1));
    assert_eq!(result.status(), ExecutionStatus::Authorized);
    assert_eq!(s.budget.executions, 1);
    let before = s.clone();
    assert_eq!(derive(&mut s, p(4), &r, at).unwrap(), e);
    assert_eq!(s, before, "an exact retry consumes no extra budget");
    // A failed derivation returns the count; the device may try again.
    let failed = ExecutionResult {
        request_id: r.approval.request_id,
        outcome: ExecutionOutcome::Failed(Error::Unavailable("rejected".into())),
        cycles_cost_upper_bound: 7,
        cycles_charged: 0,
    };
    record_execution_response(&mut s, r.approval.request_id, Ok(failed)).unwrap();
    assert_eq!(s.budget.executions, 0);
    let again = derive_request(&s, 4, 1, at);
    derive(&mut s, p(4), &again, at).unwrap();
    let mut wrong_key = completed(&again, &s);
    if let ExecutionOutcome::Completed(output) = &mut wrong_key.outcome {
        output.key.key_generation = 2;
    }
    assert_eq!(
        record_execution_response(&mut s, again.approval.request_id, Ok(wrong_key))
            .unwrap()
            .outcome,
        ExecutionOutcome::Unknown(Error::IntegrityFailed)
    );
    let done = completed(&again, &s);
    assert_eq!(
        record_execution_response(&mut s, again.approval.request_id, Ok(done.clone())),
        Ok(done.clone())
    );
    assert_eq!(s.budget.executions, 1);
    assert_eq!(
        record_execution_response(&mut s, again.approval.request_id, Err(Error::ExecutionUnknown)),
        Ok(done)
    );
    // A completed derivation does not end the right: a new approval derives
    // again, counted against the daily executions.
    let later = derive_request(&s, 4, 1, at);
    derive(&mut s, p(4), &later, at).unwrap();
    assert_eq!(s.budget.executions, 2);
    // Committing a new root ends the derivation right.
    committed_by(&mut s.account, 4, at);
    assert_eq!(s.info(NAMESPACE).recovered_device, None);
    let rekeyed = derive_request(&s, 4, 2, at);
    assert_eq!(derive(&mut s, p(4), &rekeyed, at), Err(Error::Forbidden));
}

#[test]
fn attestations_leave_the_execution_sequence_to_derivations() {
    let mut s = fixture();
    committed(&mut s, 1);
    for at in 10..13 {
        let r = attest_request(&s, 1, statement(&s), at);
        attest(&mut s, p(1), &r, at).unwrap();
    }
    // COSE closes its window only over sequences it executes, so attestations
    // must not open gaps ahead of the next derivation.
    assert_eq!(s.next_execution_sequence, 1);
    recovered(&mut s, 4, 10 * DAY);
    let at = 14 * DAY;
    let r = derive_request(&s, 4, 1, at);
    let e = derive(&mut s, p(4), &r, at).unwrap();
    let ExecutionRecord::Derivation { grant, .. } = &e.record else {
        panic!()
    };
    assert_eq!(grant.execution_sequence, 1);
}

#[test]
fn budget_failure_is_atomic() {
    let mut budget = Budget::default();
    budget.count(1, 1).unwrap();
    let before = budget.clone();
    assert_eq!(budget.count(1, 1), Err(Error::QuotaExceeded));
    assert_eq!(budget, before);
}

#[test]
fn snapshot_contains_no_auth_routes() {
    let s = fixture();
    let snapshot = canonical(&s.snapshot(NAMESPACE));
    let map: BTreeMap<String, cbor2::Value> = cbor2::from_slice(&snapshot).unwrap();
    assert!(!map.contains_key("auth_bindings"));
    assert_eq!(
        map["recovery_delay_ms"],
        cbor2::Value::Integer(DEFAULT_RECOVERY_DELAY_MS.into())
    );
}

#[test]
fn a_fresh_approval_cannot_repurpose_a_cleaned_request_id() {
    let mut s = fixture();
    let first = attest_request(&s, 1, statement(&s), 1);
    attest(&mut s, p(1), &first, 1).unwrap();
    for at in 10..75 {
        apply(
            &mut s,
            AccountCommand::SetPolicy {
                policy: SensitivePolicy::default(),
            },
            at,
        )
        .unwrap();
    }
    let next = attest_request(&s, 1, statement(&s), 2 * DAY);
    attest(&mut s, p(1), &next, 2 * DAY).unwrap();
    assert!(!s.executions.contains_key(&first.approval.request_id));
    let mut reused = attest_request(&s, 1, statement(&s), 2 * DAY + 1);
    reused.approval.request_id = first.approval.request_id;
    reused.approval.signature = sk(1)
        .sign(
            approval_message(
                s.home_user,
                &reused.account_id,
                ATTEST_APPROVAL_DOMAIN,
                &attest_approval_command(&reused.statement, &reused.origin, &reused.signature),
                &reused.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    assert_eq!(
        attest(&mut s, p(1), &reused, 2 * DAY + 1),
        Err(Error::IdempotencyConflict)
    );
}

#[test]
fn stored_callbacks_preserve_concurrent_account_changes_and_other_executions() {
    let mut s = fixture();
    committed(&mut s, 1);
    recovered(&mut s, 4, 10 * DAY);
    let at = 14 * DAY;
    CONFIG.with_borrow_mut(|t| {
        t.set(CompactStored::new(&Some(Config {
            schema: STABLE_SCHEMA,
            init: UserInit {
                home_cose: s.home_cose,
                ..test_init()
            },
            allocator: XidGenerator::new([1; 5]),
            allocator_namespace_digest: Hash::new([1; 32]),
            day: 0,
            created_today: 0,
            master_secret: Some(Hash::new([7; 32])),
        })))
    });
    let request = derive_request(&s, 4, 1, at);
    let first = derive(&mut s, p(4), &request, at).unwrap();
    save_execution(&first);
    let attested = attest_request(&s, 4, statement(&s), at);
    attest(&mut s, p(4), &attested, at).unwrap();
    let second = s.executions[&attested.approval.request_id].clone();
    save_execution(&second);

    // A device policy change commits while the derivation is pending.
    let m = mutation(
        &s,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy {
                frozen: true,
                ..SensitivePolicy::default()
            },
        },
        4,
        at + 1,
    );
    account::apply(&mut s, p(4), &m, at + 1, p(7)).unwrap();
    save(&s.account);
    let before = load(&s.account_id).unwrap();
    let snapshot = CERT.with_borrow(|c| c.get(s.account_id.as_slice()));
    let completed = completed(&request, &s);
    let mut mismatched = completed.clone();
    mismatched.request_id = attested.approval.request_id;
    assert_eq!(
        record_response(&s.account_id, request.approval.request_id, Ok(mismatched))
            .unwrap()
            .outcome,
        ExecutionOutcome::Unknown(Error::IntegrityFailed)
    );
    assert_eq!(load(&s.account_id).unwrap(), before);
    assert_eq!(
        record_response(
            &s.account_id,
            request.approval.request_id,
            Ok(completed.clone())
        ),
        Ok(completed.clone())
    );
    let mut expected = before;
    expected
        .execution_expirations
        .insert(request.approval.request_id, Some(first.retention()));
    assert_eq!(load(&s.account_id).unwrap(), expected);
    assert_eq!(
        load_execution(&s.account_id, &attested.approval.request_id),
        Some(second.clone())
    );
    assert_eq!(
        CERT.with_borrow(|c| c.get(s.account_id.as_slice())),
        snapshot
    );
    // The attestation's receipt leaf is certified; the derivation has none.
    let receipt_key = execution_receipt_key(&s.account_id, attested.approval.request_id);
    assert_eq!(
        CERT.with_borrow(|c| c.get(&receipt_key)),
        Some(dmsg_runtime::cert_map::leaf_hash(&canonical(
            &execution::receipt(&second, NAMESPACE).unwrap()
        )))
    );
    assert_eq!(
        CERT.with_borrow(|c| c.get(&execution_receipt_key(
            &s.account_id,
            request.approval.request_id
        ))),
        None
    );
    assert_eq!(
        record_response(
            &s.account_id,
            request.approval.request_id,
            Err(Error::ExecutionUnknown)
        ),
        Ok(completed)
    );
    remove_execution(&s.account_id, &attested.approval.request_id);
    assert_eq!(CERT.with_borrow(|c| c.get(&receipt_key)), None);
    assert_eq!(
        record_response(
            &s.account_id,
            attested.approval.request_id,
            Err(Error::ExecutionUnknown)
        ),
        Err(Error::ResultExpired)
    );
    assert_eq!(
        crate::store::unlock_secret(&config(), &s.account_id, &Hash::new([4; 32])).unwrap(),
        digest(
            "dmsg/unlock-secret/v1",
            &(Hash::new([7; 32]), &s.account_id, Hash::new([4; 32]))
        )
    );
}

#[test]
fn existing_handle_intent_can_be_reapproved_at_capacity() {
    let mut s = fixture();
    for n in 0..32 {
        let intent = dmsg_types::handle::HandleIntent {
            handle_canister: p(7),
            action: dmsg_types::handle::HandleAction::Register,
            account_id: s.account_id,
            target_account: None,
            handle: format!("user{n}"),
            expected_version: 0,
            op_id: Hash::new([n + 1; 32]),
            terms_digest: Hash::new([1; 32]),
        };
        apply(&mut s, AccountCommand::AuthorizeHandle { intent }, 1).unwrap();
    }
    let intent = s
        .handle_authorizations
        .values()
        .next()
        .unwrap()
        .intent
        .clone();
    apply(
        &mut s,
        AccountCommand::AuthorizeHandle {
            intent: intent.clone(),
        },
        1,
    )
    .unwrap();
    assert_eq!(s.handle_authorizations.len(), 32);
    let mut altered = intent;
    altered.terms_digest = Hash::new([2; 32]);
    let before = s.clone();
    assert_eq!(
        apply(
            &mut s,
            AccountCommand::AuthorizeHandle { intent: altered },
            1
        ),
        Err(Error::IdempotencyConflict)
    );
    assert_eq!(s, before);
}

#[test]
fn device_capability_changes_are_administrator_only_atomic_and_rekey_for_vault_access() {
    let mut s = fixture();
    committed(&mut s, 1);
    let device_id = Hash::new([1; 32]);
    let original = s.devices[&device_id].input.clone();
    // Other capabilities leave the root recipients, and the root, unchanged.
    let epoch = s.security_epoch;
    apply(
        &mut s,
        AccountCommand::SetDeviceCapabilities {
            device_id,
            capabilities: vec![Capability::RootManage, Capability::VaultUnlock],
        },
        2,
    )
    .unwrap();
    assert_eq!(s.security_epoch, epoch + 1);
    assert_eq!(s.vault_write_state, VaultWriteState::Ready);
    // Losing VaultUnlock drops the device from the recipients.
    let command = AccountCommand::SetDeviceCapabilities {
        device_id,
        capabilities: vec![Capability::RootManage, Capability::ContentSign],
    };
    let request = mutation(&s, command, 1, 3);
    let epoch = s.security_epoch;
    let receipt = account::apply(&mut s, p(1), &request, 3, p(7)).unwrap();
    assert_eq!(s.security_epoch, epoch + 1);
    assert_eq!(s.vault_write_state, VaultWriteState::RekeyRequired);
    assert_eq!(
        s.devices[&device_id].input.signing_pub,
        original.signing_pub
    );
    assert_eq!(s.devices[&device_id].input.hpke_pub, original.hpke_pub);
    assert_eq!(
        account::apply(&mut s, p(1), &request, 3, p(7)).unwrap(),
        receipt
    );
    let before = s.clone();
    assert!(apply(
        &mut s,
        AccountCommand::SetDeviceCapabilities {
            device_id,
            capabilities: vec![Capability::FormalApprove],
        },
        4
    )
    .is_err());
    assert_eq!(s, before, "last administrator cannot lose RootManage");
    let mut denied = mutation(
        &s,
        AccountCommand::SetDeviceCapabilities {
            device_id,
            capabilities: original.capabilities,
        },
        1,
        4,
    );
    s.devices.get_mut(&device_id).unwrap().input.role = ControllerRole::Member;
    denied.expected_version = s.account_version;
    let before = s.clone();
    assert!(account::apply(&mut s, p(1), &denied, 4, p(7)).is_err());
    assert_eq!(s, before);
}

/// Certified values are rebuilt from records at query time and checked against
/// leaf hashes that upgrades keep, so a changed encoding traps every query of
/// an existing leaf. These digests pin the encodings: changing one requires
/// recertifying every stored leaf, not just new code.
#[test]
fn certified_value_encodings_are_pinned() {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let mut s = fixture();
    committed(&mut s, 1);
    s.pending_recovery = Some(PendingRecovery {
        request: RecoveryRequest {
            op_id: Hash::new([4; 32]),
            new_auth: p(1),
            device: device(4, true),
            expires_at: 10 * DAY,
        },
        execute_after: 3 * DAY,
    });
    s.principal_updated_at = Some(5);
    assert_eq!(
        hex(sha256(&canonical(&s.snapshot(NAMESPACE))).as_slice()),
        "79f84b8d778882c59562ce755f86e956b5ff94bc103809d700eca5491daee132"
    );
    let attestation = AuthorizedExecution {
        account_id: s.account_id,
        request_id: Hash::new([6; 32]),
        command_digest: Hash::new([7; 32]),
        record: ExecutionRecord::Attestation(Attestation {
            device_id: Hash::new([1; 32]),
            security_epoch: 2,
            approved_at: 3,
            expires_at: 4 + MINUTE,
            origin: "https://example.com".into(),
            to_be_signed_digest: Hash::new([8; 32]),
            public_key_fingerprint: Hash::new([9; 32]),
            signature_digest: Hash::new([10; 32]),
            artifact: SignedArtifact {
                cose_sign1: vec![1].into(),
                cose_key: vec![2].into(),
            },
        }),
    };
    let receipt = execution::receipt(&attestation, NAMESPACE).unwrap();
    assert_eq!(
        hex(sha256(&canonical(&receipt)).as_slice()),
        "2422dd5aaa43efbdd11e4e22a8d7963171ecf63518c68fd0ef1481e03edf85ff"
    );
}

/// Enroll `input` with its own proof, approved by administrator 1.
fn add_device(s: &mut AccountState, input: DeviceInput, time: u64) -> Result<OperationReceipt> {
    let request_id = digest("test", &(s.account_version, 1u8));
    let proof = SigningKey::from_bytes(&[input.device_id[0]; 32])
        .sign(
            digest(
                "dmsg/add-device/v1",
                &(
                    s.home_user,
                    &s.account_id,
                    &input,
                    s.account_version,
                    request_id,
                ),
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    apply(
        s,
        AccountCommand::AddDevice {
            device: input,
            proof,
        },
        time,
    )
}

#[test]
fn only_a_change_of_root_recipients_requires_a_rekey() {
    let mut s = fixture();
    committed(&mut s, 1);
    // Logins are not recipients: binding or removing one keeps the root in use.
    let epoch = s.security_epoch;
    apply(
        &mut s,
        AccountCommand::BindAuth {
            principal: p(4),
            nonce: Hash::new([4; 32]),
        },
        3,
    )
    .unwrap();
    account::accept_binding(&mut s, p(4), Hash::new([4; 32]), 3).unwrap();
    apply(&mut s, AccountCommand::RemoveAuth { principal: p(4) }, 4).unwrap();
    assert_eq!(s.security_epoch, epoch + 2);
    assert_eq!(s.vault_write_state, VaultWriteState::Ready);
    // A device without VaultUnlock never holds the root.
    add_device(&mut s, device(2, false), 5).unwrap();
    let removed = AccountCommand::RevokeDevice {
        device_id: Hash::new([2; 32]),
    };
    apply(&mut s, removed, 6).unwrap();
    assert_eq!(s.vault_write_state, VaultWriteState::Ready);
    // A device holding VaultUnlock must receive the next root, and losing it rekeys again.
    let vault = DeviceInput {
        capabilities: vec![Capability::ContentSign, Capability::VaultUnlock],
        ..device(3, false)
    };
    add_device(&mut s, vault, 7).unwrap();
    assert_eq!(s.vault_write_state, VaultWriteState::RekeyRequired);
    committed(&mut s, 8);
    assert_eq!(s.vault_write_state, VaultWriteState::Ready);
    let revoked = AccountCommand::RevokeDevice {
        device_id: Hash::new([3; 32]),
    };
    apply(&mut s, revoked, 10).unwrap();
    assert_eq!(s.vault_write_state, VaultWriteState::RekeyRequired);

    // The initial and the recovered device must be able to hold the root.
    let mut keyless = device(1, true);
    keyless
        .capabilities
        .retain(|c| *c != Capability::VaultUnlock);
    let create = CreateAccount {
        device: keyless.clone(),
        op_id: Hash::new([9; 32]),
        expires_at: MINUTE,
        proof: Default::default(),
        admission: None,
    };
    assert!(account::create(p(5), p(6), AccountId([7; 12]), p(1), &create, 1).is_err());
    let request = RecoveryRequest {
        op_id: Hash::new([4; 32]),
        new_auth: p(1),
        device: keyless,
        expires_at: 10 * DAY,
    };
    let before = s.clone();
    assert!(recovery::begin_recovery(&mut s, p(1), &request, &[], 11).is_err());
    assert_eq!(s, before);
}

#[test]
fn unsent_dispatch_preserves_prior_uncertainty_and_concurrent_completion() {
    let mut s = fixture();
    committed(&mut s, 1);
    recovered(&mut s, 4, 10 * DAY);
    let at = 14 * DAY;
    let request = derive_request(&s, 4, 1, at);
    let execution = derive(&mut s, p(4), &request, at).unwrap();
    let ExecutionRecord::Derivation { result, .. } = &execution.record else {
        panic!()
    };
    assert!(matches!(
        execution::rejected_dispatch_result(
            result.clone(),
            stable::CallFailure::NotExecuted.into()
        ),
        Err(Error::Unavailable(_))
    ));
    // Retrying uses the same grant and consumes no additional device/execution sequence.
    let before = s.clone();
    let retry = derive(&mut s, p(4), &request, at + 1).unwrap();
    assert_eq!(retry, execution);
    assert_eq!(s, before);
    for outcome in [
        ExecutionOutcome::Executing,
        ExecutionOutcome::Unknown(Error::ExecutionUnknown),
        ExecutionOutcome::Failed(Error::Forbidden),
        ExecutionOutcome::ResultExpired,
    ] {
        let current = ExecutionResult {
            outcome,
            ..result.clone()
        };
        assert_eq!(
            execution::rejected_dispatch_result(current.clone(), Error::QuotaExceeded),
            Ok(current)
        );
    }
    let current = completed(&request, &s);
    assert_eq!(
        execution::rejected_dispatch_result(current.clone(), Error::QuotaExceeded),
        Ok(current)
    );
}

fn principal_apply(
    s: &mut AccountState,
    principal: &mut Option<crate::principal::AgentPrincipal>,
    command: AccountCommand,
    time: u64,
) -> Result<OperationReceipt> {
    let m = mutation(s, command, 1, time);
    crate::principal::apply(s, principal, p(1), &m, time)
}

fn restricted() -> dmsg_types::agent::DelegationAuthority {
    dmsg_types::agent::DelegationAuthority::Restricted {
        scopes: vec!["message.draft".into()],
        audiences: vec!["https://dmsg.net".into()],
    }
}

/// A registration whose proof is signed by controller key `key` after the
/// approval's request ID is known.
fn register(
    s: &AccountState,
    generation: u32,
    key: u8,
    supersedes: Vec<u32>,
    delegation: dmsg_types::agent::DelegationAuthority,
    time: u64,
) -> AccountMutation {
    let request_id = digest("test", &(s.account_version, 1u8));
    let command = AccountCommand::RegisterController {
        generation,
        public_key: sk(key).verifying_key().to_bytes().into(),
        name: Some(format!("dMsg signer #{generation}")),
        delegation: delegation.clone(),
        supersedes: supersedes.clone(),
        proof: sk(key)
            .sign(
                controller_pop_message(
                    s.home_user,
                    &s.account_id,
                    generation,
                    &delegation,
                    &supersedes,
                    request_id,
                )
                .as_slice(),
            )
            .to_bytes()
            .into(),
    };
    let m = mutation(s, command, 1, time);
    assert_eq!(m.approval.request_id, request_id);
    m
}

#[test]
fn principal_changes_are_monotonic_bounded_and_epoch_neutral() {
    use dmsg_types::agent::*;
    let mut s = fixture();
    let mut principal = None;
    let enable = AccountCommand::EnablePrincipal {
        principal_type: PrincipalType::Person,
    };
    let epoch = s.security_epoch;
    let m = register(&s, 1, 20, vec![], restricted(), 10);
    assert_eq!(
        crate::principal::apply(&mut s, &mut principal, p(1), &m, 10),
        Err(Error::NotFound)
    );
    principal_apply(&mut s, &mut principal, enable.clone(), 10).unwrap();
    let state = principal.as_ref().unwrap().state.clone();
    assert_eq!((state.version, state.updated_at), (1, 10));
    assert_eq!(s.principal_updated_at, Some(10));
    assert_eq!(s.security_epoch, epoch);
    assert_eq!(
        principal_apply(&mut s, &mut principal, enable, 10),
        Err(Error::VersionConflict)
    );
    // Generations are allocated in order; the same millisecond still advances time.
    let m = register(&s, 2, 20, vec![], restricted(), 10);
    assert_eq!(
        crate::principal::apply(&mut s, &mut principal, p(1), &m, 10),
        Err(Error::VersionConflict)
    );
    // The proof must come from the registered key and bind this approval.
    let mut forged = register(&s, 1, 20, vec![], restricted(), 10);
    if let AccountCommand::RegisterController { proof, .. } = &mut forged.command {
        *proof = sk(21).sign(b"x").to_bytes().into();
    }
    forged.approval.signature = sk(1)
        .sign(
            approval_message(
                s.home_user,
                &s.account_id,
                "dmsg/account/v2",
                &(&forged.expected_version, &forged.command),
                &forged.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    let before = (s.clone(), principal.clone());
    assert_eq!(
        crate::principal::apply(&mut s, &mut principal, p(1), &forged, 10),
        Err(Error::IntegrityFailed)
    );
    assert_eq!((s.clone(), principal.clone()), before);
    let m = register(&s, 1, 20, vec![], restricted(), 10);
    let receipt = crate::principal::apply(&mut s, &mut principal, p(1), &m, 10).unwrap();
    let before = (s.clone(), principal.clone());
    assert_eq!(
        crate::principal::apply(&mut s, &mut principal, p(1), &m, 10),
        Ok(receipt)
    );
    assert_eq!((s.clone(), principal.clone()), before);
    let m = register(&s, 2, 21, vec![1], restricted(), 10);
    crate::principal::apply(&mut s, &mut principal, p(1), &m, 10).unwrap();
    let state = &principal.as_ref().unwrap().state;
    assert_eq!(
        state
            .controllers
            .iter()
            .map(|c| c.valid_from)
            .collect::<Vec<_>>(),
        vec![11, 12]
    );
    assert_eq!((state.version, state.updated_at), (3, 12));
    // A successor must name an earlier generation; a key appears once.
    for m in [
        register(&s, 3, 22, vec![3], restricted(), 12),
        register(&s, 3, 21, vec![], restricted(), 12),
    ] {
        assert!(crate::principal::apply(&mut s, &mut principal, p(1), &m, 12).is_err());
    }

    principal_apply(
        &mut s,
        &mut principal,
        AccountCommand::RetireController { generation: 1 },
        20,
    )
    .unwrap();
    assert_eq!(
        principal_apply(
            &mut s,
            &mut principal,
            AccountCommand::RetireController { generation: 1 },
            21
        ),
        Err(Error::VersionConflict)
    );
    let compromise = |invalid_from| AccountCommand::MarkControllerCompromised {
        generation: 1,
        invalid_from,
    };
    for invalid_from in [10, 21] {
        assert!(principal_apply(&mut s, &mut principal, compromise(invalid_from), 22).is_err());
    }
    principal_apply(&mut s, &mut principal, compromise(15), 22).unwrap();
    assert!(principal_apply(&mut s, &mut principal, compromise(16), 23).is_err());
    principal_apply(&mut s, &mut principal, compromise(11), 23).unwrap();
    // Compromising a current key retires it at the commit time.
    principal_apply(
        &mut s,
        &mut principal,
        AccountCommand::MarkControllerCompromised {
            generation: 2,
            invalid_from: 12,
        },
        30,
    )
    .unwrap();
    principal_apply(
        &mut s,
        &mut principal,
        AccountCommand::RenameController {
            generation: 1,
            name: None,
        },
        31,
    )
    .unwrap();
    let state = &principal.as_ref().unwrap().state;
    assert_eq!(
        (
            state.controllers[0].retired_at,
            state.controllers[0].invalid_from,
            state.controllers[0].name.clone()
        ),
        (Some(20), Some(11), None)
    );
    assert_eq!(
        (
            state.controllers[1].retired_at,
            state.controllers[1].invalid_from
        ),
        (Some(30), Some(12))
    );
    assert_eq!(s.principal_updated_at, Some(state.updated_at));
    assert_eq!(s.security_epoch, epoch);
    // The ordinary account path never applies principal commands.
    let m = register(&s, 3, 22, vec![], restricted(), 40);
    assert!(matches!(
        account::apply(&mut s, p(1), &m, 40, p(7)),
        Err(Error::InvalidInput(_))
    ));
    // At most eight current controllers.
    for generation in 3..=10 {
        let m = register(&s, generation, generation as u8 + 30, vec![], restricted(), 40);
        crate::principal::apply(&mut s, &mut principal, p(1), &m, 40).unwrap();
    }
    let m = register(&s, 11, 60, vec![], restricted(), 40);
    assert_eq!(
        crate::principal::apply(&mut s, &mut principal, p(1), &m, 40),
        Err(Error::QuotaExceeded)
    );
}

#[test]
fn principal_document_budget_rejects_registration_before_commit_and_reserves_safety_changes() {
    use dmsg_types::agent::*;
    let mut s = fixture();
    let mut principal = None;
    principal_apply(
        &mut s,
        &mut principal,
        AccountCommand::EnablePrincipal {
            principal_type: PrincipalType::Person,
        },
        10,
    )
    .unwrap();
    let wide = DelegationAuthority::Restricted {
        scopes: (0..8).map(|i| format!("{i}{}", "s".repeat(63))).collect(),
        audiences: (0..4)
            .map(|i| format!("https://{i}{}", "a".repeat(247)))
            .collect(),
    };
    let mut rejected = false;
    for generation in 1..=MAX_CONTROLLER_RECORDS as u32 {
        if generation > 1 {
            principal_apply(
                &mut s,
                &mut principal,
                AccountCommand::RetireController {
                    generation: generation - 1,
                },
                10,
            )
            .unwrap();
        }
        let m = register(
            &s,
            generation,
            generation as u8 + 20,
            (1..generation).collect(),
            wide.clone(),
            10,
        );
        let before = (s.clone(), principal.clone());
        if crate::principal::apply(&mut s, &mut principal, p(1), &m, 10)
            == Err(Error::QuotaExceeded)
        {
            assert_eq!((s.clone(), principal.clone()), before);
            rejected = true;
            break;
        }
        assert_eq!(
            principal.as_ref().unwrap().state.controllers.len(),
            generation as usize
        );
    }
    assert!(rejected);
    let records = principal.as_ref().unwrap().state.controllers.clone();
    for c in records {
        principal_apply(
            &mut s,
            &mut principal,
            AccountCommand::MarkControllerCompromised {
                generation: c.generation,
                invalid_from: c.valid_from,
            },
            10,
        )
        .unwrap();
        principal_apply(
            &mut s,
            &mut principal,
            AccountCommand::RenameController {
                generation: c.generation,
                name: Some("\"".repeat(64)),
            },
            10,
        )
        .unwrap();
    }
    let config = DirectoryInit {
        environment: Environment::Local,
        issuer_namespace: NAMESPACE.into(),
        user_homes: vec![p(1)],
        principal_origin: "https://id.dmsg.test".into(),
        controller_source: "https://dmsg.test".into(),
        delegation_query_url: "https://agents.dmsg.test/query".into(),
        profile_url_prefix: "https://dmsg.test/u/".into(),
        custom_domains: vec![],
        governance: p(9),
    };
    let document = dmsg_protocol::agent::render_principal_document(
        &config,
        &s.account_id,
        &principal.unwrap().state,
    )
    .unwrap();
    assert!(document.len() <= dmsg_protocol::agent::MAX_PRINCIPAL_DOCUMENT_BYTES);
}

#[test]
fn admission_tickets_bind_the_home_the_caller_and_a_short_deadline() {
    let issuer = sk(30);
    let key: Hash = issuer.verifying_key().to_bytes().into();
    let ticket = |home: Principal, caller: Principal, expires_at: u64| AdmissionTicket {
        expires_at,
        signature: issuer
            .sign(account_admission_message(home, caller, expires_at).as_slice())
            .to_bytes()
            .into(),
    };
    let check = |t: Option<&AdmissionTicket>, now: u64| {
        account::check_admission(Some(&key), p(5), p(1), t, now)
    };
    // Without a key any caller is admitted, with or without a ticket.
    assert_eq!(account::check_admission(None, p(5), p(1), None, 1), Ok(()));
    assert_eq!(check(Some(&ticket(p(5), p(1), MINUTE)), 1), Ok(()));
    assert_eq!(check(None, 1), Err(Error::Forbidden));
    // Another home's or another caller's ticket does not verify.
    assert_eq!(
        check(Some(&ticket(p(6), p(1), MINUTE)), 1),
        Err(Error::IntegrityFailed)
    );
    assert_eq!(
        check(Some(&ticket(p(5), p(2), MINUTE)), 1),
        Err(Error::IntegrityFailed)
    );
    // Expired tickets and deadlines past the ten-minute limit are refused.
    assert_eq!(
        check(Some(&ticket(p(5), p(1), MINUTE)), MINUTE),
        Err(Error::Expired)
    );
    assert_eq!(
        check(Some(&ticket(p(5), p(1), MAX_ADMISSION_TTL_MS + 2)), 1),
        Err(Error::Expired)
    );
}

#[test]
fn a_login_binds_only_by_accepting_a_live_approval() {
    let mut s = fixture();
    let nonce = Hash::new([4; 32]);
    let approve = |s: &mut AccountState, principal: Principal, nonce: Hash, at: u64| {
        apply(s, AccountCommand::BindAuth { principal, nonce }, at)
    };
    // The approval records the login without binding it or changing the epoch.
    let epoch = s.security_epoch;
    approve(&mut s, p(4), nonce, 10).unwrap();
    assert!(!s.auth_bindings.contains(&p(4)));
    assert_eq!(s.security_epoch, epoch);
    assert_eq!(s.pending_bindings.len(), 1);
    // Only the approved login with the approved nonce, before the deadline.
    let before = s.clone();
    assert_eq!(
        account::accept_binding(&mut s, p(3), nonce, 11),
        Err(Error::NotFound)
    );
    assert_eq!(
        account::accept_binding(&mut s, p(4), Hash::new([5; 32]), 11),
        Err(Error::NotFound)
    );
    assert_eq!(
        account::accept_binding(&mut s, p(4), nonce, 10 + BINDING_ACCEPT_MS),
        Err(Error::NotFound)
    );
    assert_eq!(s, before);
    let version = s.account_version;
    account::accept_binding(&mut s, p(4), nonce, 11).unwrap();
    assert!(s.auth_bindings.contains(&p(4)));
    assert_eq!(
        (s.security_epoch, s.account_version),
        (epoch + 1, version + 1)
    );
    assert!(s.pending_bindings.is_empty());

    // Approving the same login again replaces it; the account holds at most
    // MAX_PENDING_BINDINGS approvals.
    for n in 0..MAX_PENDING_BINDINGS as u8 {
        approve(&mut s, p(20 + n), nonce, 20).unwrap();
    }
    approve(&mut s, p(20), Hash::new([6; 32]), 21).unwrap();
    assert_eq!(s.pending_bindings.len(), MAX_PENDING_BINDINGS);
    assert_eq!(approve(&mut s, p(30), nonce, 22), Err(Error::QuotaExceeded));
    // Expired approvals free their slots.
    approve(&mut s, p(30), nonce, 21 + BINDING_ACCEPT_MS).unwrap();
    assert_eq!(s.pending_bindings.len(), 1);
    // Any security-epoch change withdraws every outstanding approval.
    apply(
        &mut s,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy::default(),
        },
        30 + BINDING_ACCEPT_MS,
    )
    .unwrap();
    assert!(s.pending_bindings.is_empty());
    assert_eq!(
        account::accept_binding(&mut s, p(30), nonce, 31 + BINDING_ACCEPT_MS),
        Err(Error::NotFound)
    );
}

#[test]
fn accounts_keep_only_the_latest_operation_receipts() {
    let mut s = fixture();
    let mut receipts = vec![];
    for at in 1..=MAX_OPERATION_RECEIPTS as u64 + 4 {
        let policy = SensitivePolicy::default();
        receipts.push(apply(&mut s, AccountCommand::SetPolicy { policy }, at).unwrap());
    }
    assert_eq!(s.operations, receipts[4..]);
}
