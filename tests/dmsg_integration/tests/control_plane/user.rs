use super::*;

pub(super) fn statement_request(
    f: &Fixture,
    id: &AccountId,
    signing_key: SigningKeyRef,
    max_cycles: u128,
) -> SignRequest {
    let s = f.account_id(1, id);
    let sequence = s.devices[&Hash::new([1; 32])].next_sequence;
    let mut request = SignRequest {
        account_id: id.clone(),
        key: signing_key,
        statement: Statement {
            issuer: s.issuer,
            subject: None,
            issued_at: None,
            content: StatementContent::Text("approved statement".into()),
        },
        origin: "https://example.com".into(),
        max_cycles,
        approval: Approval {
            device_id: Hash::new([1; 32]),
            security_epoch: s.security_epoch,
            sequence,
            request_id: execution_request_id(id, s.security_epoch, Hash::new([1; 32]), sequence),
            expires_at: time(&f.ic) + MINUTE,
            signature: ByteBuf::new(),
        },
    };
    request.approval.signature = key(1)
        .sign(
            request
                .clone()
                .into_execution()
                .unwrap()
                .approval_message(f.user)
                .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    request
}

#[test]
fn early_cose_rejection_keeps_its_sequence_until_reconciled() {
    let f = Fixture::new();
    let id = f.create(1);
    f.recoverable(1, &id);
    f.mutate(
        1,
        &id,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy {
                daily_executions: 100,
                ..SensitivePolicy::default()
            },
        },
    )
    .unwrap();
    let public = key(7).verifying_key().to_bytes();
    let fingerprint =
        key_thumbprint(&public_cose_key(&Algorithm::Ed25519, &[], &public).unwrap()).unwrap();
    let first = statement_request(
        &f,
        &id,
        SigningKeyRef {
            algorithm: SigningAlgorithm::Ed25519,
            kid: fingerprint.to_vec().into(),
            public_key_fingerprint: fingerprint,
        },
        100_000_000_000,
    );
    let rejected: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "sign", (first.clone(),));
    assert!(matches!(rejected, Err(Error::Unavailable(_))));
    let account = f.account_id(1, &id);
    let retry: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (first.clone(),));
    assert_eq!(retry, rejected);
    assert_eq!(f.account_id(1, &id), account);
    let month = dmsg_protocol::billing::month_utc(time(&f.ic)).unwrap();
    let usage: Result<dmsg_types::billing::ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage",
        (&id, month),
    );
    assert_eq!(usage.unwrap().held_units, 1);
    let absent: Result<ExecutionResult> = query(
        &f.ic,
        f.cose,
        f.user,
        "get_execution",
        (&id, first.approval.request_id),
    );
    assert_eq!(absent, Err(Error::NotFound));
    let initialized: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    let real_key = f.key_ref(&id, SigningPurpose::Statement, SigningAlgorithm::Ed25519);
    let good = statement_request(&f, &id, real_key.clone(), 100_000_000_000);
    let signed: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (good,));
    assert!(matches!(
        signed.unwrap().outcome,
        ExecutionOutcome::Completed(_)
    ));
    for _ in 3..=64 {
        let r = statement_request(&f, &id, real_key.clone(), 1);
        let result: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (r,));
        assert_eq!(
            result.unwrap().outcome,
            ExecutionOutcome::Failed(Error::QuotaExceeded)
        );
    }
    f.ic.advance_time(Duration::from_millis(2 * DAY));
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let reconcile: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "reconcile_execution",
        (&id, first.approval.request_id),
    );
    assert_eq!(
        reconcile.unwrap().outcome,
        ExecutionOutcome::Failed(Error::Expired)
    );
    let usage: Result<dmsg_types::billing::ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage",
        (&id, month),
    );
    let usage = usage.unwrap();
    assert_eq!(usage.held_units, 0);
    assert_eq!(usage.charged_units, 1);
    let r = statement_request(&f, &id, real_key, 100_000_000_000);
    let result: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (r,));
    assert!(matches!(
        result.unwrap().outcome,
        ExecutionOutcome::Completed(_)
    ));
}

#[test]
fn fresh_handle_approval_renews_the_deadline() {
    let f = Fixture::new();
    let id = f.create(1);
    let intent = HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::Register,
        account_id: id.clone(),
        target_account: None,
        handle: "probehandle".into(),
        expected_version: 0,
        op_id: Hash::new([80; 32]),
        terms_digest: Hash::new([81; 32]),
    };
    f.mutate(
        1,
        &id,
        AccountCommand::AuthorizeHandle {
            intent: intent.clone(),
        },
    )
    .unwrap();
    f.ic.advance_time(Duration::from_secs(30));
    f.mutate(
        1,
        &id,
        AccountCommand::AuthorizeHandle {
            intent: intent.clone(),
        },
    )
    .unwrap();
    f.ic.advance_time(Duration::from_secs(31));
    let result: Result<()> = update(
        &f.ic,
        f.user,
        f.handle,
        "consume_handle_authorization",
        (intent,),
    );
    assert_eq!(result, Ok(()));
}

#[test]
fn policy_changes_invalidate_approvals_without_rotating_content_roots() {
    let f = Fixture::new();
    let id = f.create(1);
    f.recoverable(1, &id);
    let op = Hash::new([82; 32]);
    f.mutate(
        1,
        &id,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id: op,
        },
    )
    .unwrap();
    let generation = f.account_id(1, &id).root_slot.unwrap().generation;
    f.mutate(
        1,
        &id,
        AccountCommand::CommitRoot {
            expected_generation: 0,
            op_id: op,
            root: ContentRootRef {
                generation,
                suite: "dmsg-root-v1".into(),
                home_cose: f.cose,
                derivation_version: 2,
                key_generation: generation,
                bundle_digest: Hash::new([83; 32]),
                recovery_generation: 1,
            },
        },
    )
    .unwrap();
    let before = f.account_id(1, &id);
    assert_eq!(before.vault_write_state, VaultWriteState::Ready);
    let mut policy = before.sensitive_policy;
    policy.daily_executions = 10;
    f.mutate(1, &id, AccountCommand::SetPolicy { policy })
        .unwrap();
    let after = f.account_id(1, &id);
    assert_eq!(after.vault_write_state, VaultWriteState::Ready);
    assert_eq!(after.current_root, before.current_root);
    assert_eq!(after.security_epoch, before.security_epoch + 1);
}

#[test]
fn pruned_results_settle_the_original_month_once() {
    use dmsg_types::billing::ExecutionUsage;
    let f = Fixture::new();
    let id = f.create(1);
    f.recoverable(1, &id);
    let initialized: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    let signing_key = f.key_ref(&id, SigningPurpose::Statement, SigningAlgorithm::Ed25519);
    let first = statement_request(&f, &id, signing_key.clone(), 100_000_000_000);
    let call =
        f.ic.submit_call(
            f.user,
            person(1),
            "sign",
            candid::encode_one(first.clone()).unwrap(),
        )
        .unwrap();
    let mut before_callback = None;
    for _ in 0..30 {
        f.ic.tick();
        let state: Result<ExecutionResult> = query(
            &f.ic,
            f.user,
            person(1),
            "get_execution",
            (&id, first.approval.request_id),
        );
        if state.is_ok_and(|e| e.status() == ExecutionStatus::Authorized) {
            before_callback = Some(f.ic.get_stable_memory(f.user));
            break;
        }
    }
    let before_callback = before_callback.expect("authorized state before callback");
    let month = dmsg_protocol::billing::month_utc(time(&f.ic)).unwrap();
    let completed: Result<ExecutionResult> =
        candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
    assert!(matches!(
        completed.unwrap().outcome,
        ExecutionOutcome::Completed(_)
    ));
    // Simulate a lost success callback by restoring the exact persisted user
    // authorization/hold state. The real COSE completion remains untouched.
    f.ic.set_stable_memory(
        f.user,
        before_callback,
        pocket_ic::common::rest::BlobCompression::NoCompression,
    );
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let held: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (&id, first.approval.request_id),
    );
    assert_eq!(held.unwrap().status(), ExecutionStatus::Authorized);
    f.ic.advance_time(Duration::from_millis(32 * DAY));
    let second = statement_request(&f, &id, signing_key, 100_000_000_000);
    let result: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (second,));
    assert!(matches!(
        result.unwrap().outcome,
        ExecutionOutcome::Completed(_)
    ));
    let missing: Result<ExecutionResult> = query(
        &f.ic,
        f.cose,
        f.user,
        "get_execution",
        (&id, first.approval.request_id),
    );
    assert_eq!(missing, Err(Error::NotFound));
    let before: Result<ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage",
        (&id, month),
    );
    let before = before.unwrap();
    assert_eq!(before.held_units, 1);
    assert_eq!(before.charged_units, 0);
    let reconciled: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "reconcile_execution",
        (&id, first.approval.request_id),
    );
    assert_eq!(reconciled.unwrap().outcome, ExecutionOutcome::ResultExpired);
    let after: Result<ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage",
        (&id, month),
    );
    let after = after.unwrap();
    assert_eq!(after.held_units, 0);
    assert_eq!(after.charged_units, 1);
    let repeated: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "reconcile_execution",
        (&id, first.approval.request_id),
    );
    assert_eq!(repeated.unwrap().outcome, ExecutionOutcome::ResultExpired);
    let unchanged: Result<ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage",
        (&id, month),
    );
    assert_eq!(unchanged.unwrap(), after);
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let certified: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage_certified",
        (&id, month),
    );
    let leaf: ExecutionUsage = cbor2::from_slice(&certified_value(
        &f,
        certified.unwrap(),
        dmsg_protocol::billing::usage_key(&id, month).as_slice(),
    ))
    .unwrap();
    assert_eq!(leaf, after);
}

#[test]
fn entitlement_callback_rechecks_a_concurrent_policy_change() {
    let f = Fixture::new();
    let id = f.create(1);
    f.recoverable(1, &id);
    let initialized: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    let signing_key = f.key_ref(&id, SigningPurpose::Statement, SigningAlgorithm::Ed25519);
    let request = statement_request(&f, &id, signing_key, 100_000_000_000);
    let call =
        f.ic.submit_call(
            f.user,
            person(1),
            "sign",
            candid::encode_one(request.clone()).unwrap(),
        )
        .unwrap();
    // Queue a policy change before the first commercial lease callback can authorize the request.
    let missing: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (&id, request.approval.request_id),
    );
    assert_eq!(missing, Err(Error::ResultExpired));
    f.mutate(
        1,
        &id,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy {
                frozen: true,
                ..SensitivePolicy::default()
            },
        },
    )
    .unwrap();
    let result: Result<ExecutionResult> =
        candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
    assert_eq!(result, Err(Error::Locked));
    let missing: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (&id, request.approval.request_id),
    );
    assert_eq!(missing, Err(Error::ResultExpired));
    let month = dmsg_protocol::billing::month_utc(time(&f.ic)).unwrap();
    let usage: Result<dmsg_types::billing::ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage",
        (&id, month),
    );
    let usage = usage.unwrap();
    assert_eq!(usage.held_units, 0);
    assert_eq!(usage.charged_units, 0);
}

// Small reproducible growth samples, not a production capacity certification.
#[test]
#[ignore = "storage and upgrade comparison for dmsg_user builds"]
fn user_upgrade_profile() {
    let f = Fixture::new();
    let mut accounts = vec![];
    for owner in 1..=64u8 {
        let id = f.create(owner);
        let usage: Result<dmsg_types::billing::ExecutionUsage> = update(
            &f.ic,
            f.user,
            person(owner),
            "refresh_execution_entitlement",
            (&id,),
        );
        usage.unwrap();
        accounts.push((owner, id));
        if [1, 16, 64].contains(&owner) {
            measure_user_upgrade(&f, &accounts, 1);
        }
    }
    for _ in 1..12 {
        f.ic.advance_time(Duration::from_millis(32 * DAY));
        for (owner, id) in &accounts {
            let usage: Result<dmsg_types::billing::ExecutionUsage> = update(
                &f.ic,
                f.user,
                person(*owner),
                "refresh_execution_entitlement",
                (id,),
            );
            usage.unwrap();
        }
    }
    measure_user_upgrade(&f, &accounts, 12);
}

fn measure_user_upgrade(f: &Fixture, accounts: &[(u8, AccountId)], months: usize) {
    let before = f.ic.cycle_balance(f.user);
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let cycles = before - f.ic.cycle_balance(f.user);
    let stable_bytes = f.ic.get_stable_memory(f.user).len();
    let month = dmsg_protocol::billing::month_utc(time(&f.ic)).unwrap();
    let (owner, id) = accounts.last().unwrap();
    let certified: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(*owner),
        "get_execution_usage_certified",
        (id, month),
    );
    let leaf: dmsg_types::billing::ExecutionUsage = cbor2::from_slice(&certified_value(
        f,
        certified.unwrap(),
        dmsg_protocol::billing::usage_key(id, month).as_slice(),
    ))
    .unwrap();
    assert_eq!(leaf.account_id, *id);
    assert_eq!(leaf.month_utc, month);
    println!(
        "user_upgrade accounts={} month_rows={} cycles={cycles} stable_bytes={stable_bytes}",
        accounts.len(),
        accounts.len() * months
    );
}
