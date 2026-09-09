use super::*;
use dmsg_runtime::Budget;
use ed25519_dalek::{Signer, SigningKey};
use std::collections::BTreeMap;
const NAMESPACE: &str = "https://dmsg.test/u/";
fn authorize(
    s: &mut AccountState,
    caller: Principal,
    request: &ExecuteRequest,
    now: u64,
) -> Result<crate::state::AuthorizedExecution> {
    execution::authorize(s, caller, request, now, NAMESPACE)
}
fn signing_key() -> SigningKeyRef {
    let public = sk(7).verifying_key().to_bytes();
    let fp = key_thumbprint(&public_cose_key(&Algorithm::Ed25519, &[], &public).unwrap()).unwrap();
    SigningKeyRef {
        algorithm: SigningAlgorithm::Ed25519,
        kid: fp.to_vec().into(),
        public_key_fingerprint: fp,
    }
}
fn alter_statement(request: &mut ExecuteRequest) {
    if let ExecutionKind::Sign { to_be_signed, .. } = &mut request.kind {
        let mut prepared = parse_signing_input(to_be_signed).unwrap();
        prepared.statement.content = StatementContent::Text("different payload".into());
        *to_be_signed = prepare_cose(&prepared.statement, &prepared.algorithm, &prepared.kid)
            .unwrap()
            .1
            .into();
    }
}
fn completed(s: &AccountState, request: &ExecuteRequest) -> ExecutionResult {
    let ExecutionKind::Sign {
        to_be_signed,
        public_key_fingerprint,
        ..
    } = &request.kind
    else {
        panic!()
    };
    let artifact = finish_cose(
        to_be_signed,
        &sk(7).verifying_key().to_bytes(),
        sk(7).sign(to_be_signed).to_bytes().to_vec(),
    )
    .unwrap();
    ExecutionResult {
        request_id: request.approval.request_id,
        charged_cycles: 1,
        outcome: ExecutionOutcome::Completed(Box::new(ExecutionOutput::Signature {
            key: KeyDescriptor {
                account_id: s.account_id,
                key_id: signing_key().kid,
                purpose: KeyPurpose::Statement,
                algorithm: Algorithm::Ed25519,
                home_cose: s.home_cose,
                master_key_name: "key_1".into(),
                environment: Environment::Local,
                derivation_version: 2,
                key_generation: 1,
                public_key: sk(7).verifying_key().to_bytes().to_vec().into(),
                public_key_fingerprint: *public_key_fingerprint,
            },
            artifact,
        })),
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
fn fixture() -> AccountState {
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
            .to_vec()
            .into(),
    };
    account::create(p(5), p(6), AccountId::new([8; 12]), p(1), &input, 1).unwrap()
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
            signature: ByteBuf::new(),
        },
    };
    m.approval.signature = sk(n)
        .sign(
            approval_message(
                s.home_user,
                s.account_id,
                "dmsg/account/v2",
                &(&m.expected_version, &m.command),
                &m.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    m
}
fn apply(s: &mut AccountState, command: AccountCommand, time: u64) -> Result<OperationReceipt> {
    let m = mutation(s, command, 1, time);
    account::apply(s, p(1), &m, time, p(7))
}
fn initialized() -> AccountState {
    let mut s = fixture();
    s.recovery = Some(RecoveryPolicy {
        generation: 1,
        signing_pub: sk(3).verifying_key().to_bytes().into(),
        hpke_pub: Hash::new([3; 32]),
        delay_ms: DAY,
    });
    s.recovery_checked = true;
    s
}

#[test]
fn root_cas_errors_have_no_partial_writes_and_retry_is_idempotent() {
    let mut s = initialized();
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
    let root = ContentRootRef {
        generation: 1,
        suite: "dmsg-root-v1".into(),
        home_cose: p(6),
        derivation_version: 2,
        key_generation: 1,
        bundle_digest: Hash::new([1; 32]),
        recovery_generation: 1,
    };
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
fn abandoned_root_generation_is_never_reused() {
    let mut s = initialized();
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
    assert_eq!(s.root_slot.unwrap().generation, 2);
}
#[test]
fn content_device_cannot_administer_or_approve_formal_execution() {
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
    let req = execute_request(&s, 2, 1);
    s.recovery_checked = true;
    assert_eq!(authorize(&mut s, p(1), &req, 1), Err(Error::Forbidden));
}
fn execute_request(s: &AccountState, n: u8, time: u64) -> ExecuteRequest {
    let sequence = s.devices[&Hash::new([n; 32])].next_sequence;
    let id = execution_request_id(s.account_id, s.security_epoch, Hash::new([n; 32]), sequence);
    let mut r = SignRequest {
        account_id: s.account_id,
        key: signing_key(),
        origin: "https://example.com".into(),
        statement: Statement {
            issuer: account_issuer(NAMESPACE, s.account_id).unwrap(),
            subject: Some("release".into()),
            issued_at: None,
            content: StatementContent::Text("Approved release v1".into()),
        },
        max_cycles: 100,
        approval: Approval {
            device_id: Hash::new([n; 32]),
            security_epoch: s.security_epoch,
            sequence,
            request_id: id,
            expires_at: time + MINUTE,
            signature: ByteBuf::new(),
        },
    }
    .into_execution()
    .unwrap();
    r.approval.signature = sk(n)
        .sign(r.approval_message(s.home_user).as_slice())
        .to_bytes()
        .to_vec()
        .into();
    r
}
#[test]
fn authorize_revoke_order_and_payload_tampering() {
    let mut s = initialized();
    let r = execute_request(&s, 1, 1);
    let authorized = authorize(&mut s, p(1), &r, 1).unwrap();
    let budget = s.budget.clone();
    assert_eq!(authorize(&mut s, p(1), &r, 1).unwrap(), authorized);
    assert_eq!(s.budget, budget);
    let mut altered = r.clone();
    altered.max_cycles += 1;
    assert_eq!(
        authorize(&mut s, p(1), &altered, 1),
        Err(Error::IdempotencyConflict)
    );
    let mut s = initialized();
    s.devices.get_mut(&Hash::new([1; 32])).unwrap().revoked_at = Some(1);
    assert_eq!(
        authorize(&mut s, p(1), &r, 1),
        Err(Error::DeviceNotApproved)
    );
    let mut s = initialized();
    let mut r = execute_request(&s, 1, 1);
    alter_statement(&mut r);
    assert_eq!(authorize(&mut s, p(1), &r, 1), Err(Error::IntegrityFailed));
    assert_eq!(s.budget, Budget::default());
}
#[test]
fn recovery_dispute_requires_one_fresh_delay_not_unlimited_veto() {
    let mut s = initialized();
    let request = RecoveryRequest {
        op_id: Hash::new([4; 32]),
        new_auth: p(4),
        device: device(4, true),
        generation: 1,
        expires_at: 10 * DAY,
    };
    let signature = sk(3)
        .sign(
            digest(
                "dmsg/recovery-request/v1",
                &(s.home_user, s.account_id, s.recovery_nonce, &request),
            )
            .as_slice(),
        )
        .to_bytes();
    let proof = sk(4)
        .sign(
            digest(
                "dmsg/recovery-device/v1",
                &(s.home_user, s.account_id, &request),
            )
            .as_slice(),
        )
        .to_bytes();
    recovery::begin_recovery(&mut s, &request, &signature, &proof, 1).unwrap();
    apply(
        &mut s,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
            dispute: Hash::new([6; 32]),
        },
        2,
    )
    .unwrap();
    assert_eq!(
        recovery::complete_recovery(&mut s, p(4), DAY + 2),
        Err(Error::Locked)
    );
    let confirmation = RecoveryConfirmation {
        request_id: request.op_id,
        dispute: Hash::new([6; 32]),
        expires_at: DAY + 2 + 2 * DAY,
    };
    let sig = sk(3)
        .sign(
            recovery_confirmation_message(
                s.home_user,
                s.account_id,
                s.recovery_nonce,
                &request,
                &confirmation,
            )
            .as_slice(),
        )
        .to_bytes();
    recovery::reconfirm_recovery(&mut s, &confirmation, &sig, DAY + 2).unwrap();
    let after = s.pending_recovery.as_ref().unwrap().execute_after;
    apply(
        &mut s,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
            dispute: Hash::new([7; 32]),
        },
        DAY + 3,
    )
    .unwrap();
    assert_eq!(s.pending_recovery.as_ref().unwrap().execute_after, after);
    recovery::complete_recovery(&mut s, p(4), after).unwrap();
    assert_eq!(s.auth_bindings, vec![p(4)]);
    assert_eq!(s.devices.len(), 1);
    assert_eq!(s.status, AccountStatus::Active);
}
#[test]
fn budget_failure_is_atomic() {
    let mut budget = Budget::default();
    budget.reserve(1, 10, 1, 10).unwrap();
    let before = budget.clone();
    assert_eq!(budget.reserve(1, 1, 1, 10), Err(Error::QuotaExceeded));
    assert_eq!(budget, before);
}
#[test]
fn snapshot_contains_no_auth_routes() {
    let s = fixture();
    let snapshot = canonical(&s.snapshot(NAMESPACE));
    let map: BTreeMap<String, cbor2::Value> = cbor2::from_slice(&snapshot).unwrap();
    assert!(!map.contains_key("auth_bindings"));
}

#[test]
fn recovery_requester_can_read_and_confirm_beyond_the_original_expiry() {
    let mut s = initialized();
    s.recovery.as_mut().unwrap().delay_ms = 7 * DAY;
    let request = RecoveryRequest {
        op_id: Hash::new([4; 32]),
        new_auth: p(4),
        device: device(4, true),
        generation: 1,
        expires_at: 1 + 14 * DAY,
    };
    let signature = sk(3)
        .sign(
            digest(
                "dmsg/recovery-request/v1",
                &(
                    s.home_user,
                    s.account_id,
                    s.snapshot(NAMESPACE).recovery_nonce,
                    &request,
                ),
            )
            .as_slice(),
        )
        .to_bytes();
    let pop = sk(4)
        .sign(
            digest(
                "dmsg/recovery-device/v1",
                &(s.home_user, s.account_id, &request),
            )
            .as_slice(),
        )
        .to_bytes();
    recovery::begin_recovery(&mut s, &request, &signature, &pop, 1).unwrap();
    apply(
        &mut s,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
            dispute: Hash::new([6; 32]),
        },
        7 * DAY,
    )
    .unwrap();
    let pending = recovery::recovery_request(&s, p(4)).unwrap().unwrap();
    assert_eq!(
        recovery::recovery_request(&s, p(5)),
        Err(Error::AuthRequired)
    );
    assert_eq!(
        s.snapshot(NAMESPACE).pending_recovery_digest,
        Some(digest("dmsg/pending-recovery/v1", &pending))
    );
    let confirmation = RecoveryConfirmation {
        request_id: request.op_id,
        dispute: pending.dispute.unwrap(),
        expires_at: 15 * DAY,
    };
    let sig = sk(3)
        .sign(
            recovery_confirmation_message(
                s.home_user,
                s.account_id,
                s.recovery_nonce,
                &request,
                &confirmation,
            )
            .as_slice(),
        )
        .to_bytes();
    let mut tampered = confirmation.clone();
    tampered.expires_at += 1;
    assert_eq!(
        recovery::reconfirm_recovery(&mut s, &tampered, &sig, 7 * DAY + 2),
        Err(Error::IntegrityFailed)
    );
    recovery::reconfirm_recovery(&mut s, &confirmation, &sig, 7 * DAY + 2).unwrap();
    let deadline = s.pending_recovery.as_ref().unwrap().execute_after;
    assert!(deadline > request.expires_at);
    apply(
        &mut s,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
            dispute: Hash::new([7; 32]),
        },
        7 * DAY + 3,
    )
    .unwrap();
    recovery::reconfirm_recovery(&mut s, &confirmation, &sig, 7 * DAY + 4).unwrap();
    assert_eq!(s.pending_recovery.as_ref().unwrap().execute_after, deadline);
    recovery::complete_recovery(&mut s, p(4), deadline).unwrap();
    assert_eq!(s.auth_bindings, vec![p(4)]);
    assert_eq!(s.snapshot(NAMESPACE).recovery_nonce, 1);
}

#[test]
fn expired_remote_results_release_all_unknown_slots() {
    let mut s = initialized();
    s.sensitive_policy.daily_executions = 100;
    let mut requests = vec![];
    for _ in 0..64 {
        let request = execute_request(&s, 1, 1);
        authorize(&mut s, p(1), &request, 1).unwrap();
        execution::record_execution_response(
            &mut s,
            request.approval.request_id,
            Err(Error::ExecutionUnknown),
        )
        .unwrap();
        requests.push(request);
    }
    let next = execute_request(&s, 1, 2 * DAY);
    assert_eq!(
        authorize(&mut s, p(1), &next, 2 * DAY),
        Err(Error::QuotaExceeded)
    );
    for request in &requests {
        let result = execution::record_execution_response(
            &mut s,
            request.approval.request_id,
            Err(Error::ResultExpired),
        )
        .unwrap();
        assert_eq!(result.status(), ExecutionStatus::ResultExpired);
    }
    authorize(&mut s, p(1), &next, 2 * DAY).unwrap();
    assert_eq!(s.executions.len(), 1);
    assert_eq!(
        authorize(&mut s, p(1), &requests[0], 2 * DAY),
        Err(Error::ResultExpired)
    );
    let completed = completed(&s, &next);
    execution::record_execution_response(&mut s, next.approval.request_id, Ok(completed.clone()))
        .unwrap();
    assert_eq!(
        execution::record_execution_response(
            &mut s,
            next.approval.request_id,
            Err(Error::ExecutionUnknown)
        ),
        Ok(completed)
    );
}

#[test]
fn a_fresh_approval_cannot_repurpose_a_cleaned_request_id() {
    let mut s = initialized();
    let first = execute_request(&s, 1, 1);
    authorize(&mut s, p(1), &first, 1).unwrap();
    s.executions
        .get_mut(&first.approval.request_id)
        .unwrap()
        .result
        .outcome = ExecutionOutcome::ResultExpired;
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
    let next = execute_request(&s, 1, 2 * DAY);
    authorize(&mut s, p(1), &next, 2 * DAY).unwrap();
    assert!(!s.executions.contains_key(&first.approval.request_id));
    let mut reused = execute_request(&s, 1, 2 * DAY + 1);
    reused.approval.request_id = first.approval.request_id;
    alter_statement(&mut reused);
    reused.approval.signature = sk(1)
        .sign(
            approval_message(
                s.home_user,
                s.account_id,
                "dmsg/execute/v3",
                &(&reused.kind, reused.max_cycles),
                &reused.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    assert_eq!(
        authorize(&mut s, p(1), &reused, 2 * DAY + 1),
        Err(Error::IdempotencyConflict)
    );
}
