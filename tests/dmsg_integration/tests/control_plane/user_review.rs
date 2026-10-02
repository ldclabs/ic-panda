use super::*;

#[test]
fn untrusted_service_is_rejected_before_commerce() {
    use dmsg_types::{integration::*, membership::Beneficiary};
    let f = Fixture::new();
    let mut expected = ApplicationApproval {
        version: 1,
        environment: Environment::Local,
        app_id: "sample".into(),
        app_config_version: 1,
        origin: "https://sample.test".into(),
        approving_account: AccountId([1; 12]),
        service: f.commerce,
        beneficiary: Beneficiary {
            authority_canister: f.user,
            product_id: "sample".into(),
            subject_schema: "dmsg-account-v1".into(),
            subject_bytes: vec![1; 12].into(),
        },
        actor: person(1),
        purpose: ApprovalPurpose::CashCheckout,
        action_digest: Hash::new([1; 32]),
        operation_id: Hash::new([2; 32]),
        nonce: Hash::new([3; 32]),
        expires_at_ms: time(&f.ic) + MINUTE,
    };
    f.ic.stop_canister(f.commerce, None).unwrap();
    let denied: Result<ApplicationAuthorization> = update(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "verify_application_authorization",
        (Hash::new([4; 32]), expected.clone()),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    expected.service = Principal::anonymous();
    let reached: Result<ApplicationAuthorization> = update(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "verify_application_authorization",
        (Hash::new([4; 32]), expected),
    );
    assert_eq!(reached, Err(Error::Forbidden));
}

#[test]
fn recovery_retries_preserve_pending_and_completed_results() {
    let f = Fixture::new();
    let id = f.create(1);
    f.recoverable(1, &id);
    let s = f.account_id(1, &id);
    let at = time(&f.ic);
    let request = RecoveryRequest {
        op_id: Hash::new([77; 32]),
        new_auth: person(4),
        device: device(4),
        generation: s.recovery.as_ref().unwrap().generation,
        expires_at: at + 2 * DAY,
    };
    let signature: ByteBuf = key(70)
        .sign(
            digest(
                "dmsg/recovery-request/v1",
                &(f.user, &id, s.recovery_nonce, &request),
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    let proof: ByteBuf = key(4)
        .sign(digest("dmsg/recovery-device/v1", &(f.user, &id, &request)).as_slice())
        .to_bytes()
        .to_vec()
        .into();
    let args = (id, request.clone(), signature, proof);
    let first: Result<()> = update(&f.ic, f.user, person(4), "request_recovery", args.clone());
    first.unwrap();
    f.ic.advance_time(Duration::from_millis(DAY + MINUTE));
    let retry: Result<()> = update(&f.ic, f.user, person(4), "request_recovery", args);
    assert_eq!(retry, Ok(()));
    let complete: Result<()> = update(
        &f.ic,
        f.user,
        person(4),
        "complete_recovery",
        (id, request.op_id),
    );
    complete.unwrap();
    let before = f.account_id(4, &id);
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let retry: Result<()> = update(
        &f.ic,
        f.user,
        person(4),
        "complete_recovery",
        (id, request.op_id),
    );
    assert_eq!(retry, Ok(()));
    assert_eq!(f.account_id(4, &id), before);
    let wrong: Result<()> = update(
        &f.ic,
        f.user,
        person(5),
        "complete_recovery",
        (id, request.op_id),
    );
    assert_eq!(wrong, Err(Error::AuthRequired));
    assert_eq!(f.account_id(4, &id).auth_bindings, vec![person(4)]);
}

#[test]
fn expired_bindings_release_capacity_without_external_cleanup() {
    let f = Fixture::new();
    let id = f.create(1);
    let at = time(&f.ic);
    for n in 0..1024u64 {
        let caller = Principal::self_authenticating(n.to_be_bytes());
        let result: Result<()> = update(
            &f.ic,
            f.user,
            caller,
            "begin_auth_binding",
            (id, Hash::new([1; 32]), at + 5 * MINUTE),
        );
        result.unwrap();
    }
    let denied: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "begin_auth_binding",
        (id, Hash::new([2; 32]), time(&f.ic) + MINUTE),
    );
    assert_eq!(denied, Err(Error::QuotaExceeded));
    // Renew one entry so cleanup must preserve its exact binding.
    f.ic.advance_time(Duration::from_millis(4 * MINUTE));
    let survivor = Principal::self_authenticating(0u64.to_be_bytes());
    let renew: Result<()> = update(
        &f.ic,
        f.user,
        survivor,
        "begin_auth_binding",
        (id, Hash::new([3; 32]), time(&f.ic) + 5 * MINUTE),
    );
    renew.unwrap();
    f.ic.advance_time(Duration::from_millis(2 * MINUTE));
    let admitted: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "begin_auth_binding",
        (id, Hash::new([2; 32]), time(&f.ic) + MINUTE),
    );
    admitted.unwrap();
    f.mutate(
        1,
        &id,
        AccountCommand::BindAuth {
            principal: survivor,
            nonce: Hash::new([3; 32]),
        },
    )
    .unwrap();
    assert!(f.account_id(1, &id).auth_bindings.contains(&survivor));
}

#[test]
fn independent_cleanup_preserves_pending_executions_billing_and_replay_guards() {
    let f = Fixture::new();
    let id = f.create(1);
    f.recoverable(1, &id);
    let ready: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    ready.unwrap();
    let key = f.key_ref(&id, SigningPurpose::Statement, SigningAlgorithm::Ed25519);
    let signed = user_tests::statement_request(&f, &id, key.clone(), 80_000_000_000);
    let done: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (signed.clone(),));
    assert_eq!(done.unwrap().status(), ExecutionStatus::Completed);
    let failed = user_tests::statement_request(&f, &id, key.clone(), 1);
    let done: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (failed,));
    assert_eq!(done.unwrap().status(), ExecutionStatus::Failed);
    f.ic.stop_canister(f.cose, None).unwrap();
    let pending = user_tests::statement_request(&f, &id, key, 1);
    let _: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (pending.clone(),));
    let pending_result: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (id, pending.approval.request_id),
    );
    assert!(!pending_result.as_ref().unwrap().is_terminal());
    let month = dmsg_protocol::billing::month_utc(time(&f.ic)).unwrap();
    let usage = || {
        query::<_, Result<dmsg_types::billing::ExecutionUsage>>(
            &f.ic,
            f.user,
            person(1),
            "get_execution_usage",
            (id, month),
        )
        .unwrap()
    };
    let before_usage = usage();
    let before_account = f.account_id(1, &id);
    let early: Result<u32> = update(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "prune_executions",
        (id,),
    );
    assert_eq!(early, Ok(0));
    f.ic.advance_time(Duration::from_millis(2 * DAY));
    let removed: Result<u32> = update(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "prune_executions",
        (id,),
    );
    assert_eq!(removed, Ok(2));
    let again: Result<u32> = update(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "prune_executions",
        (id,),
    );
    assert_eq!(again, Ok(0));
    assert_eq!(f.account_id(1, &id), before_account);
    assert_eq!(usage(), before_usage);
    let current: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (id, pending.approval.request_id),
    );
    assert_eq!(current, pending_result);
    let replay: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "sign", (signed.clone(),));
    assert!(matches!(
        replay,
        Err(Error::IdempotencyConflict | Error::ResultExpired)
    ));
    for _ in 0..2 {
        let receipt: Result<CertifiedBatch> = query(
            &f.ic,
            f.user,
            person(1),
            "get_execution_receipt",
            (id, signed.approval.request_id),
        );
        let batch = receipt.unwrap();
        assert!(batch.entries[0].value.is_none());
        let witness: ic_certification::HashTree =
            cbor2::from_slice(&batch.entries[0].witness).unwrap();
        let cert: ic_certification::Certificate = cbor2::from_slice(&batch.certificate).unwrap();
        assert_eq!(
            cert.tree.lookup_path([
                b"canister".as_slice(),
                f.user.as_slice(),
                b"certified_data".as_slice()
            ]),
            ic_certification::LookupResult::Found(&witness.digest())
        );
        assert_eq!(
            witness.lookup_path([execution_receipt_key(&id, signed.approval.request_id)]),
            ic_certification::LookupResult::Absent
        );
        f.ic.upgrade_canister(
            f.user,
            wasm("dmsg_user"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
    }
}
